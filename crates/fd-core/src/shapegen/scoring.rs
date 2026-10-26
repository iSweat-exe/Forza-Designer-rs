//! Edge-weighted error metric, optimal colour and compositing.
//!
//! Everything works on flat row-major buffers: `rgb` is `w * h * 3` bytes,
//! `alpha` / `edge` are `w * h`.

use super::raster::Spans;
use super::shape::{Rgba, Shape};

/// How much more an edge pixel counts than a smooth one (weights are 1..=EDGE_BOOST).
/// Without it the search drifts to big blobs that average well but miss eyes,
/// mouths and outlines.
const EDGE_BOOST: f32 = 6.0;

/// A candidate must put at least this share of its body on opaque pixels, or
/// Forza (no per-pixel alpha) would draw paint into the transparent area.
const STICKER_OVERLAP_MIN: f32 = 0.995;

/// Immutable inputs shared by every scoring call.
pub struct Fit<'a> {
    pub w: usize,
    pub h: usize,
    pub target: &'a [u8],
    pub alpha: &'a [u8],
    pub edge: &'a [f32],
    /// See [`opaque_prefix`].
    pub opaque_prefix: &'a [u32],
    pub shape_alpha: u8,
}

/// Importance map: 0 where `alpha == 0`, 1 in flat areas, up to `EDGE_BOOST` on
/// the strongest luminance edge (3x3 Sobel).
pub fn compute_edge_weight(rgb: &[u8], alpha: &[u8], w: usize, h: usize) -> Vec<f32> {
    let lum: Vec<f32> = rgb
        .chunks_exact(3)
        .map(|p| f32::from(p[0]) * 0.299 + f32::from(p[1]) * 0.587 + f32::from(p[2]) * 0.114)
        .collect();
    let at = |x: isize, y: isize| -> f32 {
        let xc = x.clamp(0, w as isize - 1) as usize;
        let yc = y.clamp(0, h as isize - 1) as usize;
        lum[yc * w + xc]
    };
    let mut mag = vec![0.0f32; w * h];
    let mut max_mag = 0.0f32;
    for y in 0..h as isize {
        for x in 0..w as isize {
            let gx = -at(x - 1, y - 1) + at(x + 1, y - 1) - 2.0 * at(x - 1, y) + 2.0 * at(x + 1, y)
                - at(x - 1, y + 1)
                + at(x + 1, y + 1);
            let gy = -at(x - 1, y - 1) - 2.0 * at(x, y - 1) - at(x + 1, y - 1)
                + at(x - 1, y + 1)
                + 2.0 * at(x, y + 1)
                + at(x + 1, y + 1);
            let m = gx.hypot(gy);
            max_mag = max_mag.max(m);
            mag[y as usize * w + x as usize] = m;
        }
    }
    mag.iter()
        .zip(alpha)
        .map(|(&m, &a)| {
            let base = if max_mag < 1e-6 {
                1.0
            } else {
                1.0 + (EDGE_BOOST - 1.0) * (m / max_mag)
            };
            if a > 0 {
                base
            } else {
                0.0
            }
        })
        .collect()
}

/// `(sum of weighted squared error over the whole canvas, normaliser)`.
/// `rms = sqrt(full_sq / n)`.
pub fn canvas_error(canvas: &[u8], fit: &Fit<'_>) -> (f64, f64) {
    let mut full = 0.0f64;
    for ((c, t), &wt) in canvas
        .chunks_exact(3)
        .zip(fit.target.chunks_exact(3))
        .zip(fit.edge)
    {
        if wt == 0.0 {
            continue;
        }
        let d: f32 = (0..3)
            .map(|k| (f32::from(c[k]) - f32::from(t[k])).powi(2))
            .sum();
        full += f64::from(d * wt);
    }
    let n = fit.edge.iter().map(|&v| f64::from(v)).sum::<f64>() * 3.0;
    (full, n)
}

pub fn rms(full_sq: f64, n: f64) -> f64 {
    if n < 1.0 {
        0.0
    } else {
        (full_sq.max(0.0) / n).sqrt()
    }
}

/// Closed-form colour minimising the squared error over the shape (weighted by
/// opacity of the target there), for `out = a*src + (1-a)*dst` compositing.
fn optimal_color(spans: &Spans, canvas: &[u8], fit: &Fit<'_>) -> Rgba {
    let a_byte = fit.shape_alpha;
    let a = f32::from(a_byte) / 255.0;
    if a < 1e-6 {
        return [0, 0, 0, a_byte];
    }
    let (mut weight, mut sum) = (0.0f32, [0.0f32; 3]);
    for s in &spans.rows {
        let first = s.y * fit.w + s.x0;
        let last = s.y * fit.w + s.x1;
        let alpha = &fit.alpha[first..last];
        let tgt = fit.target[first * 3..last * 3].chunks_exact(3);
        let cur = canvas[first * 3..last * 3].chunks_exact(3);
        for ((&al, t), c) in alpha.iter().zip(tgt).zip(cur) {
            let m = f32::from(al) / 255.0;
            weight += m;
            for k in 0..3 {
                sum[k] += m * (f32::from(t[k]) - (1.0 - a) * f32::from(c[k])) / a;
            }
        }
    }
    if weight < 0.5 {
        return [0, 0, 0, a_byte];
    }
    // `as u8` saturates and truncates, matching clip(0..255).astype(int).
    [
        (sum[0] / weight) as u8,
        (sum[1] / weight) as u8,
        (sum[2] / weight) as u8,
        a_byte,
    ]
}

