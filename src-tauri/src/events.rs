use serde::Serialize;

/// Streamed to the UI while a generation runs.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GenerationEvent {
    Started {
        width: usize,
        height: usize,
    },
    Progress {
        shapes: usize,
        total: usize,
        rms: f64,
    },
    /// `png` is a `data:image/png;base64,...` URL (RGBA, transparent outside the content area).
    Preview {
        png: String,
    },
    Checkpoint {
        path: String,
    },
    /// Non-fatal problem (e.g. a checkpoint could not be written).
    Warning {
        message: String,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct GenerationSummary {
    pub path: String,
    pub shapes: usize,
    pub rms: f64,
    pub stopped_early: bool,
    /// Set when the engine ran out of places to put shapes.
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GameInfo {
    pub key: &'static str,
    pub label: &'static str,
    pub running: bool,
    pub validated_build: Option<&'static str>,
    /// Injection needs Windows; false elsewhere.
    pub injection_supported: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImageInfo {
    pub width: u32,
    pub height: u32,
    pub has_alpha: bool,
    pub thumbnail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DocumentInfo {
    pub path: String,
    pub shape_count: usize,
    pub image_size: [u32; 2],
    pub profile: String,
    pub source_image: String,
    pub sticker_mode: bool,
    pub generated_at: String,
}
