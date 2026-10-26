use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::shape::ShapeKind;
use crate::error::{Error, Result};

/// Generation settings. Field names mirror the `.ini` profiles of the original
/// app (`maxResolution`, `randomSamples`, ...) so existing profiles still load.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub name: String,
    pub description: String,
    /// Longest preview side sent to the UI.
    pub max_preview_size: u32,
    /// Source image is downscaled so its longest side is at most this.
    pub max_resolution: u32,
    /// 0 = automatic.
    pub max_threads: usize,
    pub mutated_samples: usize,
    pub preview_every: usize,
    pub random_samples: usize,
    pub save_at: Vec<usize>,
    pub save_every: usize,
    pub stop_at: usize,
    pub shape_types: Vec<ShapeKind>,
    /// Alpha used while fitting. Forza renders layers fully opaque, so 255 makes
    /// the preview match the in-game result. (The Python app fitted at 128.)
    pub shape_alpha: u8,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            name: "default".into(),
            description: "Default profile".into(),
            max_preview_size: 500,
            max_resolution: 1200,
            max_threads: 0,
            mutated_samples: 200,
            preview_every: 1,
            random_samples: 1000,
            save_at: vec![500, 1000, 1500, 2000, 2500, 3000],
            save_every: 100,
            stop_at: 3000,
            shape_types: vec![ShapeKind::RotatedEllipse],
            shape_alpha: 255,
        }
    }
}

impl Profile {
    /// Parse an `.ini` profile (with or without a `[profile]` header).
    /// Unknown keys (e.g. `posterizeLevels`, `computeBackend`) are ignored.
    pub fn from_ini(name: &str, text: &str) -> Result<Self> {
        let kv = parse_ini(text);
        let mut p = Self {
            name: name.to_owned(),
            ..Self::default()
        };
        let int = |key: &str| -> Result<Option<usize>> {
            kv.get(&key.to_ascii_lowercase())
                .map(|v| {
                    v.parse::<usize>()
                        .map_err(|_| Error::Profile(format!("{key}: {v:?} is not an integer")))
                })
                .transpose()
        };
        if let Some(v) = kv.get("description") {
            p.description.clone_from(v);
        }
        if let Some(v) = int("maxPreviewSize")? {
            p.max_preview_size = v as u32;
        }
        if let Some(v) = int("maxResolution")? {
            p.max_resolution = v as u32;
        }
        if let Some(v) = int("maxThreads")? {
            p.max_threads = v;
        }
        if let Some(v) = int("mutatedSamples")? {
            p.mutated_samples = v;
        }
        if let Some(v) = int("previewEvery")? {
            p.preview_every = v;
        }
        if let Some(v) = int("randomSamples")? {
            p.random_samples = v;
        }
        if let Some(v) = int("saveEvery")? {
            p.save_every = v;
        }
        if let Some(v) = int("stopAt")? {
            p.stop_at = v;
        }
        if let Some(v) = kv.get("saveat") {
            p.save_at = v
                .split(',')
                .filter(|s| !s.trim().is_empty())
                .map(|s| {
                    s.trim()
                        .parse()
                        .map_err(|_| Error::Profile(format!("saveAt: bad value {s:?}")))
                })
                .collect::<Result<_>>()?;
        }
        if let Some(v) = kv.get("shapetypes") {
            let kinds = v
                .split(',')
                .filter(|s| !s.trim().is_empty())
                .map(|s| s.parse::<ShapeKind>().map_err(Error::Profile))
                .collect::<Result<Vec<_>>>()?;
            if !kinds.is_empty() {
                p.shape_types = kinds;
            }
        }
        Ok(p)
    }

    pub fn validate(&self) -> Result<()> {
        if self.stop_at == 0 {
            return Err(Error::Profile("stopAt must be at least 1".into()));
        }
        if self.max_resolution < 16 {
            return Err(Error::Profile("maxResolution must be at least 16".into()));
        }
        if self.shape_types.is_empty() {
            return Err(Error::Profile("at least one shape type is required".into()));
        }
        Ok(())
    }
}

