//! Painter's-algorithm rendering of a shape list, plus preview encoding.

use image::codecs::png::PngEncoder;
use image::imageops::{self, FilterType};
use image::{ExtendedColorType, ImageEncoder, RgbaImage};

use super::shape::{Mask, Shape};
use crate::error::{Error, Result};

/// Renders `shapes` in order as fully opaque layers (what Forza draws).
/// `background == None` leaves the canvas transparent. Returns RGBA, `w * h * 4`.
pub fn render_shapes(shapes: &[Shape], w: usize, h: usize, background: Option<[u8; 3]>) -> Vec<u8> {
    let bg = background.map_or([0, 0, 0, 0], |[r, g, b]| [r, g, b, 255]);
    let mut out: Vec<u8> = bg.iter().copied().cycle().take(w * h * 4).collect();
    let mut mask = Mask::default();
    for shape in shapes {
        if !shape.rasterize(w, h, &mut mask) {
            continue;
        }
        let [r, g, b, _] = shape.color();
        let bb = mask.bbox;
        let bw = bb.width();
        for (row, y) in (bb.y0..bb.y1).enumerate() {
            for (col, x) in (bb.x0..bb.x1).enumerate() {
                if mask.data[row * bw + col] {
                    let i = (y * w + x) * 4;
                    out[i..i + 4].copy_from_slice(&[r, g, b, 255]);
                }
            }
        }
    }
    out
}

/// Joins an RGB canvas and its coverage mask into RGBA.
pub fn rgba_from_rgb_alpha(rgb: &[u8], alpha: &[u8]) -> Vec<u8> {
    rgb.chunks_exact(3)
        .zip(alpha)
        .flat_map(|(p, &a)| [p[0], p[1], p[2], a])
        .collect()
}

/// PNG-encodes an RGBA image, downscaling so the longest side is at most `max_side`.
pub fn encode_png(rgba: Vec<u8>, w: u32, h: u32, max_side: u32) -> Result<Vec<u8>> {
    let img = RgbaImage::from_raw(w, h, rgba)
        .ok_or_else(|| Error::Generation("preview buffer has the wrong size".into()))?;
    let img = if max_side > 0 && w.max(h) > max_side {
        let scale = f64::from(max_side) / f64::from(w.max(h));
        let (nw, nh) = (
            ((f64::from(w) * scale) as u32).max(1),
            ((f64::from(h) * scale) as u32).max(1),
        );
        imageops::resize(&img, nw, nh, FilterType::Triangle)
    } else {
        img
    };
    let mut out = Vec::new();
    PngEncoder::new(&mut out)
        .write_image(
            img.as_raw(),
            img.width(),
            img.height(),
            ExtendedColorType::Rgba8,
        )
        .map_err(Error::Image)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn later_shapes_paint_over_earlier_ones_and_are_opaque() {
        let shapes = [
            Shape::Rectangle {
                x: 5.0,
                y: 5.0,
                hw: 4.0,
                hh: 4.0,
                color: [255, 0, 0, 128],
            },
            Shape::Rectangle {
                x: 8.0,
                y: 5.0,
                hw: 1.0,
                hh: 1.0,
                color: [0, 0, 255, 128],
            },
        ];
        let img = render_shapes(&shapes, 12, 12, Some([255, 255, 255]));
        let px = |x: usize, y: usize| &img[(y * 12 + x) * 4..(y * 12 + x) * 4 + 4];
        assert_eq!(px(3, 5), &[255, 0, 0, 255]);
        assert_eq!(px(8, 5), &[0, 0, 255, 255]);
        assert_eq!(px(0, 0), &[255, 255, 255, 255]);
    }

    #[test]
    fn transparent_background_stays_transparent() {
        let img = render_shapes(&[], 3, 3, None);
        assert!(img.iter().all(|&b| b == 0));
    }

    #[test]
    fn png_encoder_downscales_and_emits_a_png() {
        let png = encode_png(vec![255; 100 * 50 * 4], 100, 50, 20).unwrap();
        assert_eq!(&png[..4], &[0x89, b'P', b'N', b'G']);
        let decoded = image::load_from_memory(&png).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (20, 10));
    }
}