/// Error if `shape` were committed on `canvas`, with its optimal colour.
/// Returns `(f64::INFINITY, ..)` for shapes that are empty or that spill out of
/// the opaque region.
pub fn score_shape(
    shape: &Shape,
    canvas: &[u8],
    fit: &Fit<'_>,
    base: (f64, f64),
    spans: &mut Spans,
) -> (f64, Rgba) {
    if !shape.spans(fit.w, fit.h, spans) {
        return (f64::INFINITY, shape.color());
    }

    // Sticker contract: the body must sit (almost) entirely on opaque pixels.
    // Opaque pixels per span come from the row prefix sums in O(1).
    let stride = fit.w + 1;
    let (mut body, mut inside) = (0usize, 0usize);
    for s in &spans.rows {
        body += s.x1 - s.x0;
        let row = s.y * stride;
        inside += (fit.opaque_prefix[row + s.x1] - fit.opaque_prefix[row + s.x0]) as usize;
    }
    if body == 0 || (inside as f32) / (body as f32) < STICKER_OVERLAP_MIN {
        return (f64::INFINITY, shape.color());
    }

    let color = optimal_color(spans, canvas, fit);
    let a = f32::from(color[3]) / 255.0;
    let src = [
        f32::from(color[0]),
        f32::from(color[1]),
        f32::from(color[2]),
    ];

    let mut delta = 0.0f64;
    for s in &spans.rows {
        let first = s.y * fit.w + s.x0;
        let last = s.y * fit.w + s.x1;
        let edge = &fit.edge[first..last];
        let alpha = &fit.alpha[first..last];
        let tgt = fit.target[first * 3..last * 3].chunks_exact(3);
        let cur = canvas[first * 3..last * 3].chunks_exact(3);
        for (((&wt, &al), t), c) in edge.iter().zip(alpha).zip(tgt).zip(cur) {
            if wt == 0.0 {
                continue;
            }
            let m = f32::from(al) / 255.0;
            let mut d = 0.0f32;
            for k in 0..3 {
                let (cur, tgt) = (f32::from(c[k]), f32::from(t[k]));
                let blended = cur + m * a * (src[k] - cur);
                d += (blended - tgt).powi(2) - (cur - tgt).powi(2);
            }
            delta += f64::from(d * wt);
        }
    }
    (rms(base.0 + delta, base.1), color)
}

/// Paint `shape` onto `canvas` with its optimal colour (stored back into the
/// shape). Returns the change in the weighted squared error.
pub fn composite(shape: &mut Shape, canvas: &mut [u8], fit: &Fit<'_>, spans: &mut Spans) -> f64 {
    if !shape.spans(fit.w, fit.h, spans) {
        return 0.0;
    }
    let color = optimal_color(spans, canvas, fit);
    shape.set_color(color);
    let a = f32::from(color[3]) / 255.0;
    let mut delta = 0.0f64;
    for s in &spans.rows {
        let first = s.y * fit.w + s.x0;
        let last = s.y * fit.w + s.x1;
        let edge = &fit.edge[first..last];
        let alpha = &fit.alpha[first..last];
        let tgt = fit.target[first * 3..last * 3].chunks_exact(3);
        let cur = canvas[first * 3..last * 3].chunks_exact_mut(3);
        for (((&wt, &al), t), c) in edge.iter().zip(alpha).zip(tgt).zip(cur) {
            let m = f32::from(al) / 255.0;
            if m == 0.0 {
                continue;
            }
            let mut d = 0.0f32;
            for k in 0..3 {
                let (old, tgt) = (f32::from(c[k]), f32::from(t[k]));
                let new = (old + m * a * (f32::from(color[k]) - old)).clamp(0.0, 255.0) as u8;
                c[k] = new;
                d += (f32::from(new) - tgt).powi(2) - (old - tgt).powi(2);
            }
            delta += f64::from(d * wt);
        }
    }
    delta
}

