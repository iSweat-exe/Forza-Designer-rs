use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};
use rayon::prelude::*;

use super::preprocess::Target;
use super::profile::Profile;
use super::scoring::{self, Fit};
use super::shape::{Mask, Shape, ShapeKind};
use crate::error::{Error, Result};

/// After this many failed attempts in a row we give up and finish early.
const MAX_CONSECUTIVE_SKIPS: usize = 80;
/// Retries of one iteration when no candidate fits inside the opaque region.
const MAX_STICKER_RETRIES: usize = 5;
/// Fill colour of the canvas before any shape is placed.
const INITIAL_GREY: u8 = 40;

/// Stop / pause switches shared with the UI thread.
#[derive(Debug, Clone, Default)]
pub struct Control {
    stop: Arc<AtomicBool>,
    pause: Arc<AtomicBool>,
}

impl Control {
    pub fn request_stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
    pub fn set_paused(&self, paused: bool) {
        self.pause.store(paused, Ordering::Relaxed);
    }
    pub fn is_stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }
    fn paused(&self) -> bool {
        self.pause.load(Ordering::Relaxed)
    }
}

/// Progress notifications. Borrowed payloads avoid copying the canvas.
#[derive(Debug)]
pub enum EngineEvent<'a> {
    Progress {
        shapes: usize,
        total: usize,
        rms: f64,
    },
    /// RGB canvas plus the coverage mask (for transparent previews).
    Preview {
        width: usize,
        height: usize,
        rgb: &'a [u8],
        alpha: &'a [u8],
    },
    Checkpoint {
        shapes: &'a [Shape],
    },
    /// `message` is set when the run ended before `stop_at` was reached.
    Done {
        shapes: usize,
        rms: f64,
        message: Option<String>,
    },
}

pub struct Engine {
    profile: Profile,
    target: Target,
    edge: Vec<f32>,
    canvas: Vec<u8>,
    shapes: Vec<Shape>,
    full_sq: f64,
    norm: f64,
    rng: SmallRng,
    workers: usize,
    control: Control,
}

impl Engine {
    /// `seed == 0` picks a time-based seed.
    pub fn new(mut target: Target, profile: Profile, seed: u64, control: Control) -> Result<Self> {
        profile.validate()?;
        if target.width == 0 || target.height == 0 {
            return Err(Error::Generation("image is empty".into()));
        }
        // Transparent pixels never contribute, so zero them in the target.
        for (px, &a) in target.rgb.chunks_exact_mut(3).zip(&target.alpha) {
            if a == 0 {
                px.fill(0);
            }
        }
        let edge =
            scoring::compute_edge_weight(&target.rgb, &target.alpha, target.width, target.height);
        let canvas = vec![INITIAL_GREY; target.width * target.height * 3];
        let (full_sq, norm) = scoring::canvas_error(&canvas, &fit_of(&target, &edge, &profile));
        let seed = if seed == 0 { time_seed() } else { seed };
        let workers = worker_count(profile.max_threads, profile.random_samples);
        Ok(Self {
            profile,
            target,
            edge,
            canvas,
            shapes: Vec::new(),
            full_sq,
            norm,
            rng: SmallRng::seed_from_u64(seed),
            workers,
            control,
        })
    }

    pub fn shapes(&self) -> &[Shape] {
        &self.shapes
    }

    pub fn size(&self) -> (usize, usize) {
        (self.target.width, self.target.height)
    }

    /// Current canvas as RGBA (transparent outside the content area).
    pub fn canvas_rgba(&self) -> Vec<u8> {
        super::render::rgba_from_rgb_alpha(&self.canvas, &self.target.alpha)
    }

    pub fn rms(&self) -> f64 {
        scoring::rms(self.full_sq, self.norm)
    }

    /// Shrinking size schedule: big tonal blocks first, pixel-scale detail last.
    fn size_cap(progress: f32) -> f32 {
        match progress {
            p if p < 0.25 => 0.30,
            p if p < 0.50 => 0.22,
            p if p < 0.75 => 0.15,
            _ => 0.10,
        }
    }

