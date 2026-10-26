use std::str::FromStr;

use rand::rngs::SmallRng;
use rand::Rng;
use serde::{Deserialize, Serialize};

pub type Rgba = [u8; 4];

/// A vinyl primitive. Serialises to the same tagged JSON as the Python app
/// (`{"type": "rotated_ellipse", "x": .., ...}`).
///
/// Triangles are intentionally absent: Forza's layer struct has no way to
/// express arbitrary vertices, so they could never be injected.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Shape {
    Ellipse {
        x: f32,
        y: f32,
        rx: f32,
        ry: f32,
        color: Rgba,
    },
    RotatedEllipse {
        x: f32,
        y: f32,
        rx: f32,
        ry: f32,
        angle: f32,
        color: Rgba,
    },
    Circle {
        x: f32,
        y: f32,
        r: f32,
        color: Rgba,
    },
    Rectangle {
        x: f32,
        y: f32,
        hw: f32,
        hh: f32,
        color: Rgba,
    },
    RotatedRectangle {
        x: f32,
        y: f32,
        hw: f32,
        hh: f32,
        angle: f32,
        color: Rgba,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShapeKind {
    Ellipse,
    RotatedEllipse,
    Circle,
    Rectangle,
    RotatedRectangle,
}

impl ShapeKind {
    pub const ALL: [ShapeKind; 5] = [
        Self::Ellipse,
        Self::RotatedEllipse,
        Self::Circle,
        Self::Rectangle,
        Self::RotatedRectangle,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Ellipse => "ellipse",
            Self::RotatedEllipse => "rotated_ellipse",
            Self::Circle => "circle",
            Self::Rectangle => "rectangle",
            Self::RotatedRectangle => "rotated_rectangle",
        }
    }
}

impl FromStr for ShapeKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|k| k.name() == s.trim())
            .ok_or_else(|| format!("unknown shape type {s:?}"))
    }
}

/// Clipped, inclusive-exclusive pixel rectangle.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BBox {
    pub x0: usize,
    pub y0: usize,
    pub x1: usize,
    pub y1: usize,
}

impl BBox {
    pub const EMPTY: Self = Self {
        x0: 0,
        y0: 0,
        x1: 0,
        y1: 0,
    };

    pub fn width(&self) -> usize {
        self.x1 - self.x0
    }
    pub fn height(&self) -> usize {
        self.y1 - self.y0
    }
    pub fn is_empty(&self) -> bool {
        self.x1 <= self.x0 || self.y1 <= self.y0
    }
}

/// Reusable rasterisation buffer: `data[(y - y0) * width + (x - x0)]`.
#[derive(Debug, Default)]
pub struct Mask {
    pub bbox: BBox,
    pub data: Vec<bool>,
}

fn clip_bbox(x0: f32, y0: f32, x1: f32, y1: f32, w: usize, h: usize) -> BBox {
    let cx0 = x0.floor().max(0.0) as usize;
    let cy0 = y0.floor().max(0.0) as usize;
    let cx1 = (x1.ceil().max(0.0) as usize).min(w);
    let cy1 = (y1.ceil().max(0.0) as usize).min(h);
    if cx1 <= cx0 || cy1 <= cy0 {
        BBox::EMPTY
    } else {
        BBox {
            x0: cx0,
            y0: cy0,
            x1: cx1,
            y1: cy1,
        }
    }
}

// ── random helpers ──────────────────────────────────────────────────────────

fn uniform(rng: &mut SmallRng, lo: f32, hi: f32) -> f32 {
    if hi <= lo {
        lo
    } else {
        rng.random_range(lo..hi)
    }
}

/// Standard normal via Box-Muller, scaled by `sigma`.
fn gauss(rng: &mut SmallRng, sigma: f32) -> f32 {
    let u1: f32 = 1.0 - rng.random::<f32>(); // (0, 1]
    let u2: f32 = rng.random::<f32>();
    sigma * (-2.0 * u1.ln()).sqrt() * (std::f32::consts::TAU * u2).cos()
}

fn random_rgb(rng: &mut SmallRng, alpha: u8) -> Rgba {
    [rng.random(), rng.random(), rng.random(), alpha]
}

fn round3(v: f32) -> f32 {
    (v * 1000.0).round() / 1000.0
}

impl Shape {
    pub fn kind(&self) -> ShapeKind {
        match self {
            Self::Ellipse { .. } => ShapeKind::Ellipse,
            Self::RotatedEllipse { .. } => ShapeKind::RotatedEllipse,
            Self::Circle { .. } => ShapeKind::Circle,
            Self::Rectangle { .. } => ShapeKind::Rectangle,
            Self::RotatedRectangle { .. } => ShapeKind::RotatedRectangle,
        }
    }

