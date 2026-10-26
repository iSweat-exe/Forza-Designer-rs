use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use base64::Engine as _;
use fd_core::document::Document;
use fd_core::inject::{self, InjectEvent, InjectOptions, InjectReport};
use fd_core::shapegen::{self, Control, Engine, EngineEvent, Profile};
use serde::Deserialize;
use tauri::ipc::Channel;
use tauri::State;

use crate::events::{DocumentInfo, GameInfo, GenerationEvent, GenerationSummary, ImageInfo};
use crate::jobs::Jobs;
use crate::overrides;

/// Minimum gap between preview frames sent to the UI.
const PREVIEW_INTERVAL: Duration = Duration::from_millis(120);
const THUMBNAIL_SIDE: u32 = 320;

fn data_url(png: &[u8]) -> String {
    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png)
    )
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn list_profiles() -> Vec<Profile> {
    shapegen::bundled_profiles()
}

#[tauri::command]
pub fn list_games() -> Vec<GameInfo> {
    inject::supported_games()
        .iter()
        .map(|g| GameInfo {
            key: g.key,
            label: g.label,
            running: inject::is_game_running(g),
            validated_build: g.validated_build,
            injection_supported: cfg!(windows),
        })
        .collect()
}

#[tauri::command]
pub async fn describe_image(path: String) -> Result<ImageInfo, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let s = shapegen::describe_image(Path::new(&path), THUMBNAIL_SIDE)
            .map_err(|e| e.to_string())?;
        Ok(ImageInfo {
            width: s.width,
            height: s.height,
            has_alpha: s.has_alpha,
            thumbnail: data_url(&s.thumbnail_png),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateRequest {
    pub image_path: String,
    pub profile: Profile,
    pub sticker_mode: bool,
    pub output_dir: Option<String>,
    pub seed: Option<u64>,
}

#[tauri::command]
pub async fn generate(
    request: GenerateRequest,
    on_event: Channel<GenerationEvent>,
    jobs: State<'_, Jobs>,
) -> Result<GenerationSummary, String> {
    let control = jobs
        .begin_generation()
        .ok_or("A generation is already running.")?;
    let result = tauri::async_runtime::spawn_blocking(move || {
        run_generation(&request, control, &|e| {
            // The UI may have gone away; generation should still finish and save.
            let _ = on_event.send(e);
        })
    })
    .await;
    jobs.end_generation();
    result.map_err(|e| format!("generation task failed: {e}"))?
}

fn run_generation(
    req: &GenerateRequest,
    control: Control,
    send: &dyn Fn(GenerationEvent),
) -> Result<GenerationSummary, String> {
    let image = PathBuf::from(&req.image_path);
    let out_dir = req.output_dir.as_ref().map_or_else(
        || {
            image
                .parent()
                .unwrap_or(Path::new("."))
                .join(image.file_stem().unwrap_or_default())
        },
        PathBuf::from,
    );
    let stem = image
        .file_stem()
        .map_or_else(|| "output".to_owned(), |s| s.to_string_lossy().into_owned());

    let profile = req.profile.clone();
    profile.validate().map_err(|e| e.to_string())?;
    let target = shapegen::load_target(&image, req.sticker_mode, profile.max_resolution)
        .map_err(|e| e.to_string())?;
    let (w, h) = (target.width, target.height);
    send(GenerationEvent::Started {
        width: w,
        height: h,
    });

    let max_preview = profile.max_preview_size;
    let stop_at = profile.stop_at;
    let mut engine = Engine::new(target, profile.clone(), req.seed.unwrap_or(0), control)
        .map_err(|e| e.to_string())?;

    let document = |shapes: &[shapegen::Shape]| {
        Document::new(
            file_name(&image),
            [w as u32, h as u32],
            shapes,
            profile.name.as_str(),
            req.sticker_mode,
        )
    };
    let preview = |rgba: Vec<u8>| -> Option<String> {
        shapegen::encode_png(rgba, w as u32, h as u32, max_preview)
            .ok()
            .map(|png| data_url(&png))
    };

    let mut last_preview = Instant::now() - PREVIEW_INTERVAL;
    let mut message = None;
    engine
        .run(|event| match event {
            EngineEvent::Progress { shapes, total, rms } => {
                send(GenerationEvent::Progress { shapes, total, rms })
            }
            EngineEvent::Preview { rgb, alpha, .. }
                if last_preview.elapsed() >= PREVIEW_INTERVAL =>
            {
                last_preview = Instant::now();
                if let Some(png) = preview(shapegen::rgba_from_rgb_alpha(rgb, alpha)) {
                    send(GenerationEvent::Preview { png });
                }
            }
            EngineEvent::Preview { .. } => {}
            EngineEvent::Checkpoint { shapes } => {
                let path = out_dir.join(format!("{stem}_{}.json", shapes.len()));
                match document(shapes).save(&path) {
                    Ok(()) => send(GenerationEvent::Checkpoint {
                        path: path.to_string_lossy().into_owned(),
                    }),
                    Err(e) => send(GenerationEvent::Warning {
                        message: format!("checkpoint not saved: {e}"),
                    }),
                }
            }
            EngineEvent::Done { message: m, .. } => message = m,
        })
        .map_err(|e| e.to_string())?;

    if let Some(png) = preview(engine.canvas_rgba()) {
        send(GenerationEvent::Preview { png });
    }
    let final_path = out_dir.join(format!("{stem}.json"));
    document(engine.shapes())
        .save(&final_path)
        .map_err(|e| e.to_string())?;
    Ok(GenerationSummary {
        path: final_path.to_string_lossy().into_owned(),
        shapes: engine.shapes().len(),
        rms: engine.rms(),
        stopped_early: engine.shapes().len() < stop_at,
        message,
    })
}

#[tauri::command]
pub fn stop_generation(jobs: State<'_, Jobs>) {
    if let Some(c) = jobs.generation() {
        c.request_stop();
    }
}

#[tauri::command]
pub fn set_generation_paused(paused: bool, jobs: State<'_, Jobs>) {
    if let Some(c) = jobs.generation() {
        c.set_paused(paused);
    }
}

#[tauri::command]
pub fn load_document(path: String) -> Result<DocumentInfo, String> {
    let doc = Document::load(Path::new(&path)).map_err(|e| e.to_string())?;
    Ok(DocumentInfo {
        path,
        shape_count: doc.shapes.len(),
        image_size: doc.image_size,
        profile: doc.profile,
        source_image: doc.source_image,
        sticker_mode: doc.sticker_mode,
        generated_at: doc.generated_at,
    })
}

/// Renders a saved document the way the game will draw it (opaque layers, in order).
#[tauri::command]
pub async fn render_document(path: String, max_side: u32) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let doc = Document::load(Path::new(&path)).map_err(|e| e.to_string())?;
        let [w, h] = doc.image_size;
        if w == 0 || h == 0 {
            return Err("document has no image size".to_owned());
        }
        let background = (!doc.sticker_mode).then_some([255, 255, 255]);
        let rgba = shapegen::render_shapes(&doc.shapes, w as usize, h as usize, background);
        let png = shapegen::encode_png(rgba, w, h, max_side).map_err(|e| e.to_string())?;
        Ok(data_url(&png))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Debug, Deserialize)]