    pub fn run(&mut self, mut on_event: impl FnMut(EngineEvent<'_>)) -> Result<()> {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(self.workers)
            .build()
            .map_err(|e| Error::Generation(format!("could not start worker threads: {e}")))?;

        // Each iteration is locked to one shape type so every enabled type gets
        // commit slots (otherwise ellipses win every fitness contest).
        let kinds = self.profile.shape_types.clone();
        let mut type_cursor = 0usize;
        let mut skips = 0usize;
        let mut mask = Mask::default();
        let stop_at = self.profile.stop_at;

        while self.shapes.len() < stop_at && !self.control.is_stopped() {
            while self.control.paused() && !self.control.is_stopped() {
                std::thread::sleep(Duration::from_millis(50));
            }
            if self.control.is_stopped() {
                break;
            }
            let kind = kinds[type_cursor % kinds.len()];
            type_cursor += 1;
            let cap = Self::size_cap(self.shapes.len() as f32 / stop_at as f32);

            let mut best = pool.install(|| self.parallel_search(kind, cap));
            let mut retries = 0;
            while best.is_none() && retries < MAX_STICKER_RETRIES && !self.control.is_stopped() {
                best = pool.install(|| self.parallel_search(kind, cap));
                retries += 1;
            }
            let Some((_, mut shape)) = best else {
                skips += 1;
                if skips >= MAX_CONSECUTIVE_SKIPS {
                    on_event(EngineEvent::Done {
                        shapes: self.shapes.len(),
                        rms: self.rms(),
                        message: Some(format!(
                            "Stopped early at {} shapes: nothing else fits inside the opaque region \
                             after {MAX_CONSECUTIVE_SKIPS} consecutive attempts.",
                            self.shapes.len()
                        )),
                    });
                    return Ok(());
                }
                continue;
            };
            skips = 0;

            let fit = fit_of(&self.target, &self.edge, &self.profile);
            self.full_sq += scoring::composite(&mut shape, &mut self.canvas, &fit, &mut mask);
            self.shapes.push(shape);
            let count = self.shapes.len();

            on_event(EngineEvent::Progress {
                shapes: count,
                total: stop_at,
                rms: self.rms(),
            });
            if self.profile.preview_every > 0 && count % self.profile.preview_every == 0 {
                on_event(EngineEvent::Preview {
                    width: self.target.width,
                    height: self.target.height,
                    rgb: &self.canvas,
                    alpha: &self.target.alpha,
                });
            }
            if self.profile.save_at.contains(&count)
                || (self.profile.save_every > 0 && count % self.profile.save_every == 0)
            {
                on_event(EngineEvent::Checkpoint {
                    shapes: &self.shapes,
                });
            }
        }
        on_event(EngineEvent::Done {
            shapes: self.shapes.len(),
            rms: self.rms(),
            message: None,
        });
        Ok(())
    }

    /// One independent search per worker; the global best wins.
    fn parallel_search(&mut self, kind: ShapeKind, size_cap: f32) -> Option<(f64, Shape)> {
        let seeds: Vec<u64> = (0..self.workers).map(|_| self.rng.random()).collect();
        let fit = fit_of(&self.target, &self.edge, &self.profile);
        let base = (self.full_sq, self.norm);
        let (n_random, n_mutate) = (
            self.profile.random_samples.max(1),
            self.profile.mutated_samples.max(1),
        );
        let canvas = &self.canvas;
        seeds
            .into_par_iter()
            .filter_map(|seed| search(&fit, canvas, base, kind, n_random, n_mutate, size_cap, seed))
            .min_by(|a, b| a.0.total_cmp(&b.0))
    }
}

/// Random search, then hill climbing on the best sample.
#[allow(clippy::too_many_arguments)]
fn search(
    fit: &Fit<'_>,
    canvas: &[u8],
    base: (f64, f64),
    kind: ShapeKind,
    n_random: usize,
    n_mutate: usize,
    size_cap: f32,
    seed: u64,
) -> Option<(f64, Shape)> {
    let mut rng = SmallRng::seed_from_u64(seed);
    let mut mask = Mask::default();
    let mut best: Option<(f64, Shape)> = None;

    for _ in 0..n_random {
        let mut s = Shape::random(
            kind,
            &mut rng,
            fit.w,
            fit.h,
            Some(size_cap),
            fit.shape_alpha,
        );
        let (score, color) = scoring::score_shape(&s, canvas, fit, base, &mut mask);
        if best.as_ref().map_or(score.is_finite(), |b| score < b.0) {
            s.set_color(color);
            best = Some((score, s));
        }
    }
    let (mut best_score, mut best_shape) = best?;

    let patience = 20.max(n_mutate / 4);
    let mut no_improve = 0;
    for _ in 0..n_mutate {
        let cand = best_shape.mutate(&mut rng, fit.w, fit.h);
        let (score, color) = scoring::score_shape(&cand, canvas, fit, base, &mut mask);
        if score < best_score {
            best_score = score;
            best_shape = cand;
            best_shape.set_color(color);
            no_improve = 0;
        } else {
            no_improve += 1;
            if no_improve >= patience {
                break;
            }
        }
    }
    Some((best_score, best_shape))
}

fn fit_of<'a>(target: &'a Target, edge: &'a [f32], profile: &Profile) -> Fit<'a> {
    Fit {
        w: target.width,
        h: target.height,
        target: &target.rgb,
        alpha: &target.alpha,
        edge,
        shape_alpha: profile.shape_alpha,
    }
}

fn time_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |d| d.as_millis() as u64 & 0xFFFF_FFFF)
        .max(1)
}

