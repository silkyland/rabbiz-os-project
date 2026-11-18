//! Golden-image comparison (M0-T7).
//!
//! Comparator contract: composite both images over the scene background,
//! compare straight RGBA, count a pixel as differing when any channel differs
//! by > 2/255, fail when differing pixels > 1% or MAE > 1/255.

use de_api::Pixels;
use de_paint::{decode_png, encode_png};

/// Background the transparent regions are composited over (theme `bg`).
const BACKDROP: [u8; 3] = [0x16, 0x10, 0x1A];

/// Result of comparing a render against a golden.
#[derive(Debug, Clone, Copy)]
pub struct Compare {
    /// Fraction of pixels differing (>2/255 any channel), 0..=1.
    pub diff_frac: f64,
    /// Mean absolute error per channel, 0..=1.
    pub mae: f64,
    pub pass: bool,
}

impl std::fmt::Display for Compare {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "diff={:.4}% mae={:.5} {}",
            self.diff_frac * 100.0,
            self.mae,
            if self.pass { "PASS" } else { "FAIL" }
        )
    }
}

fn composite_straight(px: &Pixels) -> Vec<u8> {
    // Pixels are premultiplied; composite over BACKDROP: out = src + bg*(1-a).
    let mut out = Vec::with_capacity(px.data.len());
    for c in px.data.as_chunks::<4>().0 {
        let a = u32::from(c[3]);
        let inv = 255 - a;
        for k in 0..3 {
            out.push(((u32::from(c[k]) * 255 + u32::from(BACKDROP[k]) * inv) / 255).min(255) as u8);
        }
        out.push(255);
    }
    out
}

/// Compare `actual` against the golden PNG at `golden_path`.
///
/// Returns `Err` when the golden cannot be read or sizes mismatch.
pub fn compare(actual: &Pixels, golden_path: &std::path::Path) -> Result<Compare, String> {
    let golden_bytes =
        std::fs::read(golden_path).map_err(|e| format!("read {}: {e}", golden_path.display()))?;
    let golden = decode_png(&golden_bytes).map_err(|e| format!("decode golden: {e}"))?;
    if golden.size != actual.size {
        return Err(format!(
            "size mismatch: actual {}x{} vs golden {}x{}",
            actual.size.w, actual.size.h, golden.size.w, golden.size.h
        ));
    }
    let a = composite_straight(actual);
    let g = composite_straight(&golden);
    let px_count = (actual.size.area().max(1)) as u64;
    let (mut diff_px, mut err_sum) = (0u64, 0u64);
    for (ca, cg) in a.as_chunks::<4>().0.iter().zip(g.as_chunks::<4>().0.iter()) {
        let mut maxd = 0u64;
        for k in 0..3 {
            let d = u64::from(ca[k].abs_diff(cg[k]));
            maxd = maxd.max(d);
            err_sum += d;
        }
        if maxd > 2 {
            diff_px += 1;
        }
    }
    let diff_frac = diff_px as f64 / px_count as f64;
    let mae = err_sum as f64 / (px_count as f64 * 3.0 * 255.0);
    Ok(Compare {
        diff_frac,
        mae,
        pass: diff_frac <= 0.01 && mae <= 1.0 / 255.0,
    })
}

/// Write `pixels` as PNG to `path` (creating parent dirs).
pub fn write_png(pixels: &Pixels, path: &std::path::Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
    }
    let png = encode_png(pixels).map_err(|e| format!("encode: {e}"))?;
    std::fs::write(path, png).map_err(|e| format!("write {}: {e}", path.display()))
}
