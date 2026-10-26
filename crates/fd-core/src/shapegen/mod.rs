//! Image -> shapes generation (greedy random search + hill climbing).

mod engine;
mod preprocess;
mod profile;
mod raster;
mod render;
mod scoring;
mod shape;

pub use engine::{Control, Engine, EngineEvent};
pub use preprocess::{describe_image, load_target, ImageSummary, Target};
pub use profile::{bundled_profiles, Profile};
pub use render::{encode_png, render_shapes, rgba_from_rgb_alpha};
pub use shape::{Rgba, Shape, ShapeKind};