    pub fn color(&self) -> Rgba {
        match *self {
            Self::Ellipse { color, .. }
            | Self::RotatedEllipse { color, .. }
            | Self::Circle { color, .. }
            | Self::Rectangle { color, .. }
            | Self::RotatedRectangle { color, .. } => color,
        }
    }

    pub fn set_color(&mut self, c: Rgba) {
        match self {
            Self::Ellipse { color, .. }
            | Self::RotatedEllipse { color, .. }
            | Self::Circle { color, .. }
            | Self::Rectangle { color, .. }
            | Self::RotatedRectangle { color, .. } => *color = c,
        }
    }

    /// Copy with geometry rounded to 3 decimals (what gets written to JSON).
    pub fn rounded(&self) -> Self {
        let mut s = self.clone();
        match &mut s {
            Self::Ellipse { x, y, rx, ry, .. } => {
                [x, y, rx, ry].into_iter().for_each(|v| *v = round3(*v))
            }
            Self::RotatedEllipse {
                x,
                y,
                rx,
                ry,
                angle,
                ..
            } => {
                [x, y, rx, ry, angle]
                    .into_iter()
                    .for_each(|v| *v = round3(*v));
            }
            Self::Circle { x, y, r, .. } => [x, y, r].into_iter().for_each(|v| *v = round3(*v)),
            Self::Rectangle { x, y, hw, hh, .. } => {
                [x, y, hw, hh].into_iter().for_each(|v| *v = round3(*v))
            }
            Self::RotatedRectangle {
                x,
                y,
                hw,
                hh,
                angle,
                ..
            } => {
                [x, y, hw, hh, angle]
                    .into_iter()
                    .for_each(|v| *v = round3(*v));
            }
        }
        s
    }

    pub fn bbox(&self, w: usize, h: usize) -> BBox {
        match *self {
            Self::Ellipse { x, y, rx, ry, .. } => {
                clip_bbox(x - rx, y - ry, x + rx + 1.0, y + ry + 1.0, w, h)
            }
            Self::RotatedEllipse { x, y, rx, ry, .. } => {
                let r = rx.max(ry);
                clip_bbox(x - r, y - r, x + r + 1.0, y + r + 1.0, w, h)
            }
            Self::Circle { x, y, r, .. } => clip_bbox(x - r, y - r, x + r + 1.0, y + r + 1.0, w, h),
            Self::Rectangle { x, y, hw, hh, .. } => {
                clip_bbox(x - hw, y - hh, x + hw + 1.0, y + hh + 1.0, w, h)
            }
            Self::RotatedRectangle { x, y, hw, hh, .. } => {
                let r = hw.hypot(hh);
                clip_bbox(x - r, y - r, x + r + 1.0, y + r + 1.0, w, h)
            }
        }
    }

    /// Rasterise into `mask` (binary coverage, pixel centres on the integer grid
    /// like the original). Returns `false` when the shape covers no pixel.
    pub fn rasterize(&self, w: usize, h: usize, mask: &mut Mask) -> bool {
        let bbox = self.bbox(w, h);
        mask.bbox = bbox;
        mask.data.clear();
        if bbox.is_empty() {
            return false;
        }
        mask.data.reserve(bbox.width() * bbox.height());
        let rows = bbox.y0..bbox.y1;
        let cols = || bbox.x0..bbox.x1;
        match *self {
            Self::Ellipse { x, y, rx, ry, .. } => {
                let (irx, iry) = (1.0 / rx.max(1e-6), 1.0 / ry.max(1e-6));
                for py in rows {
                    let dy = (py as f32 - y) * iry;
                    mask.data.extend(cols().map(|px| {
                        let dx = (px as f32 - x) * irx;
                        dx * dx + dy * dy <= 1.0
                    }));
                }
            }
            Self::Circle { x, y, r, .. } => {
                let r2 = r.max(1e-6).powi(2);
                for py in rows {
                    let dy = py as f32 - y;
                    mask.data.extend(cols().map(|px| {
                        let dx = px as f32 - x;
                        dx * dx + dy * dy <= r2
                    }));
                }
            }
            Self::Rectangle { .. } => {
                mask.data.resize(bbox.width() * bbox.height(), true);
            }
            Self::RotatedEllipse {
                x,
                y,
                rx,
                ry,
                angle,
                ..
            } => {
                let (sin_a, cos_a) = angle.to_radians().sin_cos();
                let (irx, iry) = (1.0 / rx.max(1e-6), 1.0 / ry.max(1e-6));
                for py in rows {
                    let yg = py as f32 - y;
                    mask.data.extend(cols().map(|px| {
                        let xg = px as f32 - x;
                        let xr = (cos_a * xg + sin_a * yg) * irx;
                        let yr = (-sin_a * xg + cos_a * yg) * iry;
                        xr * xr + yr * yr <= 1.0
                    }));
                }
            }
            Self::RotatedRectangle {
                x,
                y,
                hw,
                hh,
                angle,
                ..
            } => {
                let (sin_a, cos_a) = angle.to_radians().sin_cos();
                for py in rows {
                    let yg = py as f32 - y;
                    mask.data.extend(cols().map(|px| {
                        let xg = px as f32 - x;
                        let xr = cos_a * xg + sin_a * yg;
                        let yr = -sin_a * xg + cos_a * yg;
                        xr.abs() <= hw && yr.abs() <= hh
                    }));
                }
            }
        }
        true
    }

