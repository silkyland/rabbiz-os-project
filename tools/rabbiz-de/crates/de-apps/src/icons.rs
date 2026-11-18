//! Icon pipeline (M2-T3): resolve a desktop entry's `Icon=` name through the
//! hicolor theme, rasterize to the shell's 96 px box, cache the result.
//!
//! Rules from plan §7.17:
//! - Rabbiz apps (`a21s-*`): 96 px sand tile (#DAC9B3 base, radius 28) with a
//!   plum glyph — first letter of the app name until per-app glyphs land.
//! - Third-party apps: original icon in a 96 px box, no recolor (Chrome keeps
//!   its logo, KDE apps keep theirs).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use de_api::{Pixels, Size};
use de_design::tokens;
use de_paint::{svg, text, Canvas};

/// Pixel size of the icon box (contract constant).
pub const ICON_PX: i32 = tokens::ICON_TILE_SIZE;

/// Directories searched for themed icons, in priority order.
fn icon_roots() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(h) = std::env::var_os("HOME").map(PathBuf::from) {
        v.push(h.join(".local/share/icons"));
        v.push(h.join(".icons"));
    }
    v.push(PathBuf::from("/usr/share/icons"));
    v.push(PathBuf::from("/usr/local/share/icons"));
    v.push(PathBuf::from("/usr/share/pixmaps"));
    v
}

/// Candidate file paths for `icon_name` (hicolor subdirs + pixmaps flat).
fn candidates(icon_name: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if icon_name.starts_with('/') {
        out.push(PathBuf::from(icon_name));
        return out;
    }
    for root in icon_roots() {
        for (theme, sub) in [
            ("hicolor", "scalable/apps"),
            ("hicolor", "512x512/apps"),
            ("hicolor", "256x256/apps"),
            ("hicolor", "128x128/apps"),
            ("hicolor", "96x96/apps"),
            ("hicolor", "64x64/apps"),
            ("hicolor", "48x48/apps"),
        ] {
            for ext in ["png", "svg"] {
                out.push(
                    root.join(theme)
                        .join(sub)
                        .join(format!("{icon_name}.{ext}")),
                );
            }
        }
        for ext in ["png", "svg", "xpm"] {
            out.push(root.join(format!("{icon_name}.{ext}")));
        }
    }
    out
}

/// Find the first existing icon file for `icon_name`.
#[must_use]
pub fn lookup(icon_name: &str) -> Option<PathBuf> {
    if icon_name.is_empty() {
        return None;
    }
    candidates(icon_name).into_iter().find(|p| p.is_file())
}

/// Whether `desktop_id` is a first-party Rabbiz app (`a21s-*`).
#[must_use]
pub fn is_rabbiz_app(desktop_id: &str) -> bool {
    desktop_id.strip_prefix("a21s-").is_some() || desktop_id == "rabbiz-de"
}

/// Rasterize the icon for `desktop_id` + `icon_name` + display `label`
/// (fallback letter) at `size` px. Third-party artwork is passed through
/// un-recolored; first-party gets the sand tile + plum glyph.
#[must_use]
pub fn rasterize_icon(desktop_id: &str, icon_name: &str, label: &str, size: i32) -> Pixels {
    if !is_rabbiz_app(desktop_id) {
        if let Some(path) = lookup(icon_name) {
            if let Some(px) = rasterize_file(&path, size) {
                return px;
            }
        }
    }
    tile_icon(label, size)
}

fn rasterize_file(path: &Path, size: i32) -> Option<Pixels> {
    let bytes = std::fs::read(path).ok()?;
    match path.extension().and_then(|e| e.to_str()) {
        Some("svg") => {
            let px = svg::rasterize(&bytes, size as u32).ok()?;
            Some(px)
        }
        Some("png") => {
            let mut px = de_paint::decode_png(&bytes).ok()?;
            if px.size.w != size || px.size.h != size {
                px = scale_pixels(&px, size);
            }
            Some(px)
        }
        _ => None,
    }
}

/// First-party tile: sand tile, plum initial.
fn tile_icon(label: &str, size: i32) -> Pixels {
    let t = tokens::DEFAULT;
    let mut c = Canvas::new(Size::new(size, size));
    let r = de_api::Rect::new(0, 0, size, size);
    let radius = tokens::ICON_TILE_RADIUS * (size as f32 / 96.0);
    c.fill_rounded_rect(r, radius, t.icon_tile);
    let letter: String = label
        .chars()
        .find(|ch| ch.is_alphanumeric())
        .map_or_else(|| "?".into(), |ch| ch.to_uppercase().collect());
    let style = text::LabelStyle {
        size: size as f32 * 0.44,
        line_height: size as f32 * 0.5,
        weight: 600,
        color: t.icon_glyph,
        ..Default::default()
    };
    if let Some(px) = text::label(&letter, style, None) {
        let (w, h) = (px.width() as i32, px.height() as i32);
        let p = Pixels::new(Size::new(w, h), px.data().to_vec());
        c.blit(&p, de_api::Point::new((size - w) / 2, (size - h) / 2));
    }
    c.into_pixels()
}

/// Nearest-neighbor downscale/upscale (good enough for icon boxes; keeps
/// premultiplied RGBA semantics).
fn scale_pixels(src: &Pixels, size: i32) -> Pixels {
    let (sw, sh) = (src.size.w.max(1), src.size.h.max(1));
    let mut out = Pixels::empty(Size::new(size, size));
    for y in 0..size {
        for x in 0..size {
            let sx = (x * sw / size).min(sw - 1);
            let sy = (y * sh / size).min(sh - 1);
            let (si, di) = (((sy * sw + sx) * 4) as usize, ((y * size + x) * 4) as usize);
            out.data[di..di + 4].copy_from_slice(&src.data[si..si + 4]);
        }
    }
    out
}

/// Shared icon cache keyed `(desktop_id, size)`.
static CACHE: Mutex<Option<HashMap<(String, i32), Pixels>>> = Mutex::new(None);

/// Cached icon raster for `desktop_id` (or `icon_name` lookup key).
#[must_use]
pub fn icon(desktop_id: &str, icon_name: &str, label: &str, size: i32) -> Pixels {
    let key = (desktop_id.to_string(), size);
    let mut g = CACHE.lock().unwrap();
    let map = g.get_or_insert_with(HashMap::new);
    if let Some(px) = map.get(&key) {
        return px.clone();
    }
    let px = rasterize_icon(desktop_id, icon_name, label, size);
    map.insert(key, px.clone());
    px
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rabbiz_detection() {
        assert!(is_rabbiz_app("a21s-calculator"));
        assert!(is_rabbiz_app("rabbiz-de"));
        assert!(!is_rabbiz_app("org.kde.dolphin"));
        assert!(!is_rabbiz_app("google-chrome"));
    }

    #[test]
    fn tile_is_sand_with_glyph() {
        let px = tile_icon("Calculator", 96);
        assert_eq!(px.size, Size::new(96, 96));
        // Corner radius area should be transparent (outside the tile).
        assert_eq!(px.data[3], 0);
        // Center should be sand tile or glyph ink — not transparent.
        let mid = ((48 * 96 + 48) * 4) as usize;
        assert!(px.data[mid + 3] > 0);
    }

    #[test]
    fn third_party_falls_back_to_tile_when_missing() {
        let px = rasterize_icon("org.kde.nosuch", "definitely-not-an-icon", "X", 96);
        assert_eq!(px.size, Size::new(96, 96));
    }
}
