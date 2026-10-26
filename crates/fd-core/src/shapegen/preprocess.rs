//! Source image -> fitting target.
//!
//! The engine always works with an alpha mask: content = 255, and an 8 % ring of
//! transparent "buffer" around it. Forza treats layers touching the canvas edge
//! as unbounded (smears/corner artefacts), so the engine must never place paint
//! there.

use std::path::Path;

use image::imageops::{self, FilterType};
use image::{DynamicImage, GrayImage, Rgb, RgbImage};

use crate::error::{Error, Result};

const BUFFER_FRAC: f64 = 0.08;

/// What the engine fits against.
#[derive(Debug, Clone)]
pub struct Target {
    pub width: usize,
    pub height: usize,
    /// Row-major RGB, `width * height * 3`.
    pub rgb: Vec<u8>,
    /// Row-major coverage, `width * height`. 0 = never paint here.
    pub alpha: Vec<u8>,
}

pub fn load_target(path: &Path, sticker_mode: bool, max_resolution: u32) -> Result<Target> {
    let img = image::open(path).map_err(|e| match e {
        image::ImageError::IoError(io) => Error::io(path, io),
        other => Error::Image(other),
    })?;
    Ok(build_target(&img, sticker_mode, max_resolution))
}

/// Source-image summary for the UI.
#[derive(Debug, Clone)]
pub struct ImageSummary {
    pub width: u32,
    pub height: u32,
    pub has_alpha: bool,
    /// PNG thumbnail (longest side <= `max_side`).
    pub thumbnail_png: Vec<u8>,
}

pub fn describe_image(path: &Path, max_side: u32) -> Result<ImageSummary> {
    let img = image::open(path).map_err(|e| match e {
        image::ImageError::IoError(io) => Error::io(path, io),
        other => Error::Image(other),
    })?;
    let (width, height) = (img.width(), img.height());
    let thumb = img.thumbnail(max_side, max_side).to_rgba8();
    let (tw, th) = thumb.dimensions();
    Ok(ImageSummary {
        width,
        height,
        has_alpha: img.color().has_alpha(),
        thumbnail_png: super::render::encode_png(thumb.into_raw(), tw, th, 0)?,
    })
}

pub fn build_target(img: &DynamicImage, sticker_mode: bool, max_resolution: u32) -> Target {
    let has_alpha = img.color().has_alpha();
    let (mut rgb, mut alpha): (RgbImage, Option<GrayImage>) = if has_alpha && sticker_mode {
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        let mut c = RgbImage::new(w, h);
        let mut a = GrayImage::new(w, h);
        for (x, y, p) in rgba.enumerate_pixels() {
            c.put_pixel(x, y, Rgb([p[0], p[1], p[2]]));
            a.put_pixel(x, y, image::Luma([p[3]]));
        }
        (c, Some(a))
    } else if has_alpha {
        (composite_on_white(img), None)
    } else {
        (img.to_rgb8(), None)
    };

    // Non-sticker: square white canvas so the (square) vinyl canvas fills evenly.
    if !sticker_mode && rgb.width() != rgb.height() {
        let side = rgb.width().max(rgb.height());
        let mut square = RgbImage::from_pixel(side, side, Rgb([255, 255, 255]));
        let (ox, oy) = ((side - rgb.width()) / 2, (side - rgb.height()) / 2);
        imageops::replace(&mut square, &rgb, i64::from(ox), i64::from(oy));
        rgb = square;
    }

    // Transparent buffer ring around everything.
    let (src_w, src_h) = rgb.dimensions();
    let pad = ((f64::from(src_w.max(src_h)) * BUFFER_FRAC).round() as u32).max(8);
    let (new_w, new_h) = (src_w + 2 * pad, src_h + 2 * pad);
    let content_alpha = alpha
        .take()
        .unwrap_or_else(|| GrayImage::from_pixel(src_w, src_h, image::Luma([255])));
    let mut padded_alpha = GrayImage::new(new_w, new_h);
    imageops::replace(
        &mut padded_alpha,
        &content_alpha,
        i64::from(pad),
        i64::from(pad),
    );
    let mut buffered = RgbImage::from_pixel(new_w, new_h, Rgb([255, 255, 255]));
    imageops::replace(&mut buffered, &rgb, i64::from(pad), i64::from(pad));

    let (mut rgb, mut alpha) = (buffered, padded_alpha);
    let longest = new_w.max(new_h);
    if longest > max_resolution {
        let scale = f64::from(max_resolution) / f64::from(longest);
        let nw = ((f64::from(new_w) * scale) as u32).max(1);
        let nh = ((f64::from(new_h) * scale) as u32).max(1);
        rgb = imageops::resize(&rgb, nw, nh, FilterType::Lanczos3);
        alpha = imageops::resize(&alpha, nw, nh, FilterType::Lanczos3);
    }

    Target {
        width: rgb.width() as usize,
        height: rgb.height() as usize,
        rgb: rgb.into_raw(),
        alpha: alpha.into_raw(),
    }
}

fn composite_on_white(img: &DynamicImage) -> RgbImage {
    let rgba = img.to_rgba8();
    let mut out = RgbImage::new(rgba.width(), rgba.height());
    for (x, y, p) in rgba.enumerate_pixels() {
        let a = u32::from(p[3]);
        let blend = |c: u8| ((u32::from(c) * a + 255 * (255 - a) + 127) / 255) as u8;
        out.put_pixel(x, y, Rgb([blend(p[0]), blend(p[1]), blend(p[2])]));
    }
    out
}

#[cfg(test)]
mod tests {
    use image::{Rgba, RgbaImage};

    use super::*;

    fn solid(w: u32, h: u32, px: Rgba<u8>) -> DynamicImage {
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(w, h, px))
    }

    fn at(t: &Target, x: usize, y: usize) -> u8 {
        t.alpha[y * t.width + x]
    }

    #[test]
    fn adds_transparent_buffer_ring_and_square_pads() {
        let t = build_target(&solid(100, 50, Rgba([10, 20, 30, 255])), false, 4096);
        // 100x50 -> 100x100 square -> pad 8 (8 % of 100) per side.
        assert_eq!((t.width, t.height), (116, 116));
        assert_eq!(at(&t, 0, 0), 0);
        assert_eq!(at(&t, 8, 8), 255);
        assert_eq!(at(&t, 115, 115), 0);
    }

    #[test]
    fn downscales_to_max_resolution() {
        let t = build_target(&solid(1000, 1000, Rgba([0, 0, 0, 255])), false, 400);
        assert_eq!(t.width.max(t.height), 400);
        assert_eq!(t.rgb.len(), t.width * t.height * 3);
        assert_eq!(t.alpha.len(), t.width * t.height);
    }

    #[test]
    fn transparent_pixels_become_white_without_sticker_mode() {
        let t = build_target(&solid(20, 20, Rgba([0, 0, 0, 0])), false, 4096);
        let i = (10 * t.width + 10) * 3;
        assert_eq!(&t.rgb[i..i + 3], &[255, 255, 255]);
        assert_eq!(at(&t, 10, 10), 255);
    }

    #[test]
    fn sticker_mode_keeps_source_alpha() {
        let t = build_target(&solid(20, 20, Rgba([5, 6, 7, 0])), true, 4096);
        assert_eq!(at(&t, 10, 10), 0);
    }
}
