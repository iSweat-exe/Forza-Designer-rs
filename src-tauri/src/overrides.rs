//! Optional `.fd6_offsets.json` next to the app / in the working directory: lets
//! users correct struct offsets after a game patch without a rebuild.

use std::path::{Path, PathBuf};

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
    load_first(&candidates())
}

fn load_first(paths: &[PathBuf]) -> Option<serde_json::Value> {
    paths.iter().find_map(|path| read_object(path))
}

fn read_object(path: &Path) -> Option<serde_json::Value> {
    let text = std::fs::read_to_string(path).ok()?;
    let value: serde_json::Value =
        serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
    value.is_object().then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, text: &str) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, text).unwrap();
        p
    }

    #[test]
    fn reads_a_valid_object() {
        let dir = tempfile::tempdir().unwrap();
        let p = write(
            dir.path(),
            "a.json",
            r#"{"fh6": {"layer_color_offset": "0x74"}}"#,
        );
        assert_eq!(
            load_first(&[p]).unwrap()["fh6"]["layer_color_offset"],
            "0x74"
        );
    }

    #[test]
    fn tolerates_a_utf8_bom_from_windows_editors() {
        let dir = tempfile::tempdir().unwrap();
        let p = write(dir.path(), "a.json", "\u{feff}{\"a\": 1}");
        assert!(load_first(&[p]).is_some());
    }

    #[test]
    fn skips_broken_files_and_uses_the_next_candidate() {
        let dir = tempfile::tempdir().unwrap();
        let bad = write(dir.path(), "bad.json", "{ not json");
        let array = write(dir.path(), "array.json", "[1, 2]");
        let good = write(dir.path(), "good.json", r#"{"ok": true}"#);
        let missing = dir.path().join("missing.json");
        assert_eq!(
            load_first(&[missing, bad, array, good]).unwrap()["ok"],
            true
        );
    }

    #[test]
    fn no_candidates_means_no_overrides() {
        assert!(load_first(&[]).is_none());
    }
}