    /// Random shape. `max_size_frac` caps the largest dimension as a fraction of
    /// the canvas (`None` -> legacy 1/8 of the canvas as half-extent).
    pub fn random(
        kind: ShapeKind,
        rng: &mut SmallRng,
        w: usize,
        h: usize,
        max_size_frac: Option<f32>,
        alpha: u8,
    ) -> Self {
        let (wf, hf) = (w as f32, h as f32);
        let caps = |a: f32, b: f32| -> (f32, f32) {
            match max_size_frac {
                None => ((a / 8.0).max(2.0), (b / 8.0).max(2.0)),
                Some(f) => ((a * f / 2.0).max(2.0), (b * f / 2.0).max(2.0)),
            }
        };
        let color = random_rgb(rng, alpha);
        let x = uniform(rng, 0.0, wf - 1.0);
        let y = uniform(rng, 0.0, hf - 1.0);
        match kind {
            ShapeKind::Ellipse => {
                let (cx, cy) = caps(wf, hf);
                Self::Ellipse {
                    x,
                    y,
                    rx: uniform(rng, 1.0, cx),
                    ry: uniform(rng, 1.0, cy),
                    color,
                }
            }
            ShapeKind::RotatedEllipse => {
                let (cx, cy) = caps(wf, hf);
                Self::RotatedEllipse {
                    x,
                    y,
                    rx: uniform(rng, 1.0, cx),
                    ry: uniform(rng, 1.0, cy),
                    angle: uniform(rng, 0.0, 180.0),
                    color,
                }
            }
            ShapeKind::Circle => {
                let m = wf.min(hf);
                let (c, _) = caps(m, m);
                Self::Circle {
                    x,
                    y,
                    r: uniform(rng, 1.0, c),
                    color,
                }
            }
            ShapeKind::Rectangle => {
                let (cx, cy) = caps(wf, hf);
                Self::Rectangle {
                    x,
                    y,
                    hw: uniform(rng, 1.0, cx),
                    hh: uniform(rng, 1.0, cy),
                    color,
                }
            }
            ShapeKind::RotatedRectangle => {
                let (cx, cy) = caps(wf, hf);
                Self::RotatedRectangle {
                    x,
                    y,
                    hw: uniform(rng, 1.0, cx),
                    hh: uniform(rng, 1.0, cy),
                    angle: uniform(rng, 0.0, 180.0),
                    color,
                }
            }
        }
    }

    /// Mutated copy (small gaussian nudge of position / size / angle).
    pub fn mutate(&self, rng: &mut SmallRng, w: usize, h: usize) -> Self {
        let (wf, hf) = (w as f32, h as f32);
        let mut s = self.clone();
        match &mut s {
            Self::Ellipse { x, y, rx, ry, .. } => match rng.random_range(0..3) {
                0 => nudge_pos(rng, x, y, 16.0, wf, hf),
                1 => nudge_size(rng, rx, ry, 16.0, wf, hf),
                _ => {
                    nudge_pos(rng, x, y, 8.0, wf, hf);
                    nudge_size(rng, rx, ry, 8.0, wf, hf);
                }
            },
            Self::RotatedEllipse {
                x,
                y,
                rx,
                ry,
                angle,
                ..
            } => match rng.random_range(0..4) {
                0 => nudge_pos(rng, x, y, 16.0, wf, hf),
                1 => nudge_size(rng, rx, ry, 16.0, wf, hf),
                2 => *angle = (*angle + gauss(rng, 25.0)).rem_euclid(180.0),
                _ => {
                    nudge_pos(rng, x, y, 8.0, wf, hf);
                    *angle = (*angle + gauss(rng, 15.0)).rem_euclid(180.0);
                }
            },
            Self::Circle { x, y, r, .. } => {
                if rng.random_range(0..2) == 0 {
                    nudge_pos(rng, x, y, 16.0, wf, hf);
                } else {
                    *r = (*r + gauss(rng, 16.0)).clamp(1.0, wf.max(hf));
                }
            }
            Self::Rectangle { x, y, hw, hh, .. } => {
                if rng.random_range(0..2) == 0 {
                    nudge_pos(rng, x, y, 16.0, wf, hf);
                } else {
                    nudge_size(rng, hw, hh, 16.0, wf, hf);
                }
            }
            Self::RotatedRectangle {
                x,
                y,
                hw,
                hh,
                angle,
                ..
            } => match rng.random_range(0..3) {
                0 => nudge_pos(rng, x, y, 16.0, wf, hf),
                1 => nudge_size(rng, hw, hh, 16.0, wf, hf),
                _ => *angle = (*angle + gauss(rng, 25.0)).rem_euclid(180.0),
            },
        }
        s
    }
}

