//! Scanline coverage of a shape.
//!
//! The search scores hundreds of thousands of candidates, so instead of
//! testing every pixel of the bounding box we solve each row analytically for
//! the covered `x` interval and then *correct* the interval ends against the
//! exact per-pixel predicate ([`Shape::contains`]). The result is therefore
//! bit-for-bit the set of pixels the predicate selects (see the equivalence
//! tests), only much cheaper: work is proportional to covered pixels plus a few
//! predicate evaluations per row.

use super::shape::{BBox, Shape};

/// Covered pixels `x0..x1` (exclusive end) of row `y`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub y: usize,
    pub x0: usize,
    pub x1: usize,
}

/// Reusable output buffer for [`Shape::spans`].
#[derive(Debug, Default)]
pub struct Spans {
    pub bbox: BBox,
    pub rows: Vec<Span>,
}

impl Spans {
    /// Number of covered pixels.
    pub fn area(&self) -> usize {
        self.rows.iter().map(|s| s.x1 - s.x0).sum()
    }
}

/// Solutions are padded by this many pixels before being corrected, so a small
/// numeric error in the analytic estimate can never lose a covered pixel.
const MARGIN: i64 = 1;

impl Shape {
    /// Exact per-pixel coverage test (pixel centres on the integer grid).
    /// Rectangles cover their whole bounding box, like the original app.
    pub fn contains(&self, px: f32, py: f32) -> bool {
        match *self {
            Self::Ellipse { x, y, rx, ry, .. } => {
                let dx = (px - x) * (1.0 / rx.max(1e-6));
                let dy = (py - y) * (1.0 / ry.max(1e-6));
                dx * dx + dy * dy <= 1.0
            }
            Self::Circle { x, y, r, .. } => {
                let (dx, dy) = (px - x, py - y);
                dx * dx + dy * dy <= r.max(1e-6).powi(2)
            }
            Self::Rectangle { .. } => true,
            Self::RotatedEllipse {
                x,
                y,
                rx,
                ry,
                angle,
                ..
            } => {
                let (s, c) = angle.to_radians().sin_cos();
                let (xg, yg) = (px - x, py - y);
                let xr = (c * xg + s * yg) * (1.0 / rx.max(1e-6));
                let yr = (-s * xg + c * yg) * (1.0 / ry.max(1e-6));
                xr * xr + yr * yr <= 1.0
            }
            Self::RotatedRectangle {
                x,
                y,
                hw,
                hh,
                angle,
                ..
            } => {
                let (s, c) = angle.to_radians().sin_cos();
                let (xg, yg) = (px - x, py - y);
                (c * xg + s * yg).abs() <= hw && (-s * xg + c * yg).abs() <= hh
            }
        }
    }

    /// Fills `out` with the covered rows. Returns `false` when nothing is covered.
    pub fn spans(&self, w: usize, h: usize, out: &mut Spans) -> bool {
        let bbox = self.bbox(w, h);
        out.bbox = bbox;
        out.rows.clear();
        if bbox.is_empty() {
            return false;
        }
        out.rows.reserve(bbox.height());

        if matches!(self, Self::Rectangle { .. }) {
            out.rows.extend((bbox.y0..bbox.y1).map(|y| Span {
                y,
                x0: bbox.x0,
                x1: bbox.x1,
            }));
            return true;
        }

        // Per-shape constants hoisted out of the row loop.
        let prep = Prepared::new(self);
        for y in bbox.y0..bbox.y1 {
            if let Some((lo, hi)) = prep.row_estimate(y as f64) {
                push_corrected(self, y, lo, hi, &bbox, &mut out.rows);
            }
        }
        !out.rows.is_empty()
    }
}

/// Analytic row solver for the non-rectangular shapes.
enum Prepared {
    Ellipse {
        x: f64,
        y: f64,
        rx: f64,
        ry: f64,
    },
    Quadric {
        /// `A·xg² + B(yg)·xg + C(yg) <= 1`, in coordinates relative to the centre.
        x: f64,
        y: f64,
        a: f64,
        b_k: f64, // B = b_k * yg
        c_k: f64, // C = c_k * yg²
    },
    Rect {
        x: f64,
        y: f64,
        s: f64,
        c: f64,
        hw: f64,
        hh: f64,
    },
}

