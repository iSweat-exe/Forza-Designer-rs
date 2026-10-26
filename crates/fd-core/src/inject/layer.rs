//! Layer plausibility checks and the JSON-shape -> game-layer mapping.

use super::layout::Layout;
use super::memory::{is_user_ptr, read_exact, ProcessMemory};
use crate::shapegen::Shape;

/// A layer's fields as read from the game.
struct LayerBytes {
    raw: Vec<u8>,
}

impl LayerBytes {
    fn read(mem: &dyn ProcessMemory, layout: &Layout, ptr: u64) -> Option<Self> {
        if !is_user_ptr(ptr) {
            return None;
        }
        read_exact(mem, ptr, layout.layer_span()).map(|raw| Self { raw })
    }

    fn f32x2(&self, off: u64) -> [f32; 2] {
        let o = off as usize;
        let f = |i: usize| {
            f32::from_le_bytes([
                self.raw[i],
                self.raw[i + 1],
                self.raw[i + 2],
                self.raw[i + 3],
            ])
        };
        [f(o), f(o + 4)]
    }

    fn u8(&self, off: u64) -> u8 {
        self.raw[off as usize]
    }
}

/// Strict plausibility score 0..=5. A *fresh* sphere-template layer scores 5.
/// (Position in canvas range, scale in `(0, 64]`, colour readable, known shape
/// id, mask 0/1.)
pub fn score_layer(mem: &dyn ProcessMemory, layout: &Layout, ptr: u64) -> u8 {
    let Some(l) = LayerBytes::read(mem, layout, ptr) else {
        return 0;
    };
    let mut score = 1; // colour bytes are readable: the whole span was read
    if l.f32x2(layout.position_offset)
        .iter()
        .all(|v| v.is_finite() && (-8192.0..=8192.0).contains(v))
    {
        score += 1;
    }
    if l.f32x2(layout.scale_offset)
        .iter()
        .all(|v| v.is_finite() && *v != 0.0 && v.abs() <= 64.0)
    {
        score += 1;
    }
    let shape = l.u8(layout.shape_id_offset);
    if shape == layout.shape_id_ellipse || shape == layout.shape_id_other {
        score += 1;
    }
    if l.u8(layout.mask_offset) <= 1 {
        score += 1;
    }
    score
}

/// Looser check for groups already identified by C++ type (RTTI): the pointer
/// must lead to readable memory holding finite position and scale floats. Lets
/// us re-inject onto an already painted template.
pub fn loose_valid_layer(mem: &dyn ProcessMemory, layout: &Layout, ptr: u64) -> bool {
    LayerBytes::read(mem, layout, ptr).is_some_and(|l| {
        l.f32x2(layout.position_offset)
            .iter()
            .all(|v| v.is_finite())
            && l.f32x2(layout.scale_offset).iter().all(|v| v.is_finite())
    })
}

/// What gets written into one game layer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayerValues {
    pub position: [f32; 2],
    pub scale: [f32; 2],
    pub rotation: f32,
    pub color: [u8; 4],
    pub shape_id: u8,
}

/// Maps a generated shape to game-space values.
///
/// * Position: `(x, -y)` — the game's Y axis points the other way.
/// * Scale: ellipse radii / 63; rectangles use *full* extents (`2*half / 127`).
/// * Rotation: `(360 - angle) mod 360`.
/// * Colour: RGB with alpha forced to 255 (the game requires opaque layers).
pub fn layer_values(shape: &Shape, layout: &Layout) -> LayerValues {
    let (x, y, scale, angle, ellipse) = match *shape {
        Shape::Ellipse { x, y, rx, ry, .. } => (x, y, [rx, ry], 0.0, true),
        Shape::RotatedEllipse {
            x,
            y,
            rx,
            ry,
            angle,
            ..
        } => (x, y, [rx, ry], angle, true),
        Shape::Circle { x, y, r, .. } => (x, y, [r, r], 0.0, true),
        Shape::Rectangle { x, y, hw, hh, .. } => (x, y, [hw * 2.0, hh * 2.0], 0.0, false),
        Shape::RotatedRectangle {
            x,
            y,
            hw,
            hh,
            angle,
            ..
        } => (x, y, [hw * 2.0, hh * 2.0], angle, false),
    };
    let divisor = if ellipse {
        layout.scale_divisor_ellipse
    } else {
        layout.scale_divisor_other
    };
    let [r, g, b, _] = shape.color();
    LayerValues {
        position: [x, -y],
        scale: [scale[0] / divisor, scale[1] / divisor],
        rotation: (360.0 - angle.rem_euclid(360.0)).rem_euclid(360.0),
        color: [r, g, b, 255],
        shape_id: if ellipse {
            layout.shape_id_ellipse
        } else {
            layout.shape_id_other
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ellipse_maps_radius_over_63_and_negates_y() {
        let s = Shape::RotatedEllipse {
            x: 10.0,
            y: 20.0,
            rx: 63.0,
            ry: 126.0,
            angle: 30.0,
            color: [1, 2, 3, 128],
        };
        let v = layer_values(&s, &Layout::FH6);
        assert_eq!(v.position, [10.0, -20.0]);
        assert_eq!(v.scale, [1.0, 2.0]);
        assert_eq!(v.rotation, 330.0);
        assert_eq!(v.color, [1, 2, 3, 255]);
        assert_eq!(v.shape_id, 102);
    }

    #[test]
    fn rectangle_uses_full_extent_over_127() {
        let s = Shape::Rectangle {
            x: 0.0,
            y: 0.0,
            hw: 63.5,
            hh: 127.0,
            color: [0; 4],
        };
        let v = layer_values(&s, &Layout::FH6);
        assert_eq!(v.scale, [1.0, 2.0]);
        assert_eq!(v.shape_id, 101);
        assert_eq!(v.rotation, 0.0);
    }

    #[test]
    fn circle_uses_ellipse_mapping() {
        let s = Shape::Circle {
            x: 1.0,
            y: 1.0,
            r: 63.0,
            color: [0; 4],
        };
        let v = layer_values(&s, &Layout::FH6);
        assert_eq!((v.scale, v.shape_id), ([1.0, 1.0], 102));
    }

    #[test]
    fn zero_angle_stays_zero_not_360() {
        let s = Shape::RotatedEllipse {
            x: 0.0,
            y: 0.0,
            rx: 1.0,
            ry: 1.0,
            angle: 0.0,
            color: [0; 4],
        };
        assert_eq!(layer_values(&s, &Layout::FH6).rotation, 0.0);
    }
}
