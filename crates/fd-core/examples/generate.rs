//! Headless generation: `cargo run --release -p fd-core --example generate -- <image> [profile] [shapes] [out.json]`
//! Writes the JSON and a PNG render of the result next to it.

use std::path::PathBuf;
use std::time::Instant;

use fd_core::document::Document;
use fd_core::shapegen::{bundled_profiles, load_target, Control, Engine, EngineEvent};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let image = PathBuf::from(
        args.next()
            .ok_or("usage: generate <image> [profile] [shapes] [out.json]")?,
    );
    let profile_name = args.next().unwrap_or_else(|| "fast".into());
    let mut profile = bundled_profiles()
        .into_iter()
        .find(|p| p.name == profile_name)
        .ok_or("unknown profile")?;
    if let Some(n) = args.next() {
        profile.stop_at = n.parse()?;
    }
    let out = args
        .next()
        .map_or_else(|| image.with_extension("json"), PathBuf::from);

    let target = load_target(&image, false, profile.max_resolution)?;
    let (w, h) = (target.width, target.height);
    let mut engine = Engine::new(target, profile.clone(), 1, Control::default())?;
    let started = Instant::now();
    let mut last = (Vec::new(), 0usize);
    engine.run(|e| match e {
        EngineEvent::Progress { shapes, total, rms } if shapes % 100 == 0 => {
            eprintln!(
                "{shapes}/{total}  rms {rms:.2}  {:.1}s",
                started.elapsed().as_secs_f32()
            );
        }
        EngineEvent::Preview { rgb, .. } => last = (rgb.to_vec(), 0),
        _ => {}
    })?;
    let doc = Document::new(
        image
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
        [w as u32, h as u32],
        engine.shapes(),
        profile.name.as_str(),
        false,
    );
    doc.save(&out)?;
    let png = out.with_extension("png");
    let rgb = if last.0.is_empty() {
        return Ok(());
    } else {
        last.0
    };
    image::RgbImage::from_raw(w as u32, h as u32, rgb)
        .ok_or("bad canvas")?
        .save(&png)?;
    eprintln!(
        "done: {} shapes, rms {:.2} in {:.1}s -> {} / {}",
        doc.shape_count,
        engine.rms(),
        started.elapsed().as_secs_f32(),
        out.display(),
        png.display()
    );
    Ok(())
}