/// Leave a core or two free for the UI; small sample budgets don't amortise more threads.
fn worker_count(requested: usize, random_samples: usize) -> usize {
    let cpus = std::thread::available_parallelism().map_or(1, usize::from);
    let cpu_cap = cpus.saturating_sub(if cpus <= 4 { 1 } else { 2 }).max(1);
    let work_cap = (random_samples / 64).max(1);
    let wanted = if requested > 0 { requested } else { cpu_cap };
    wanted.min(cpu_cap).min(work_cap).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 64x64 target: white content area with a solid red square in the middle.
    fn target() -> Target {
        let (w, h) = (64usize, 64usize);
        let mut rgb = vec![255u8; w * h * 3];
        let mut alpha = vec![0u8; w * h];
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                if (6..58).contains(&x) && (6..58).contains(&y) {
                    alpha[i] = 255;
                    if (24..40).contains(&x) && (24..40).contains(&y) {
                        rgb[i * 3..i * 3 + 3].copy_from_slice(&[220, 30, 30]);
                    }
                }
            }
        }
        Target {
            width: w,
            height: h,
            rgb,
            alpha,
        }
    }

    fn profile(stop_at: usize) -> Profile {
        Profile {
            stop_at,
            random_samples: 128,
            mutated_samples: 40,
            preview_every: 0,
            save_every: 0,
            save_at: vec![],
            max_threads: 2,
            ..Profile::default()
        }
    }

    #[test]
    fn error_decreases_and_shapes_stay_in_bounds() {
        let mut engine = Engine::new(target(), profile(30), 42, Control::default()).unwrap();
        let start = engine.rms();
        let mut progress = Vec::new();
        engine
            .run(|e| {
                if let EngineEvent::Progress { rms, .. } = e {
                    progress.push(rms);
                }
            })
            .unwrap();
        assert_eq!(engine.shapes().len(), 30);
        assert!(engine.rms() < start * 0.6, "{start} -> {}", engine.rms());
        assert_eq!(progress.len(), 30);
        for s in engine.shapes() {
            assert_eq!(s.color()[3], 255);
        }
    }

    #[test]
    fn incremental_error_matches_a_full_recompute() {
        let mut engine = Engine::new(target(), profile(15), 7, Control::default()).unwrap();
        engine.run(|_| {}).unwrap();
        let fit = fit_of(&engine.target, &engine.edge, &engine.profile);
        let (full, n) = scoring::canvas_error(&engine.canvas, &fit);
        assert!((scoring::rms(full, n) - engine.rms()).abs() < 0.01);
    }

    #[test]
    fn stop_request_ends_the_run_cleanly() {
        let control = Control::default();
        let mut engine = Engine::new(target(), profile(1000), 1, control.clone()).unwrap();
        let mut done = false;
        engine
            .run(|e| match e {
                EngineEvent::Progress { shapes: 3, .. } => control.request_stop(),
                EngineEvent::Done { .. } => done = true,
                _ => {}
            })
            .unwrap();
        assert!(done);
        assert_eq!(engine.shapes().len(), 3);
    }

    #[test]
    fn same_seed_and_thread_count_is_deterministic() {
        let run = || {
            let mut e = Engine::new(target(), profile(8), 99, Control::default()).unwrap();
            e.run(|_| {}).unwrap();
            e.shapes().to_vec()
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn checkpoints_fire_at_configured_counts() {
        let mut p = profile(10);
        p.save_every = 5;
        let mut engine = Engine::new(target(), p, 3, Control::default()).unwrap();
        let mut at = Vec::new();
        engine
            .run(|e| {
                if let EngineEvent::Checkpoint { shapes } = e {
                    at.push(shapes.len());
                }
            })
            .unwrap();
        assert_eq!(at, vec![5, 10]);
    }

    #[test]
    fn fully_transparent_image_finishes_early_instead_of_hanging() {
        let mut t = target();
        t.alpha.fill(0);
        let mut engine = Engine::new(t, profile(50), 5, Control::default()).unwrap();
        let mut message = None;
        engine
            .run(|e| {
                if let EngineEvent::Done { message: m, .. } = e {
                    message = m;
                }
            })
            .unwrap();
        assert!(message.is_some());
    }
}