/// Row-wise prefix sums of "is opaque" (`alpha >= 128`): `(w + 1)` entries per
/// row, so the opaque count of `x0..x1` on row `y` is `p[y*(w+1)+x1] - p[y*(w+1)+x0]`.
pub fn opaque_prefix(alpha: &[u8], w: usize, h: usize) -> Vec<u32> {
    let mut out = vec![0u32; (w + 1) * h];
    for y in 0..h {
        let row = &mut out[y * (w + 1)..(y + 1) * (w + 1)];
        for x in 0..w {
            row[x + 1] = row[x] + u32::from(alpha[y * w + x] >= 128);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scene {
        w: usize,
        h: usize,
        target: Vec<u8>,
        alpha: Vec<u8>,
        edge: Vec<f32>,
        canvas: Vec<u8>,
        opaque: Vec<u32>,
    }

    /// 40x40, opaque everywhere except a 4px border; left half red, right half blue.
    fn scene() -> Scene {
        let (w, h) = (40usize, 40usize);
        let mut target = vec![0u8; w * h * 3];
        let mut alpha = vec![0u8; w * h];
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                let rgb = if x < w / 2 {
                    [200, 20, 20]
                } else {
                    [20, 20, 200]
                };
                target[i * 3..i * 3 + 3].copy_from_slice(&rgb);
                alpha[i] = u8::from((4..w - 4).contains(&x) && (4..h - 4).contains(&y)) * 255;
            }
        }
        for (px, a) in target.chunks_exact_mut(3).zip(&alpha) {
            if *a == 0 {
                px.fill(0);
            }
        }
        let edge = compute_edge_weight(&target, &alpha, w, h);
        Scene {
            w,
            h,
            opaque: opaque_prefix(&alpha, w, h),
            canvas: vec![40; w * h * 3],
            target,
            alpha,
            edge,
        }
    }

    fn fit(s: &Scene) -> Fit<'_> {
        Fit {
            w: s.w,
            h: s.h,
            target: &s.target,
            alpha: &s.alpha,
            edge: &s.edge,
            opaque_prefix: &s.opaque,
            shape_alpha: 255,
        }
    }

    #[test]
    fn edge_weight_is_zero_outside_alpha_and_bounded() {
        let s = scene();
        assert!(s
            .edge
            .iter()
            .zip(&s.alpha)
            .all(|(&e, &a)| (a == 0) == (e == 0.0)));
        assert!(s
            .edge
            .iter()
            .all(|&e| (0.0..=EDGE_BOOST + 1e-4).contains(&e)));
    }

    #[test]
    fn rectangle_on_red_half_gets_red_colour_and_lowers_error() {
        let s = scene();
        let f = fit(&s);
        let base = canvas_error(&s.canvas, &f);
        let shape = Shape::Rectangle {
            x: 12.0,
            y: 20.0,
            hw: 6.0,
            hh: 6.0,
            color: [0, 0, 0, 255],
        };
        let mut mask = Spans::default();
        let (score, color) = score_shape(&shape, &s.canvas, &f, base, &mut mask);
        assert_eq!(color, [200, 20, 20, 255]);
        assert!(score < rms(base.0, base.1));
    }

    #[test]
    fn shape_spilling_into_transparent_border_is_rejected() {
        let s = scene();
        let f = fit(&s);
        let base = canvas_error(&s.canvas, &f);
        let shape = Shape::Rectangle {
            x: 5.0,
            y: 20.0,
            hw: 6.0,
            hh: 6.0,
            color: [0; 4],
        };
        let (score, _) = score_shape(&shape, &s.canvas, &f, base, &mut Spans::default());
        assert!(score.is_infinite());
    }

    #[test]
    fn composite_delta_matches_full_recompute() {
        let mut s = scene();
        let (w, h) = (s.w, s.h);
        let (target, alpha, edge, opaque) = (
            s.target.clone(),
            s.alpha.clone(),
            s.edge.clone(),
            s.opaque.clone(),
        );
        let f = Fit {
            w,
            h,
            target: &target,
            alpha: &alpha,
            edge: &edge,
            opaque_prefix: &opaque,
            shape_alpha: 200,
        };
        let (full0, n) = canvas_error(&s.canvas, &f);
        let mut shape = Shape::RotatedEllipse {
            x: 20.0,
            y: 20.0,
            rx: 9.0,
            ry: 5.0,
            angle: 30.0,
            color: [0, 0, 0, 200],
        };
        let delta = composite(&mut shape, &mut s.canvas, &f, &mut Spans::default());
        let (full1, _) = canvas_error(&s.canvas, &f);
        assert!(
            ((full0 + delta) - full1).abs() < 1.0,
            "{} vs {}",
            full0 + delta,
            full1
        );
        assert!(rms(full1, n) < rms(full0, n));
    }

    #[test]
    fn score_predicts_committed_error() {
        let mut s = scene();
        let (w, h) = (s.w, s.h);
        let (target, alpha, edge, opaque) = (
            s.target.clone(),
            s.alpha.clone(),
            s.edge.clone(),
            s.opaque.clone(),
        );
        let f = Fit {
            w,
            h,
            target: &target,
            alpha: &alpha,
            edge: &edge,
            opaque_prefix: &opaque,
            shape_alpha: 255,
        };
        let base = canvas_error(&s.canvas, &f);
        let mut shape = Shape::Circle {
            x: 25.0,
            y: 20.0,
            r: 8.0,
            color: [0, 0, 0, 255],
        };
        let (predicted, _) = score_shape(&shape, &s.canvas, &f, base, &mut Spans::default());
        let d = composite(&mut shape, &mut s.canvas, &f, &mut Spans::default());
        // Prediction ignores u8 truncation of the committed pixels; allow a hair of slack.
        assert!((predicted - rms(base.0 + d, base.1)).abs() < 0.05);
    }
}
