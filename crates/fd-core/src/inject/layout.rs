use serde_json::Value;

/// Struct offsets and constants for one game's `CLiveryGroup` / layer types.
#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    /// `u16` number of layers, inside the group.
    pub count_offset: u64,
    /// `u64` pointer to the layer table (array of `u64` layer pointers).
    pub table_offset: u64,
    /// Layer fields (offsets inside each layer struct).
    pub position_offset: u64, // 2 x f32: x, y
    pub scale_offset: u64,    // 2 x f32
    pub rotation_offset: u64, // f32, degrees
    pub color_offset: u64,    // 4 x u8: R, G, B, A (A must be 255)
    pub mask_offset: u64,     // u8: 0 / 1
    pub shape_id_offset: u64, // u8
    /// JSON size -> game scale: `value / divisor`.
    pub scale_divisor_ellipse: f32,
    pub scale_divisor_other: f32,
    pub shape_id_ellipse: u8,
    pub shape_id_other: u8,
}

impl Layout {
    /// Build 354.221 / 364.933 values (FH5 and FH6 share them).
    pub const FH6: Layout = Layout {
        count_offset: 0x5A,
        table_offset: 0x78,
        position_offset: 0x18,
        scale_offset: 0x28,
        rotation_offset: 0x50,
        color_offset: 0x74,
        mask_offset: 0x78,
        shape_id_offset: 0x7A,
        scale_divisor_ellipse: 63.0,
        scale_divisor_other: 127.0,
        shape_id_ellipse: 102,
        shape_id_other: 101,
    };

    /// Highest byte (exclusive) of a layer we ever touch.
    pub fn layer_span(&self) -> usize {
        [
            self.position_offset + 8,
            self.scale_offset + 8,
            self.rotation_offset + 4,
            self.color_offset + 4,
            self.mask_offset + 1,
            self.shape_id_offset + 1,
        ]
        .into_iter()
        .max()
        .unwrap_or(0) as usize
    }

    /// Applies a user override file (`.fd6_offsets.json`, same format as the
    /// Python app: flat or scoped under the game key; numbers or `"0x5A"`
    /// strings). Returns the names of the keys that were applied.
    pub fn with_overrides(&self, game_key: &str, json: &Value) -> (Layout, Vec<&'static str>) {
        let scope = json.get(game_key).filter(|v| v.is_object()).unwrap_or(json);
        let mut out = self.clone();
        let mut applied = Vec::new();
        let mut off = |key: &'static str, field: &mut u64| {
            if let Some(v) = scope.get(key).and_then(parse_int) {
                *field = v;
                applied.push(key);
            }
        };
        off("livery_count_offset", &mut out.count_offset);
        off("layer_table_offset", &mut out.table_offset);
        off("layer_position_offset", &mut out.position_offset);
        off("layer_scale_offset", &mut out.scale_offset);
        off("layer_rotation_offset", &mut out.rotation_offset);
        off("layer_color_offset", &mut out.color_offset);
        off("layer_mask_offset", &mut out.mask_offset);
        off("layer_shape_id_offset", &mut out.shape_id_offset);
        let mut float = |key: &'static str, field: &mut f32| {
            if let Some(v) = scope.get(key).and_then(Value::as_f64).filter(|v| *v > 0.0) {
                *field = v as f32;
                applied.push(key);
            }
        };
        float("scale_divisor_ellipse", &mut out.scale_divisor_ellipse);
        float("scale_divisor_other", &mut out.scale_divisor_other);
        let mut byte = |key: &'static str, field: &mut u8| {
            if let Some(v) = scope
                .get(key)
                .and_then(parse_int)
                .and_then(|v| u8::try_from(v).ok())
            {
                *field = v;
                applied.push(key);
            }
        };
        byte("shape_id_ellipse", &mut out.shape_id_ellipse);
        byte("shape_id_other", &mut out.shape_id_other);
        (out, applied)
    }
}

/// `90`, `"0x5A"`, `"90"` or bare hex `"5A"`.
fn parse_int(v: &Value) -> Option<u64> {
    match v {
        Value::Number(n) => n.as_u64(),
        Value::String(s) => {
            let s = s.trim();
            if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
                u64::from_str_radix(hex, 16).ok()
            } else {
                s.parse().ok().or_else(|| u64::from_str_radix(s, 16).ok())
            }
        }
        _ => None,
    }
}

#[derive(Debug, Clone)]
pub struct GameProfile {
    pub key: &'static str,
    pub label: &'static str,
    /// Executable names to look for (case-insensitive, exact match).
    pub process_names: &'static [&'static str],
    pub layout: Layout,
    /// MSVC RTTI name of the vinyl group class, used by the fallback locator.
    pub rtti_class_name: &'static [u8],
    /// Game build the offsets were validated against.
    pub validated_build: Option<&'static str>,
}

pub const FH6: GameProfile = GameProfile {
    key: "fh6",
    label: "Forza Horizon 6",
    process_names: &["forzahorizon6.exe", "ForzaHorizon6-Win64-Shipping.exe"],
    layout: Layout::FH6,
    rtti_class_name: b".?AVCLiveryGroup@@",
    validated_build: Some("364.933"),
};

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn layer_span_covers_the_last_field() {
        assert_eq!(Layout::FH6.layer_span(), 0x7B);
    }

    #[test]
    fn overrides_accept_hex_decimal_and_bare_hex() {
        let j = json!({"fh6": {"layer_color_offset": "0x80", "layer_table_offset": 100, "livery_count_offset": "5A"}});
        let (l, applied) = Layout::FH6.with_overrides("fh6", &j);
        assert_eq!(
            (l.color_offset, l.table_offset, l.count_offset),
            (0x80, 100, 0x5A)
        );
        assert_eq!(applied.len(), 3);
    }

    #[test]
    fn flat_overrides_apply_and_garbage_is_ignored() {
        let j = json!({"scale_divisor_ellipse": 64.0, "layer_mask_offset": "nope", "shape_id_other": 300});
        let (l, applied) = Layout::FH6.with_overrides("fh6", &j);
        assert_eq!(l.scale_divisor_ellipse, 64.0);
        assert_eq!(l.mask_offset, Layout::FH6.mask_offset);
        assert_eq!(l.shape_id_other, 101);
        assert_eq!(applied, vec!["scale_divisor_ellipse"]);
    }

    #[test]
    fn overrides_for_another_game_are_not_applied() {
        let j = json!({"fh5": {"layer_color_offset": "0x99"}});
        let (l, applied) = Layout::FH6.with_overrides("fh6", &j);
        // Falls back to treating the top level as flat -> no known keys there.
        assert_eq!(l, Layout::FH6);
        assert!(applied.is_empty());
    }
}
