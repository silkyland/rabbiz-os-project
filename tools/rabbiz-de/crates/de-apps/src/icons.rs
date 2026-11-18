//! Icon pipeline (M2-T3): resolve a desktop entry's `Icon=` name through the
//! hicolor theme, rasterize to the shell's 96 px box, cache the result.
//!
//! Rules from plan §7.17:
//! - Every app: the themed icon file wins (`Icon=` → hicolor/pixmaps);
//!   first-party `rabbiz-*.svg` ships real per-app glyphs.
//! - No file resolves: 96 px sand tile (#DAC9B3 base, radius 28) with a
//!   plum glyph — first letter of the app name.

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
        // A path recorded under another account (the session user was
        // renamed `gamer` -> `rabbiz`) still points at the same relative
        // file in this user's home, and Chrome's PWA icons are also
        // findable by name in the hicolor tree.
        if let (Some(rest), Some(home)) = (
            icon_name.strip_prefix("/home/").and_then(|r| r.split_once('/')),
            std::env::var_os("HOME").map(PathBuf::from),
        ) {
            out.push(home.join(rest.1));
        }
        if let Some(stem) = Path::new(icon_name).file_stem().and_then(|n| n.to_str()) {
            out.extend(candidates(stem));
        }
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

/// Whether `desktop_id` is a first-party Rabbiz app (`rabbiz-*`; legacy
/// `a21s-*` ids count too so pre-rename entries keep their branding).
#[must_use]
pub fn is_rabbiz_app(desktop_id: &str) -> bool {
    desktop_id.strip_prefix("rabbiz-").is_some() || desktop_id.strip_prefix("a21s-").is_some()
}

/// Rasterize the icon for `desktop_id` + `icon_name` + display `label`
/// (fallback letter) at `size` px. The themed file wins for every app —
/// first-party `rabbiz-*.svg` tiles carry the real per-app glyphs; the
/// sand tile + initial is only the fallback when no file resolves.
#[must_use]
pub fn rasterize_icon(_desktop_id: &str, icon_name: &str, label: &str, size: i32) -> Pixels {
    if let Some(path) = lookup(icon_name) {
        if let Some(px) = rasterize_file(&path, size) {
            return if is_framed(&px) { px } else { put_on_tile(&px) };
        }
    }
    tile_icon(label, size)
}

/// Whether the icon already is a tile: its opaque pixels nearly fill the
/// square (a rounded tile covers ~93 %; a free-standing logo such as a
/// circle covers ~78 % or much less). Alpha is read from premultiplied RGBA.
#[must_use]
pub fn is_framed(px: &Pixels) -> bool {
    let (w, h) = (px.size.w.max(1), px.size.h.max(1));
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, -1, -1);
    let mut opaque = 0i64;
    for y in 0..h {
        for x in 0..w {
            let a = px.data[((y * w + x) * 4 + 3) as usize];
            if a > 24 {
                x0 = x0.min(x);
                x1 = x1.max(x);
                y0 = y0.min(y);
                y1 = y1.max(y);
            }
            if a > 200 {
                opaque += 1;
            }
        }
    }
    if x1 < 0 {
        return false;
    }
    let (bw, bh) = (x1 - x0 + 1, y1 - y0 + 1);
    let spans = bw * 100 >= w * 88 && bh * 100 >= h * 88;
    let filled = opaque * 100 >= i64::from(w) * i64::from(h) * 86;
    spans && filled
}

/// Seat a frameless icon on the shared sand tile (artwork untouched, only
/// scaled to ~64 % and centred) so every icon on the home screen reads as
/// one family.
#[must_use]
pub fn put_on_tile(glyph: &Pixels) -> Pixels {
    let size = glyph.size.w.max(glyph.size.h);
    let t = tokens::DEFAULT;
    let mut c = Canvas::new(Size::new(size, size));
    c.fill_rounded_rect(
        de_api::Rect::new(0, 0, size, size),
        tokens::ICON_TILE_RADIUS * (size as f32 / 96.0),
        t.icon_tile,
    );
    let inner = size * 64 / 100;
    let off = (size - inner) / 2;
    c.blit_scaled(glyph, de_api::Rect::new(off, off, inner, inner), 1.0);
    c.into_pixels()
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
        let (x0, y0, x1, y1) = text::ink_bounds(&px).unwrap_or((0, 0, w, h));
        let p = Pixels::new(Size::new(w, h), px.data().to_vec());
        c.blit(
            &p,
            de_api::Point::new(
                (size - (x1 - x0)) / 2 - x0,
                (size - (y1 - y0)) / 2 - y0,
            ),
        );
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
    #[test]
    fn absolute_icon_path_of_another_user_falls_back_to_this_home_and_name() {
        let c = candidates("/home/gamer/.local/share/icons/hicolor/256x256/apps/chrome-x-Default.png");
        assert_eq!(c[0], PathBuf::from("/home/gamer/.local/share/icons/hicolor/256x256/apps/chrome-x-Default.png"));
        assert!(c.iter().any(|p| p.ends_with(".local/share/icons/hicolor/256x256/apps/chrome-x-Default.png") && !p.starts_with("/home/gamer")));
        assert!(c.iter().any(|p| p.ends_with("hicolor/128x128/apps/chrome-x-Default.png")));
    }

    fn solid(size: i32, f: impl Fn(i32, i32) -> bool) -> Pixels {
        let mut px = Pixels::empty(Size::new(size, size));
        for y in 0..size {
            for x in 0..size {
                if f(x, y) {
                    let i = ((y * size + x) * 4) as usize;
                    px.data[i..i + 4].copy_from_slice(&[200, 40, 40, 255]);
                }
            }
        }
        px
    }

    #[test]
    fn square_tile_is_framed_circle_and_small_glyph_are_not() {
        assert!(is_framed(&solid(96, |_, _| true)));
        // Rounded tile: corners cut.
        assert!(is_framed(&solid(96, |x, y| {
            let (dx, dy) = ((x - 47).abs().max(47 - 28) - (47 - 28), (y - 47).abs().max(47 - 28) - (47 - 28));
            dx * dx + dy * dy <= 28 * 28
        })));
        // Full-bleed circle (a round logo): not a tile.
        assert!(!is_framed(&solid(96, |x, y| (x - 48).pow(2) + (y - 48).pow(2) <= 48 * 48)));
        // Small glyph in the middle, and an empty icon.
        assert!(!is_framed(&solid(96, |x, y| (20..76).contains(&x) && (20..76).contains(&y))));
        assert!(!is_framed(&Pixels::empty(Size::new(96, 96))));
    }

    #[test]
    fn frameless_icons_land_on_the_sand_tile() {
        let glyph = solid(96, |x, y| (x - 48).pow(2) + (y - 48).pow(2) <= 48 * 48);
        let tiled = put_on_tile(&glyph);
        assert_eq!(tiled.size, Size::new(96, 96));
        assert!(is_framed(&tiled), "the result reads as a tile");
        // Corner is transparent (rounded), the margin ring is sand, the centre is the glyph.
        assert_eq!(tiled.data[3], 0);
        let at = |x: i32, y: i32| {
            let i = ((y * 96 + x) * 4) as usize;
            [tiled.data[i], tiled.data[i + 1], tiled.data[i + 2]]
        };
        assert_eq!(at(48, 8), [0xDA, 0xC9, 0xB3]);
        assert_eq!(at(48, 48), [200, 40, 40]);
    }

    use super::*;

    #[test]
    fn rabbiz_detection() {
        assert!(is_rabbiz_app("rabbiz-calculator"));
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