/// Lower-cased `key -> value`, ignoring comments and section headers.
fn parse_ini(text: &str) -> HashMap<String, String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with(['#', ';', '[']))
        .filter_map(|l| l.split_once(['=', ':']))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_owned()))
        .collect()
}

/// The four profiles shipped with the original app, in display order.
pub fn bundled_profiles() -> Vec<Profile> {
    const SOURCES: [(&str, &str); 4] = [
        ("fast", include_str!("../../profiles/fast.ini")),
        ("balanced", include_str!("../../profiles/balanced.ini")),
        (
            "best_quality",
            include_str!("../../profiles/best_quality.ini"),
        ),
        (
            "extreme_quality",
            include_str!("../../profiles/extreme_quality.ini"),
        ),
    ];
    SOURCES
        .iter()
        .filter_map(|(name, text)| Profile::from_ini(name, text).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_profiles_all_parse() {
        let all = bundled_profiles();
        assert_eq!(all.len(), 4);
        let balanced = all.iter().find(|p| p.name == "balanced").unwrap();
        assert_eq!(balanced.random_samples, 1000);
        assert_eq!(balanced.mutated_samples, 200);
        assert_eq!(balanced.max_resolution, 1200);
        assert_eq!(balanced.shape_types, vec![ShapeKind::RotatedEllipse]);
        assert_eq!(balanced.save_at, vec![500, 1000, 1500, 2000, 2500, 3000]);
    }

    #[test]
    fn fast_profile_stops_at_1500() {
        let all = bundled_profiles();
        assert_eq!(all.iter().find(|p| p.name == "fast").unwrap().stop_at, 1500);
    }

    #[test]
    fn rejects_non_numeric_value() {
        assert!(Profile::from_ini("x", "stopAt = lots").is_err());
    }

    #[test]
    fn headerless_and_sectioned_ini_agree() {
        let a = Profile::from_ini("x", "stopAt = 42").unwrap();
        let b = Profile::from_ini("x", "[profile]\nstopAt = 42").unwrap();
        assert_eq!(a, b);
    }
}

#[cfg(test)]
mod more_tests {
    use super::*;

    #[test]
    fn validate_rejects_degenerate_profiles() {
        let ok = Profile::default();
        assert!(ok.validate().is_ok());
        assert!(Profile {
            stop_at: 0,
            ..ok.clone()
        }
        .validate()
        .is_err());
        assert!(Profile {
            max_resolution: 8,
            ..ok.clone()
        }
        .validate()
        .is_err());
        assert!(Profile {
            shape_types: vec![],
            ..ok
        }
        .validate()
        .is_err());
    }

    #[test]
    fn unknown_shape_type_is_an_error_and_unknown_keys_are_ignored() {
        assert!(Profile::from_ini("x", "shapeTypes = hexagon").is_err());
        let p = Profile::from_ini(
            "x",
            "posterizeLevels = 64\ncomputeBackend = gpu\nstopAt = 7",
        )
        .unwrap();
        assert_eq!(p.stop_at, 7);
    }

    #[test]
    fn parses_multiple_shape_types_and_save_points() {
        let p = Profile::from_ini(
            "x",
            "shapeTypes = circle, rotated_rectangle\nsaveAt = 10, 20,\n",
        )
        .unwrap();
        assert_eq!(
            p.shape_types,
            vec![ShapeKind::Circle, ShapeKind::RotatedRectangle]
        );
        assert_eq!(p.save_at, vec![10, 20]);
    }

    #[test]
    fn comments_and_colons_are_understood() {
        let p = Profile::from_ini("x", "# note\n; other\nstopAt: 9\n").unwrap();
        assert_eq!(p.stop_at, 9);
    }

    #[test]
    fn profile_round_trips_through_json_for_the_ui() {
        let p = Profile::default();
        let back: Profile = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn json_with_missing_fields_falls_back_to_defaults() {
        let p: Profile = serde_json::from_str(r#"{"stop_at": 5}"#).unwrap();
        assert_eq!(
            (p.stop_at, p.random_samples),
            (5, Profile::default().random_samples)
        );
    }
}
