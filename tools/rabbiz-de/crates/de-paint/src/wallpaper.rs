//! Wallpaper loading + parallax (M3-T7/M3-T2).
//!
//! Resolution order for [`wallpaper`]: `<dir>/<id>.png` → procedural
//! gradient for that id → procedural `ember`. The procedural path is fully
//! deterministic, so host goldens never depend on shipped assets.
//!
//! Every wallpaper gets the §7.17 2 % grain overlay (deterministic hash
//! noise — kills banding in the smooth gradients).

use std::path::Path;

use de_api::{Pixels, Size};
use de_design::tokens;

use crate::image::decode_png;

const EMBER: &[u8] = include_bytes!("../../../assets/wallpapers/ember.png");

/// The compositor's built-in wallpaper. Sunset is the legacy Ember choice.
#[must_use]
pub fn bundled(id: &str, size: Size) -> Pixels {
    if matches!(id, tokens::WALLPAPER_DEFAULT | "sunset") {
        if let Ok(pixels) = decode_png(EMBER) {
            return if pixels.size == size {
                pixels
            } else {
                cover_scale(&pixels, size)
            };
        }
    }
    wallpaper(None, id, size)
}

/// Per-id gradient stops (top → mid → bottom), premultiplied-ready sRGB.
/// Tuned to sit behind Cream Glass: warm, low-saturation, darker up top.
fn stops(id: &str) -> Option<[[u8; 3]; 3]> {
    Some(match id {
        "ember" => [[0x3A, 0x2A, 0x43], [0xB8, 0x68, 0x3A], [0xF4, 0xED, 0xE1]],
        "dusk" => [[0x2A, 0x1D, 0x2F], [0x22, 0x16, 0x26], [0x10, 0x0C, 0x14]],
        "sand" => [[0xF4, 0xED, 0xE1], [0xEA, 0xDD, 0xC9], [0xDA, 0xC9, 0xB3]],
        "aurora" => [[0x1E, 0x2A, 0x2E], [0x3F, 0x9E, 0x8A], [0xE8, 0xD9, 0xB8]],
        "moon" => [[0x1A, 0x1E, 0x2E], [0x4A, 0x5A, 0x8A], [0xC9, 0xC4, 0xD4]],
        "oled" => [[0x0A, 0x0A, 0x0C], [0x06, 0x06, 0x08], [0x00, 0x00, 0x00]],
        _ => return None,
    })
}

/// Deterministic three-stop vertical gradient for `id` (unknown → ember).
#[must_use]
pub fn procedural(id: &str, size: Size) -> Pixels {
    let s = stops(id)
        .or_else(|| stops(tokens::WALLPAPER_DEFAULT))
        .unwrap();
    let mut px = Pixels::empty(size);
    let h = (size.h.max(1) - 1) as f32;
    for y in 0..size.h.max(0) {
        let t = y as f32 / h;
        let (a, b, u) = if t < 0.5 {
            (s[0], s[1], t * 2.0)
        } else {
            (s[1], s[2], (t - 0.5) * 2.0)
        };
        let r = lerp(a[0], b[0], u);
        let g = lerp(a[1], b[1], u);
        let bl = lerp(a[2], b[2], u);
        for x in 0..size.w.max(0) {
            let i = ((y * size.w + x) * 4) as usize;
            px.data[i] = r;
            px.data[i + 1] = g;
            px.data[i + 2] = bl;
            px.data[i + 3] = 0xFF;
        }
    }
    px
}

fn lerp(a: u8, b: u8, t: f32) -> u8 {
    (f32::from(a) + (f32::from(b) - f32::from(a)) * t.clamp(0.0, 1.0) + 0.5) as u8
}

/// Load `<dir>/<id>.png`, cover-scale to `size`. Returns `None` when the
/// asset is absent or undecodable.
#[must_use]
pub fn load(dir: &Path, id: &str, size: Size) -> Option<Pixels> {
    let bytes = std::fs::read(dir.join(format!("{id}.png"))).ok()?;
    let px = decode_png(&bytes).ok()?;
    Some(cover_scale(&px, size))
}

/// Wallpaper for `id`: asset when present, procedural gradient otherwise.
#[must_use]
pub fn wallpaper(dir: Option<&Path>, id: &str, size: Size) -> Pixels {
    let mut px = dir
        .and_then(|d| load(d, id, size))
        .or_else(|| {
            // Unknown id → try the default id's asset before going procedural.
            if id == tokens::WALLPAPER_DEFAULT {
                None
            } else {
                dir.and_then(|d| load(d, tokens::WALLPAPER_DEFAULT, size))
            }
        })
        .unwrap_or_else(|| procedural(id, size));
    apply_grain(&mut px, tokens::WALLPAPER_GRAIN);
    px
}