impl Prepared {
    fn new(shape: &Shape) -> Self {
        match *shape {
            Shape::Ellipse { x, y, rx, ry, .. } => Self::Ellipse {
                x: f64::from(x),
                y: f64::from(y),
                rx: f64::from(rx.max(1e-6)),
                ry: f64::from(ry.max(1e-6)),
            },
            Shape::Circle { x, y, r, .. } => {
                let r = f64::from(r.max(1e-6));
                Self::Ellipse {
                    x: f64::from(x),
                    y: f64::from(y),
                    rx: r,
                    ry: r,
                }
            }
            Shape::RotatedEllipse {
                x,
                y,
                rx,
                ry,
                angle,
                ..
            } => {
                let (s, c) = f64::from(angle).to_radians().sin_cos();
                let (irx2, iry2) = (
                    1.0 / f64::from(rx.max(1e-6)).powi(2),
                    1.0 / f64::from(ry.max(1e-6)).powi(2),
                );
                Self::Quadric {
                    x: f64::from(x),
                    y: f64::from(y),
                    a: c * c * irx2 + s * s * iry2,
                    b_k: 2.0 * c * s * (irx2 - iry2),
                    c_k: s * s * irx2 + c * c * iry2,
                }
            }
            Shape::RotatedRectangle {
                x,
                y,
                hw,
                hh,
                angle,
                ..
            } => {
                let (s, c) = f64::from(angle).to_radians().sin_cos();
                Self::Rect {
                    x: f64::from(x),
                    y: f64::from(y),
                    s,
                    c,
                    hw: f64::from(hw),
                    hh: f64::from(hh),
                }
            }
            Shape::Rectangle { .. } => unreachable!("rectangles are handled before this point"),
        }
    }

    /// Estimated covered interval `[lo, hi]` (real x) of integer row `py`, or `None`.
    fn row_estimate(&self, py: f64) -> Option<(f64, f64)> {
        match *self {
            Self::Ellipse { x, y, rx, ry } => {
                let dy = (py - y) / ry;
                let t = 1.0 - dy * dy;
                // Slightly negative values are float noise on the tangent row.
                (t > -1e-3).then(|| {
                    let half = rx * t.max(0.0).sqrt();
                    (x - half, x + half)
                })
            }
            Self::Quadric { x, y, a, b_k, c_k } => {
                let yg = py - y;
                let (b, c) = (b_k * yg, c_k * yg * yg - 1.0);
                let disc = b * b - 4.0 * a * c;
                let tol = 1e-3 * (b * b + (4.0 * a * c).abs() + 1e-12);
                (disc > -tol).then(|| {
                    let root = disc.max(0.0).sqrt();
                    (x + (-b - root) / (2.0 * a), x + (-b + root) / (2.0 * a))
                })
            }
            Self::Rect { x, y, s, c, hw, hh } => {
                let yg = py - y;
                let mut lo = f64::NEG_INFINITY;
                let mut hi = f64::INFINITY;
                // |c·xg + s·yg| <= hw   and   |−s·xg + c·yg| <= hh
                for (coef, offset, limit) in [(c, s * yg, hw), (-s, c * yg, hh)] {
                    if coef.abs() < 1e-9 {
                        if offset.abs() > limit + 1e-3 {
                            return None;
                        }
                    } else {
                        let (a, b) = ((-limit - offset) / coef, (limit - offset) / coef);
                        lo = lo.max(a.min(b));
                        hi = hi.min(a.max(b));
                    }
                }
                (lo <= hi + 1.0).then_some((x + lo, x + hi))
            }
        }
    }
}

/// Pads the estimate, clips it to the bbox and trims both ends with the exact predicate.
fn push_corrected(shape: &Shape, y: usize, lo: f64, hi: f64, bbox: &BBox, rows: &mut Vec<Span>) {
    let (min_x, max_x) = (bbox.x0 as i64, bbox.x1 as i64 - 1);
    let clamp = |v: f64| -> i64 { v.clamp(-1e9, 1e9) as i64 };
    let mut a = (clamp(lo.ceil()) - MARGIN).clamp(min_x, max_x + 1);
    let mut b = (clamp(hi.floor()) + MARGIN).clamp(min_x - 1, max_x);
    let py = y as f32;
    while a <= b && !shape.contains(a as f32, py) {
        a += 1;
    }
    while b >= a && !shape.contains(b as f32, py) {
        b -= 1;
    }
    if a <= b {
        rows.push(Span {
            y,
            x0: a as usize,
            x1: b as usize + 1,
        });
    }
}

#[cfg(test)]
mod tests {
    use rand::rngs::SmallRng;
    use rand::{Rng, SeedableRng};