pub struct InjectRequest {
    pub path: String,
    pub game: String,
}

#[tauri::command]
pub async fn inject(
    request: InjectRequest,
    on_event: Channel<InjectEvent>,
    jobs: State<'_, Jobs>,
) -> Result<InjectReport, String> {
    let game = inject::game_by_key(&request.game)
        .ok_or_else(|| format!("Unsupported game {:?}.", request.game))?;
    let doc = Document::load(Path::new(&request.path)).map_err(|e| e.to_string())?;
    let cancel = jobs
        .begin_injection()
        .ok_or("An injection is already running.")?;

    let options = InjectOptions {
        offset_overrides: overrides::load(),
    };
    let result = tauri::async_runtime::spawn_blocking(move || {
        inject::attach_and_inject(game, &doc.shapes, &options, &cancel, &mut |e| {
            let _ = on_event.send(e);
        })
    })
    .await;
    jobs.end_injection();
    result
        .map_err(|e| format!("injection task failed: {e}"))?
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn cancel_injection(jobs: State<'_, Jobs>) {
    if let Some(flag) = jobs.injection() {
        flag.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    fn write_test_image(dir: &Path) -> PathBuf {
        let path = dir.join("pic.png");
        let img = image::RgbImage::from_fn(80, 60, |x, _| {
            if x < 40 {
                image::Rgb([220, 40, 40])
            } else {
                image::Rgb([30, 60, 200])
            }
        });
        img.save(&path).unwrap();
        path
    }

    fn request(image: &Path, stop_at: usize) -> GenerateRequest {
        let mut profile = shapegen::bundled_profiles().remove(0);
        profile.stop_at = stop_at;
        profile.random_samples = 64;
        profile.mutated_samples = 20;
        profile.save_every = 5;
        profile.save_at = vec![];
        profile.max_threads = 2;
        GenerateRequest {
            image_path: image.to_string_lossy().into_owned(),
            profile,
            sticker_mode: false,
            output_dir: None,
            seed: Some(11),
        }
    }

    #[test]
    fn generation_saves_checkpoints_and_a_loadable_final_document() {
        let dir = tempfile::tempdir().unwrap();
        let image = write_test_image(dir.path());
        let events = RefCell::new(Vec::new());
        let summary = run_generation(&request(&image, 10), Control::default(), &|e| {
            events.borrow_mut().push(e)
        })
        .unwrap();

        assert_eq!(summary.shapes, 10);
        assert!(!summary.stopped_early);
        let doc = Document::load(Path::new(&summary.path)).unwrap();
        assert_eq!(doc.shape_count, 10);
        assert_eq!(doc.source_image, "pic.png");
        assert!(dir.path().join("pic/pic_5.json").exists());
        assert!(dir.path().join("pic/pic_10.json").exists());

        let events = events.borrow();
        assert!(matches!(
            events.first(),
            Some(GenerationEvent::Started { .. })
        ));
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e, GenerationEvent::Checkpoint { .. }))
                .count(),
            2
        );
        assert!(events.iter().any(|e| matches!(e, GenerationEvent::Preview { png } if png.starts_with("data:image/png;base64,"))));
    }

    #[test]
    fn stopping_early_still_saves_the_partial_result() {
        let dir = tempfile::tempdir().unwrap();
        let image = write_test_image(dir.path());
        let control = Control::default();
        control.request_stop();
        let summary = run_generation(&request(&image, 50), control, &|_| {}).unwrap();
        assert!(summary.stopped_early);
        assert!(Path::new(&summary.path).exists());
    }

    #[test]
    fn missing_image_is_reported_not_panicked() {
        let err = run_generation(
            &request(Path::new("/definitely/not/here.png"), 5),
            Control::default(),
            &|_| {},
        )
        .unwrap_err();
        assert!(err.contains("here.png"), "{err}");
    }
}
