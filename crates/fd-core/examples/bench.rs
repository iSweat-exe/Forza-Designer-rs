//! Single-threaded engine benchmark: `cargo run --release -p fd-core --example bench -- <image> [shapes]`.
//! Prints throughput and a checksum of the result so optimisations can be checked for identical output.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::PathBuf;
use std::time::Instant;

use fd_core::shapegen::{bundled_profiles, load_target, Control, Engine};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let image = PathBuf::from(args.next().ok_or("usage: bench <image> [shapes]")?);
    let shapes: usize = args.next().map_or(Ok(150), |s| s.parse())?;
    let mut profile = bundled_profiles()
        .into_iter()
        .find(|p| p.name == "balanced")
        .ok_or("no profile")?;
    profile.stop_at = shapes;
    profile.max_threads = 1;
    profile.preview_every = 0;
    profile.save_every = 0;
    profile.save_at.clear();

    let target = load_target(&image, false, profile.max_resolution)?;
    let mut engine = Engine::new(target, profile, 1234, Control::default())?;
    let started = Instant::now();
    engine.run(|_| {})?;
    let secs = started.elapsed().as_secs_f64();

    let mut h = DefaultHasher::new();
    for s in engine.shapes() {
        serde_json::to_string(&s.rounded())?.hash(&mut h);
    }
    println!(
        "{shapes} shapes in {secs:.2}s ({:.1} shapes/s), rms {:.4}, checksum {:016x}",
        shapes as f64 / secs,
        engine.rms(),
        h.finish()
    );
    Ok(())
}