/// Cover-scale `src` to exactly `size` (crop the overflow, keep aspect).
/// Nearest-neighbour — wallpapers are gradients/photos, jaggies don't show.
#[must_use]
pub fn cover_scale(src: &Pixels, size: Size) -> Pixels {
    let (sw, sh) = (src.size.w.max(1), src.size.h.max(1));
    let scale = (size.w as f32 / sw as f32).max(size.h as f32 / sh as f32);
    let (tw, th) = ((sw as f32 * scale) as i32, (sh as f32 * scale) as i32);
    let (ox, oy) = ((tw - size.w).max(0) / 2, (th - size.h).max(0) / 2);
    let mut out = Pixels::empty(size);
    for y in 0..size.h.max(0) {
        for x in 0..size.w.max(0) {
            let sx = (((x + ox) as f32 / scale) as i32).clamp(0, sw - 1);
            let sy = (((y + oy) as f32 / scale) as i32).clamp(0, sh - 1);
            let (si, di) = (
                ((sy * sw + sx) * 4) as usize,
                ((y * size.w + x) * 4) as usize,
            );
            out.data[di..di + 4].copy_from_slice(&src.data[si..si + 4]);
        }
    }
    out
}

/// Crop a `size` frame out of a wider wallpaper for the parallax shift.
/// `shift_px` is signed (−`max_shift..=max_shift`); the source must be a`t`
/// least `size.w + 2 * max_shift` wide (use [`cover_scale`] to prepare it).
#[must_use]
pub fn parallax_frame(src: &Pixels, size: Size, max_shift: i32, shift_px: i32) -> Pixels {
    let spare = (src.size.w - size.w).max(0);
    let center = spare / 2;
    let shift = shift_px.clamp(-max_shift, max_shift);
    let x0 = (center + shift).clamp(0, spare);
    let mut out = Pixels::empty(size);
    for y in 0..size.h.min(src.size.h).max(0) {
        let (si, di) = (
            ((y * src.size.w + x0) * 4) as usize,
            ((y * size.w) * 4) as usize,
        );
        let n = (size.w.min(src.size.w - x0).max(0) * 4) as usize;
        out.data[di..di + n].copy_from_slice(&src.data[si..si + n]);
    }
    out
}

/// §7.17 grain: ±amount luminance noise per pixel, deterministic (hash of
/// coordinates — no RNG, no state, same frame every run).
pub fn apply_grain(px: &mut Pixels, amount: f32) {
    let amp = (amount * 255.0) as i32;
    if amp <= 0 {
        return;
    }
    let (w, h) = (px.size.w, px.size.h);
    for y in 0..h.max(0) {
        for x in 0..w.max(0) {
            let i = ((y * w + x) * 4) as usize;
            // Small integer hash; output in [-amp, amp].
            let mut hsh =
                (x.wrapping_mul(2_654_435_761u32 as i32)) ^ (y.wrapping_mul(97_073)) ^ 0x5BF0_3635;
            hsh ^= hsh >> 13;
            let noise = (hsh % (2 * amp + 1)) - amp;
            for c in 0..3 {
                let v = (i32::from(px.data[i + c]) + noise).clamp(0, 255);
                px.data[i + c] = v as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_ember_replaces_the_legacy_sunset_placeholder() {
        let size = Size::new(720, 1600);
        let image = decode_png(EMBER).expect("embedded Ember image decodes");
        assert_eq!(image.size, size);
        assert_eq!(bundled("sunset", size), image);
        assert_eq!(bundled(tokens::WALLPAPER_DEFAULT, size), image);
    }

    #[test]
    fn procedural_is_deterministic() {
        let a = procedural("ember", Size::new(32, 32));
        let b = procedural("ember", Size::new(32, 32));
        assert_eq!(a.data, b.data);
        // Top ≠ bottom (it's a gradient).
        assert_ne!(a.data[0], a.data[((31 * 32) * 4) as usize]);
    }

    #[test]
    fn unknown_id_uses_ember_stops() {
        let a = procedural("nonsense", Size::new(8, 8));
        let b = procedural("ember", Size::new(8, 8));
        assert_eq!(a.data, b.data);
    }

    #[test]
    fn cover_scale_output_size() {
        let src = procedural("dusk", Size::new(100, 50));
        let out = cover_scale(&src, Size::new(40, 40));
        assert_eq!(out.size, Size::new(40, 40));
    }

    #[test]
    fn parallax_crops_shifted_region() {
        // Column-varying source so a horizontal crop is observable.
        let mut src = Pixels::empty(Size::new(200, 100));
        for x in 0..200 {
            for y in 0..100 {
                let i = ((y * 200 + x) * 4) as usize;
                src.data[i] = x as u8;
                src.data[i + 3] = 0xFF;
            }
        }
        let a = parallax_frame(&src, Size::new(160, 100), 20, 0);
        let b = parallax_frame(&src, Size::new(160, 100), 20, 20);
        assert_eq!(a.size, Size::new(160, 100));
        assert_eq!(a.data[0], 20); // centered: spare 40 → x0 = 20
        assert_eq!(b.data[0], 40); // shifted right by max_shift
                                   // Clamp: beyond max behaves exactly like max.
        let c = parallax_frame(&src, Size::new(160, 100), 20, 500);
        assert_eq!(b.data, c.data);
    }

    #[test]
    fn grain_is_deterministic_and_small() {
        let mut a = procedural("moon", Size::new(16, 16));
        let orig = a.data.clone();
        apply_grain(&mut a, 0.02);
        let mut b = Pixels::new(Size::new(16, 16), orig.clone());
        apply_grain(&mut b, 0.02);
        assert_eq!(a.data, b.data);
        // Max deviation ≤ ~5 for 2 % grain.
        for (x, y) in a.data.iter().zip(orig.iter()) {
            assert!((i32::from(*x) - i32::from(*y)).abs() <= 6);
        }
    }
}
