//! Layout model: parse a layout TOML, resolve named keys, compute key rectangles.
//!
//! Layout files live in `tools/rabbiz-osk/data/layouts/*.toml` and are shared by every
//! platform: the Wayland client renders exactly the rectangles this module computes,
//! and the host PNG preview uses the same numbers.

use std::collections::BTreeMap;
use std::fmt;

use serde::Deserialize;

/// What tapping a key does. Character keys are `Text`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyAction {
    Text(String),
    Shift,
    Backspace,
    Return,
    Space,
    /// Switch to another layer of the active layout (numbers, symbols, emoji).
    Layer(String),
    /// Switch the whole layout to the next language in the cycle.
    Language,
    /// Open another top-level layout as a panel (`panel = "emoji"`).
    Panel(String),
    /// Return from a panel to the layout that opened it.
    PanelBack,
    EmojiGroupPrev,
    EmojiGroupNext,
    EmojiSearch,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Key {
    pub label: String,
    pub action: KeyAction,
    /// Width in letter-key units; rows are normalised, so absolute values only shape ratios.
    pub width: f32,
    /// Long-press alternates, filled from the layout's `[alternates]` table.
    pub alternates: Vec<String>,
    /// What shift commits instead of upper-casing, filled from the layout's `[shifted]` table.
    pub shifted: Option<String>,
    /// Material Icons ligature name (`language`, `backspace`, `mic`…). When set the renderer
    /// draws the icon instead of the label.
    pub icon: Option<String>,
}

impl Key {
    pub fn is_character(&self) -> bool {
        matches!(self.action, KeyAction::Text(_))
    }
}

/// Bottom accessory bar height below the key rows (dismiss chevron on the
/// left, language switch globe on the right — matching Android/Gboard).
pub const BOTTOM_BAR_HEIGHT_PX: f32 = 48.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

/// A position inside the active layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyPos {
    pub row: usize,
    pub col: usize,
}

#[derive(Debug, Clone)]
pub struct Layer {
    pub name: String,
    pub rows: Vec<Vec<Key>>,
    /// Per-row inset in letter-key units, applied to both sides (Gboard insets row 2 by 0.5).
    pub insets: Vec<f32>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct Geometry {
    pub width_px: f32,
    pub key_height_px: f32,
    #[serde(default = "default_gap")]
    pub gap_px: f32,
    #[serde(default = "default_margin")]
    pub margin_px: f32,
    #[serde(default)]
    pub strip_height_px: f32,
}

fn default_gap() -> f32 {
    5.0
}
fn default_margin() -> f32 {
    5.0
}

#[derive(Debug)]
pub enum LayoutError {
    Parse(String),
    Invalid(String),
}

impl fmt::Display for LayoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LayoutError::Parse(m) => write!(f, "layout TOML: {m}"),
            LayoutError::Invalid(m) => write!(f, "layout: {m}"),
        }
    }
}

impl std::error::Error for LayoutError {}

#[derive(Debug, Clone)]
pub struct Layout {
    pub id: String,
    pub name: String,
    pub language: String,
    pub geometry: Geometry,
    pub layers: Vec<Layer>,
    pub alternates: BTreeMap<String, Vec<String>>,
    pub shifted: BTreeMap<String, String>,
}

// ---- raw TOML shape -------------------------------------------------------

#[derive(Deserialize)]
struct RawLayout {
    id: String,
    name: String,
    language: String,
    geometry: Geometry,
    #[serde(default)]
    keys: BTreeMap<String, RawKey>,
    #[serde(default)]
    alternates: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    shifted: BTreeMap<String, String>,
    #[serde(default)]
    layer: Vec<RawLayer>,
}

#[derive(Deserialize)]
struct RawLayer {
    name: String,
    #[serde(default)]
    insets: Vec<f32>,
    rows: Vec<String>,
}

#[derive(Deserialize)]
struct RawKey {
    action: String,
    label: Option<String>,
    width: Option<f32>,
    text: Option<String>,
    layer: Option<String>,
    panel: Option<String>,
    icon: Option<String>,
}