fn nudge_pos(rng: &mut SmallRng, x: &mut f32, y: &mut f32, sigma: f32, w: f32, h: f32) {
    *x = (*x + gauss(rng, sigma)).clamp(0.0, w - 1.0);
    *y = (*y + gauss(rng, sigma)).clamp(0.0, h - 1.0);
}

fn nudge_size(rng: &mut SmallRng, a: &mut f32, b: &mut f32, sigma: f32, w: f32, h: f32) {
    *a = (*a + gauss(rng, sigma)).clamp(1.0, w);
    *b = (*b + gauss(rng, sigma)).clamp(1.0, h);
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;

    use super::*;

    fn count(mask: &Mask) -> usize {
        mask.data.iter().filter(|&&b| b).count()
    }

    #[test]
    fn circle_area_is_close_to_pi_r_squared() {
        let s = Shape::Circle {
            x: 50.0,
            y: 50.0,
            r: 20.0,
            color: [0; 4],
        };
        let mut m = Mask::default();
        assert!(s.rasterize(100, 100, &mut m));
        let area = count(&m) as f32;
        assert!((area - std::f32::consts::PI * 400.0).abs() < 30.0, "{area}");
    }

    #[test]
    fn rotated_ellipse_with_zero_angle_matches_plain_ellipse() {
        let a = Shape::Ellipse {
            x: 30.0,
            y: 40.0,
            rx: 12.0,
            ry: 7.0,
            color: [0; 4],
        };
        let b = Shape::RotatedEllipse {
            x: 30.0,
            y: 40.0,
            rx: 12.0,
            ry: 7.0,
            angle: 0.0,
            color: [0; 4],
        };
        let (mut ma, mut mb) = (Mask::default(), Mask::default());
        a.rasterize(100, 100, &mut ma);
        b.rasterize(100, 100, &mut mb);
        // Rotated bbox is square so compare coverage, not layout.
        assert_eq!(count(&ma), count(&mb));
    }

    #[test]
    fn shape_outside_canvas_has_empty_mask() {
        let s = Shape::Circle {
            x: -50.0,
            y: -50.0,
            r: 5.0,
            color: [0; 4],
        };
        let mut m = Mask::default();
        assert!(!s.rasterize(100, 100, &mut m));
    }

    #[test]
    fn mutation_keeps_geometry_in_bounds() {
        let mut rng = SmallRng::seed_from_u64(7);
        let mut s = Shape::random(
            ShapeKind::RotatedEllipse,
            &mut rng,
            200,
            100,
            Some(0.3),
            255,
        );
        for _ in 0..500 {
            s = s.mutate(&mut rng, 200, 100);
            let Shape::RotatedEllipse {
                x,
                y,
                rx,
                ry,
                angle,
                ..
            } = s
            else {
                unreachable!()
            };
            assert!((0.0..=199.0).contains(&x) && (0.0..=99.0).contains(&y));
            assert!(rx >= 1.0 && ry >= 1.0 && (0.0..180.0).contains(&angle));
        }
    }

    #[test]
    fn serde_uses_python_compatible_tags() {
        let s = Shape::RotatedRectangle {
            x: 1.0,
            y: 2.0,
            hw: 3.0,
            hh: 4.0,
            angle: 5.0,
            color: [1, 2, 3, 4],
        };
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["type"], "rotated_rectangle");
        assert_eq!(v["color"], serde_json::json!([1, 2, 3, 4]));
    }

    #[test]
    fn kind_names_round_trip() {
        for k in ShapeKind::ALL {
            assert_eq!(k.name().parse::<ShapeKind>().unwrap(), k);
        }
        assert!("triangle".parse::<ShapeKind>().is_err());
    }
}
