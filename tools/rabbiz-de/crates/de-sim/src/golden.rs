//! Golden PNG comparison in straight RGBA8.
//!
//! Compares a rendered `tiny_skia::Pixmap` against a reference PNG on disk.
//! Both are converted to straight RGBA8 (`demultiply()`) before comparing:
//! - A pixel counts as differing if any channel differs by more than `2 / 255`.
//! - Comparison fails if differing pixels exceed `1%` or if mean absolute
//!   error (MAE) across all channels exceeds `1 / 255`.

use std::path::Path;

use tiny_skia::Pixmap;

/// Maximum per-channel delta (in 0..=255) before a pixel is counted as differing.
pub const MAX_CHANNEL_DELTA: u8 = 2;

/// Maximum fraction of differing pixels allowed (1%).
pub const MAX_DIFFERING_RATIO: f64 = 0.01;

/// Maximum mean absolute error per channel in 0..=255 units (`1 / 255` normalized).
pub const MAX_MAE_255: f64 = 1.0;

#[derive(Debug, Clone, PartialEq)]
pub struct CompareReport {
    pub width: u32,
    pub height: u32,
    pub total_pixels: usize,
    pub differing_pixels: usize,
    pub differing_ratio: f64,
    pub mae_255: f64,
}

impl CompareReport {
    pub fn passed(&self) -> bool {
        self.differing_ratio <= MAX_DIFFERING_RATIO && self.mae_255 <= MAX_MAE_255
    }
}

/// Convert a premultiplied `tiny_skia::Pixmap` into straight RGBA8 bytes.
pub fn pixmap_to_straight_rgba(pixmap: &Pixmap) -> Vec<u8> {
    let mut out = Vec::with_capacity(pixmap.pixels().len() * 4);
    for px in pixmap.pixels() {
        let straight = px.demultiply();
        out.push(straight.red());
        out.push(straight.green());
        out.push(straight.blue());
        out.push(straight.alpha());
    }
    out
}

/// Compare two straight-RGBA8 buffers of identical dimensions.
pub fn compare_straight_rgba(
    actual: &[u8],
    actual_size: (u32, u32),
    expected: &[u8],
    expected_size: (u32, u32),
) -> Result<CompareReport, String> {
    if actual_size != expected_size {
        return Err(format!(
            "dimension mismatch: actual {}x{}, expected {}x{}",
            actual_size.0, actual_size.1, expected_size.0, expected_size.1
        ));
    }
    let (width, height) = actual_size;
    let total_pixels = (width as usize) * (height as usize);
    let expected_len = total_pixels * 4;
    if actual.len() != expected_len || expected.len() != expected_len {
        return Err(format!(
            "buffer length mismatch: actual {}, expected {}, want {}",
            actual.len(),
            expected.len(),
            expected_len
        ));
    }
    if total_pixels == 0 {
        return Ok(CompareReport {
            width,
            height,
            total_pixels: 0,
            differing_pixels: 0,
            differing_ratio: 0.0,
            mae_255: 0.0,
        });
    }

    let mut differing_pixels = 0usize;
    let mut sum_abs_err = 0u64;

    for (a, e) in actual.chunks_exact(4).zip(expected.chunks_exact(4)) {
        let dr = a[0].abs_diff(e[0]);
        let dg = a[1].abs_diff(e[1]);
        let db = a[2].abs_diff(e[2]);
        let da = a[3].abs_diff(e[3]);

        if dr > MAX_CHANNEL_DELTA
            || dg > MAX_CHANNEL_DELTA
            || db > MAX_CHANNEL_DELTA
            || da > MAX_CHANNEL_DELTA
        {
            differing_pixels += 1;
        }
        sum_abs_err += u64::from(dr) + u64::from(dg) + u64::from(db) + u64::from(da);
    }

    let differing_ratio = differing_pixels as f64 / total_pixels as f64;
    let mae_255 = sum_abs_err as f64 / (total_pixels as f64 * 4.0);

    Ok(CompareReport {
        width,
        height,
        total_pixels,
        differing_pixels,
        differing_ratio,
        mae_255,
    })
}

/// Load a golden PNG from `path` and compare it against `rendered`.
pub fn compare_against_png(rendered: &Pixmap, golden_path: &Path) -> Result<CompareReport, String> {
    let golden = Pixmap::load_png(golden_path)
        .map_err(|err| format!("load golden {}: {err}", golden_path.display()))?;
    let actual_rgba = pixmap_to_straight_rgba(rendered);
    let expected_rgba = pixmap_to_straight_rgba(&golden);
    compare_straight_rgba(
        &actual_rgba,
        (rendered.width(), rendered.height()),
        &expected_rgba,
        (golden.width(), golden.height()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_buffers_pass() {
        let buf = vec![10, 20, 30, 255, 40, 50, 60, 255];
        let report = compare_straight_rgba(&buf, (2, 1), &buf, (2, 1)).unwrap();
        assert_eq!(report.differing_pixels, 0);
        assert_eq!(report.mae_255, 0.0);
        assert!(report.passed());
    }

    #[test]
    fn small_delta_within_threshold_passes() {
        let expected = vec![100u8; 400]; // 100 pixels
        let mut actual = expected.clone();
        // 1 pixel with delta 2 (<= MAX_CHANNEL_DELTA)
        actual[0] = 102;
        let report = compare_straight_rgba(&actual, (10, 10), &expected, (10, 10)).unwrap();
        assert_eq!(report.differing_pixels, 0);
        assert!(report.mae_255 <= MAX_MAE_255);
        assert!(report.passed());
    }

    #[test]
    fn differing_over_one_percent_fails() {
        let expected = vec![100u8; 400]; // 100 pixels
        let mut actual = expected.clone();
        // 2 pixels with delta 3 (> 2) => 2% > 1%
        actual[0] = 103;
        actual[4] = 103;
        let report = compare_straight_rgba(&actual, (10, 10), &expected, (10, 10)).unwrap();
        assert_eq!(report.differing_pixels, 2);
        assert!(!report.passed());
    }

    #[test]
    fn mae_over_one_per_255_fails_even_with_zero_differing_pixels() {
        let expected = vec![100u8; 400];
        let actual = vec![102u8; 400]; // every channel differs by 2 (<= 2), so MAE = 2.0 > 1.0
        let report = compare_straight_rgba(&actual, (10, 10), &expected, (10, 10)).unwrap();
        assert_eq!(report.differing_pixels, 0);
        assert_eq!(report.mae_255, 2.0);
        assert!(!report.passed());
    }

    #[test]
    fn dimension_mismatch_errors() {
        let a = vec![0u8; 16];
        let b = vec![0u8; 16];
        assert!(compare_straight_rgba(&a, (2, 2), &b, (4, 1)).is_err());
    }
}