impl Layout {
    pub fn parse(src: &str) -> Result<Layout, LayoutError> {
        let raw: RawLayout = toml::from_str(src).map_err(|e| LayoutError::Parse(e.to_string()))?;
        if raw.layer.is_empty() {
            return Err(LayoutError::Invalid("no [[layer]] blocks".into()));
        }
        let mut layers = Vec::with_capacity(raw.layer.len());
        for raw_layer in &raw.layer {
            let mut rows = Vec::with_capacity(raw_layer.rows.len());
            for row_src in &raw_layer.rows {
                let mut keys = Vec::new();
                for token in row_src.split_whitespace() {
                    keys.push(resolve_token(token, &raw)?);
                }
                if keys.is_empty() {
                    return Err(LayoutError::Invalid(format!(
                        "layer {} has an empty row",
                        raw_layer.name
                    )));
                }
                rows.push(keys);
            }
            if rows.is_empty() {
                return Err(LayoutError::Invalid(format!(
                    "layer {} has no rows",
                    raw_layer.name
                )));
            }
            if !raw_layer.insets.is_empty() && raw_layer.insets.len() != rows.len() {
                return Err(LayoutError::Invalid(format!(
                    "layer {}: {} insets for {} rows",
                    raw_layer.name,
                    raw_layer.insets.len(),
                    rows.len()
                )));
            }
            layers.push(Layer {
                name: raw_layer.name.clone(),
                rows,
                insets: raw_layer.insets.clone(),
            });
        }
        // Layer names are the handle used by keys and by the session state.
        let mut seen = BTreeMap::new();
        for layer in &layers {
            if seen.insert(layer.name.clone(), ()).is_some() {
                return Err(LayoutError::Invalid(format!(
                    "duplicate layer `{}`",
                    layer.name
                )));
            }
        }
        Ok(Layout {
            id: raw.id,
            name: raw.name,
            language: raw.language,
            geometry: raw.geometry,
            layers,
            alternates: raw.alternates,
            shifted: raw.shifted,
        })
    }

    pub fn layer(&self, name: &str) -> Option<&Layer> {
        self.layers.iter().find(|l| l.name == name)
    }

    /// Replace one layer's rows (used for emoji recents/search results).
    pub fn replace_layer_rows(&mut self, name: &str, rows: Vec<Vec<Key>>) {
        if let Some(layer) = self.layers.iter_mut().find(|layer| layer.name == name) {
            layer.rows = rows;
        }
    }

    pub fn base_layer(&self) -> &Layer {
        self.layer("base").unwrap_or(&self.layers[0])
    }

    /// Surface height in px, fixed by the base layer's row count. Every layer fills exactly this
    /// height: a layer with fewer rows (numbers, emoji) gets taller keys instead of a shorter
    /// keyboard, so the surface and its exclusive zone never jump when the layer changes.
    pub fn height_px(&self) -> f32 {
        self.height_for_rows(self.base_layer().rows.len().max(1))
    }

    /// Same surface height for every layer — kept as a named call so renderers stay explicit.
    pub fn height_px_for(&self, _layer_name: &str) -> f32 {
        self.height_px()
    }

    fn height_for_rows(&self, rows: usize) -> f32 {
        let g = self.geometry;
        g.strip_height_px
            + g.gap_px
            + rows as f32 * g.key_height_px
            + rows.saturating_sub(1) as f32 * g.gap_px
            + g.gap_px
            + BOTTOM_BAR_HEIGHT_PX
    }

    /// Key rectangles for one layer, in surface coordinates.
    pub fn row_rects(&self, layer_name: &str) -> Result<Vec<Vec<(Rect, KeyPos)>>, LayoutError> {
        let layer = self
            .layer(layer_name)
            .ok_or_else(|| LayoutError::Invalid(format!("unknown layer `{layer_name}`")))?;
        let g = self.geometry;
        let available = g.width_px - 2.0 * g.margin_px;
        if available <= 0.0 {
            return Err(LayoutError::Invalid(
                "width_px is smaller than the margins".into(),
            ));
        }
        let insets = &layer.insets;
        let mut out = Vec::with_capacity(layer.rows.len());
        for (r, row) in layer.rows.iter().enumerate() {
            let inset = insets.get(r).copied().unwrap_or(0.0);
            let (left, right) = (inset, inset);
            let units: f32 = left + right + row.iter().map(|k| k.width).sum::<f32>();
            if units <= 0.0 {
                return Err(LayoutError::Invalid(format!(
                    "layer {layer_name} row {r} is empty"
                )));
            }
            let gaps = (row.len() as f32 - 1.0).max(0.0) * g.gap_px;
            let usable = available - gaps;
            if usable <= 0.0 {
                return Err(LayoutError::Invalid(format!(
                    "layer {layer_name} row {r} has more gap than width"
                )));
            }
            let unit_px = usable / units;
            let rows_n = layer.rows.len() as f32;
            let row_h = (self.height_px()
                - g.strip_height_px
                - BOTTOM_BAR_HEIGHT_PX
                - g.gap_px * (rows_n + 1.0))
                / rows_n;
            if row_h <= 0.0 {
                return Err(LayoutError::Invalid(format!(
                    "layer {layer_name} has too many rows for the surface height"
                )));
            }
            let y = g.strip_height_px + g.gap_px + r as f32 * (row_h + g.gap_px);
            let mut x = g.margin_px + left * unit_px;
            let mut keys = Vec::with_capacity(row.len());
            for (c, key) in row.iter().enumerate() {
                let w = unit_px * key.width;
                keys.push((Rect { x, y, w, h: row_h }, KeyPos { row: r, col: c }));
                x += w + g.gap_px;
            }
            out.push(keys);
        }
        Ok(out)
    }