    use super::*;
    use crate::shapegen::ShapeKind;

    /// Ground truth: test every pixel of the bounding box.
    fn reference(shape: &Shape, w: usize, h: usize) -> Vec<(usize, usize)> {
        let bb = shape.bbox(w, h);
        let mut out = Vec::new();
        if bb.is_empty() {
            return out;
        }
        for y in bb.y0..bb.y1 {
            for x in bb.x0..bb.x1 {
                if shape.contains(x as f32, y as f32) {
                    out.push((x, y));
                }
            }
        }
        out
    }

    fn from_spans(spans: &Spans) -> Vec<(usize, usize)> {
        spans
            .rows
            .iter()
            .flat_map(|s| (s.x0..s.x1).map(move |x| (x, s.y)))
            .collect()
    }

    fn assert_equivalent(shape: &Shape, w: usize, h: usize) {
        let mut spans = Spans::default();
        let any = shape.spans(w, h, &mut spans);
        let expected = reference(shape, w, h);
        assert_eq!(any, !expected.is_empty(), "{shape:?}");
        assert_eq!(from_spans(&spans), expected, "{shape:?}");
    }

    #[test]
    fn spans_match_the_per_pixel_predicate_for_random_shapes() {
        let mut rng = SmallRng::seed_from_u64(2024);
        for &(w, h) in &[(97usize, 61usize), (200, 200), (33, 150)] {
            for kind in ShapeKind::ALL {
                for _ in 0..400 {
                    let frac = rng.random_range(0.02..1.0);
                    let s = Shape::random(kind, &mut rng, w, h, Some(frac), 255);
                    assert_equivalent(&s, w, h);
                }
            }
        }
    }

    #[test]
    fn spans_match_for_extreme_and_degenerate_shapes() {
        let c = [0; 4];
        let shapes = [
            // very thin, nearly axis-aligned rotated shapes (tiny coefficients)
            Shape::RotatedRectangle {
                x: 50.0,
                y: 50.0,
                hw: 30.0,
                hh: 0.4,
                angle: 0.0,
                color: c,
            },
            Shape::RotatedRectangle {
                x: 50.0,
                y: 50.0,
                hw: 30.0,
                hh: 5.0,
                angle: 90.0,
                color: c,
            },
            Shape::RotatedRectangle {
                x: 50.0,
                y: 50.0,
                hw: 20.0,
                hh: 8.0,
                angle: 45.0,
                color: c,
            },
            Shape::RotatedEllipse {
                x: 50.5,
                y: 50.5,
                rx: 40.0,
                ry: 1.0,
                angle: 33.0,
                color: c,
            },
            Shape::RotatedEllipse {
                x: 50.0,
                y: 50.0,
                rx: 1.0,
                ry: 1.0,
                angle: 10.0,
                color: c,
            },
            Shape::Ellipse {
                x: 0.0,
                y: 0.0,
                rx: 30.0,
                ry: 20.0,
                color: c,
            },
            Shape::Circle {
                x: 99.0,
                y: 99.0,
                r: 25.0,
                color: c,
            },
            Shape::Circle {
                x: 50.0,
                y: 50.0,
                r: 1.0,
                color: c,
            },
            Shape::Rectangle {
                x: 10.0,
                y: 90.0,
                hw: 15.0,
                hh: 15.0,
                color: c,
            },
        ];
        for s in &shapes {
            assert_equivalent(s, 100, 100);
        }
    }

    #[test]
    fn rows_are_sorted_and_non_overlapping() {
        let mut rng = SmallRng::seed_from_u64(5);
        let mut spans = Spans::default();
        for _ in 0..200 {
            let s = Shape::random(ShapeKind::RotatedEllipse, &mut rng, 120, 90, Some(0.5), 255);
            s.spans(120, 90, &mut spans);
            assert!(spans.rows.windows(2).all(|p| p[0].y < p[1].y));
            assert!(spans.rows.iter().all(|r| r.x0 < r.x1 && r.x1 <= 120));
        }
    }

    #[test]
    fn area_counts_covered_pixels() {
        let s = Shape::Rectangle {
            x: 5.0,
            y: 5.0,
            hw: 2.0,
            hh: 1.0,
            color: [0; 4],
        };
        let mut spans = Spans::default();
        assert!(s.spans(20, 20, &mut spans));
        assert_eq!(spans.area(), from_spans(&spans).len());
    }
}
