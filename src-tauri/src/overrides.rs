//! Optional `.fd6_offsets.json` next to the app / in the working directory: lets
//! users correct struct offsets after a game patch without a rebuild.

use std::path::PathBuf;

pub const FILE_NAME: &str = ".fd6_offsets.json";

fn candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        out.push(cwd.join(FILE_NAME));
    }
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(PathBuf::from))
    {
        out.push(dir.join(FILE_NAME));
    }
    out.dedup();
    out
}

/// First readable, valid override file; anything broken is ignored so injection
/// still runs on the built-in offsets.
pub fn load() -> Option<serde_json::Value> {
    candidates().into_iter().find_map(|path| {
        let text = std::fs::read_to_string(path).ok()?;
        let value: serde_json::Value =
            serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
        value.is_object().then_some(value)
    })
}
