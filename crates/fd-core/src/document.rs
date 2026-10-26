//! The `fd6.shapes` JSON document, byte-compatible with the Python app.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::shapegen::Shape;

pub const FORMAT: &str = "fd6.shapes";
pub const VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Document {
    pub format: String,
    pub version: u32,
    #[serde(default)]
    pub source_image: String,
    /// `[width, height]` of the (padded, resized) canvas the shapes live in.
    #[serde(default)]
    pub image_size: [u32; 2],
    #[serde(default)]
    pub shape_count: usize,
    #[serde(default)]
    pub generated_at: String,
    #[serde(default)]
    pub profile: String,
    /// True when generated with a transparent backdrop ("sticker" mode).
    #[serde(default)]
    pub sticker_mode: bool,
    pub shapes: Vec<Shape>,
}

impl Document {
    pub fn new(
        source_image: impl Into<String>,
        image_size: [u32; 2],
        shapes: &[Shape],
        profile: impl Into<String>,
        sticker_mode: bool,
    ) -> Self {
        let shapes: Vec<Shape> = shapes.iter().map(Shape::rounded).collect();
        Self {
            format: FORMAT.into(),
            version: VERSION,
            source_image: source_image.into(),
            image_size,
            shape_count: shapes.len(),
            generated_at: utc_timestamp(SystemTime::now()),
            profile: profile.into(),
            sticker_mode,
            shapes,
        }
    }

    fn validate(&self) -> Result<()> {
        if self.format != FORMAT {
            return Err(Error::Document(format!(
                "unsupported document format {:?} (expected {FORMAT:?})",
                self.format
            )));
        }
        if self.version != VERSION {
            return Err(Error::Document(format!(
                "unsupported document version {} (expected {VERSION})",
                self.version
            )));
        }
        Ok(())
    }

    pub fn from_json(text: &str) -> Result<Self> {
        // Python wrote UTF-8 (possibly with BOM when edited on Windows).
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let doc: Self = serde_json::from_str(text).map_err(|e| Error::Document(e.to_string()))?;
        doc.validate()?;
        Ok(doc)
    }

    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).map_err(|e| Error::io(path, e))?;
        Self::from_json(&text)
    }

    /// Atomic write: serialise to a sibling temp file, then rename over `path`.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        let json =
            serde_json::to_string_pretty(self).map_err(|e| Error::Document(e.to_string()))?;
        let mut tmp = path.as_os_str().to_owned();
        tmp.push(".tmp");
        let tmp = std::path::PathBuf::from(tmp);
        std::fs::write(&tmp, json).map_err(|e| Error::io(&tmp, e))?;
        std::fs::rename(&tmp, path).map_err(|e| {
            // Don't leave the temp file behind if the rename fails.
            let _ = std::fs::remove_file(&tmp);
            Error::io(path, e)
        })
    }
}

/// `YYYY-MM-DDTHH:MM:SSZ` without pulling in a date crate.
fn utc_timestamp(now: SystemTime) -> String {
    let secs = now.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let (days, rem) = (secs / 86_400, secs % 86_400);
    let (y, m, d) = civil_from_days(days as i64);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// Howard Hinnant's days-since-epoch -> civil date algorithm.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_formats_known_instant() {
        // 2026-05-17T00:00:00Z
        let t = UNIX_EPOCH + std::time::Duration::from_secs(1_778_976_000);
        assert_eq!(utc_timestamp(t), "2026-05-17T00:00:00Z");
    }

    #[test]
    fn parses_python_generated_document() {
        let json = r#"{
          "format": "fd6.shapes", "version": 1, "source_image": "a.png",
          "image_size": [10, 12], "shape_count": 2, "generated_at": "2026-01-01T00:00:00Z",
          "profile": "balanced", "sticker_mode": false,
          "shapes": [
            {"type": "rotated_ellipse", "x": 1.5, "y": 2.0, "rx": 3.0, "ry": 4.0, "angle": 45.0, "color": [1,2,3,128]},
            {"type": "rotated_rectangle", "x": 1.0, "y": 2.0, "hw": 3.0, "hh": 4.0, "angle": 10.0, "color": [9,8,7,128]}
          ]}"#;
        let doc = Document::from_json(json).unwrap();
        assert_eq!(doc.shapes.len(), 2);
        assert_eq!(doc.image_size, [10, 12]);
    }

    #[test]
    fn rejects_wrong_format() {
        let json = r#"{"format":"other","version":1,"shapes":[]}"#;
        assert!(matches!(Document::from_json(json), Err(Error::Document(_))));
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/out.json");
        let shapes = [Shape::Circle {
            x: 1.0,
            y: 2.0,
            r: 3.0,
            color: [1, 2, 3, 255],
        }];
        let doc = Document::new("src.png", [4, 4], &shapes, "fast", true);
        doc.save(&path).unwrap();
        assert_eq!(Document::load(&path).unwrap(), doc);
    }

    #[test]
    fn rejects_an_unsupported_version() {
        let json = r#"{"format":"fd6.shapes","version":2,"shapes":[]}"#;
        let err = Document::from_json(json).unwrap_err().to_string();
        assert!(err.contains("version"), "{err}");
    }

    #[test]
    fn rejects_unknown_shape_types_with_a_useful_message() {
        let json = r#"{"format":"fd6.shapes","version":1,"shapes":[{"type":"triangle","x1":0}]}"#;
        let err = Document::from_json(json).unwrap_err().to_string();
        assert!(err.contains("triangle"), "{err}");
    }

    #[test]
    fn accepts_a_utf8_bom_and_missing_optional_fields() {
        let json = "\u{feff}{\"format\":\"fd6.shapes\",\"version\":1,\"shapes\":[]}";
        let doc = Document::from_json(json).unwrap();
        assert_eq!(
            (doc.shape_count, doc.image_size, doc.sticker_mode),
            (0, [0, 0], false)
        );
    }

    #[test]
    fn save_leaves_no_temp_file_and_replaces_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.json");
        let doc = Document::new("a.png", [1, 1], &[], "fast", false);
        doc.save(&path).unwrap();
        doc.save(&path).unwrap(); // overwrite
        let names: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, vec![std::ffi::OsString::from("out.json")]);
    }

    #[test]
    fn geometry_is_rounded_to_three_decimals_on_export() {
        let s = Shape::Circle {
            x: 1.234_567,
            y: 2.0,
            r: 3.0,
            color: [0; 4],
        };
        let doc = Document::new("a.png", [4, 4], &[s], "fast", false);
        let json = serde_json::to_string(&doc).unwrap();
        assert!(json.contains("1.235"), "{json}");
    }

    #[test]
    fn loading_a_missing_file_names_the_path() {
        let err = Document::load(Path::new("/no/such/file.json"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("file.json"), "{err}");
    }
}