    /// Which key is under a surface point, in the given layer.
    pub fn hit_test(&self, layer_name: &str, x: f32, y: f32) -> Option<KeyPos> {
        let rows = self.row_rects(layer_name).ok()?;
        rows.iter()
            .flat_map(|row| row.iter())
            .find(|(rect, _)| rect.contains(x, y))
            .map(|(_, pos)| *pos)
    }

    /// Build a character key at runtime (emoji recents/results are patched in, not in TOML).
    pub fn text_key(text: &str) -> Key {
        Key {
            label: text.to_string(),
            action: KeyAction::Text(text.to_string()),
            width: 1.0,
            alternates: Vec::new(),
            shifted: None,
            icon: None,
        }
    }

    pub fn key(&self, layer_name: &str, pos: KeyPos) -> Option<&Key> {
        self.layer(layer_name)?.rows.get(pos.row)?.get(pos.col)
    }

    /// Label to draw for a key given the shift state (letters upper-case when shifted).
    pub fn display_label(&self, key: &Key, shifted: bool) -> String {
        if shifted {
            if let Some(shifted_text) = &key.shifted {
                return shifted_text.clone();
            }
        }
        match (&key.action, shifted) {
            (KeyAction::Text(t), true) => t.to_uppercase(),
            _ => key.label.clone(),
        }
    }
}

fn resolve_token(token: &str, raw: &RawLayout) -> Result<Key, LayoutError> {
    if let Some(k) = raw.keys.get(token) {
        let label = k.label.clone().unwrap_or_else(|| token.to_string());
        let action = match k.action.as_str() {
            "text" => KeyAction::Text(k.text.clone().unwrap_or_else(|| token.to_string())),
            "shift" => KeyAction::Shift,
            "backspace" => KeyAction::Backspace,
            "return" => KeyAction::Return,
            "space" => KeyAction::Space,
            "layer" => KeyAction::Layer(
                k.layer
                    .clone()
                    .ok_or_else(|| LayoutError::Invalid(format!("key `{token}` needs a layer")))?,
            ),
            "language" => KeyAction::Language,
            "panel" => KeyAction::Panel(
                k.panel
                    .clone()
                    .ok_or_else(|| LayoutError::Invalid(format!("key `{token}` needs a panel")))?,
            ),
            "panel_back" => KeyAction::PanelBack,
            "emoji_group_prev" => KeyAction::EmojiGroupPrev,
            "emoji_group_next" => KeyAction::EmojiGroupNext,
            "emoji_search" => KeyAction::EmojiSearch,
            other => {
                return Err(LayoutError::Invalid(format!(
                    "key `{token}` has unknown action `{other}`"
                )))
            }
        };
        let alternates = match &action {
            KeyAction::Text(t) => raw.alternates.get(t).cloned().unwrap_or_default(),
            _ => Vec::new(),
        };
        return Ok(Key {
            label,
            action,
            width: k.width.unwrap_or(1.0),
            alternates,
            shifted: raw.shifted.get(token).cloned(),
            icon: k.icon.clone(),
        });
    }
    Ok(Key {
        label: token.to_string(),
        action: KeyAction::Text(token.to_string()),
        width: 1.0,
        alternates: raw.alternates.get(token).cloned().unwrap_or_default(),
        shifted: raw.shifted.get(token).cloned(),
        icon: None,
    })
}
