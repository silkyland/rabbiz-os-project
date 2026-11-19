//! Photo library scanning and local-date helpers.
//!
//! Sources are `~/Pictures` and `~/Downloads`, non-recursive, matching the
//! base-app spec. Dates are rendered in the device's local timezone via
//! `libc::localtime_r` (the only sane way to get TZ on a bare Linux userland
//! without pulling in chrono).

use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// Raster formats the resvg/usvg pipeline behind `de-paint` can decode
/// (JPEG via zune-jpeg, PNG, GIF, WebP — the `raster-images` feature).
pub const IMAGE_EXTS: [&str; 5] = ["png", "jpg", "jpeg", "webp", "gif"];

/// One library item: path plus the metadata the UI shows.
#[derive(Debug, Clone)]
pub struct Photo {
    pub path: PathBuf,
    pub name: String,
    /// Modification time, seconds since the Unix epoch.
    pub mtime: i64,
    pub bytes: u64,
}

#[must_use]
pub fn is_image_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| {
            let e = e.to_ascii_lowercase();
            IMAGE_EXTS.contains(&e.as_str())
        })
        .unwrap_or(false)
}

fn secs_of(md: &fs::Metadata) -> i64 {
    md.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Scan `dirs` (in order) for image files, newest first.
///
/// Sort is by mtime descending, then name ascending for a stable order when
/// timestamps tie (common with bulk copies).
#[must_use]
pub fn scan(dirs: &[PathBuf]) -> Vec<Photo> {
    let mut out = Vec::new();
    for dir in dirs {
        let Ok(rd) = fs::read_dir(dir) else {
            continue;
        };
        for entry in rd.flatten() {
            let p = entry.path();
            if !p.is_file() || !is_image_file(&p) {
                continue;
            }
            let Ok(md) = entry.metadata() else {
                continue;
            };
            let name = p
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("photo")
                .to_string();
            out.push(Photo {
                path: p,
                name,
                mtime: secs_of(&md),
                bytes: md.len(),
            });
        }
    }
    out.sort_by(|a, b| b.mtime.cmp(&a.mtime).then_with(|| a.name.cmp(&b.name)));
    out
}

/// Default library roots for the current user.
#[must_use]
pub fn default_dirs() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"));
    vec![home.join("Pictures"), home.join("Downloads")]
}

// ---------------------------------------------------------------------------
// Local dates (days-from-civil, Howard Hinnant's algorithm — public domain)
// ---------------------------------------------------------------------------

/// Days since 1970-01-01 for a proleptic Gregorian date.
#[must_use]
pub fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = i64::from(y) - i64::from(m <= 2);
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (i64::from(m) + 9) % 12;
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// Inverse of [`days_from_civil`].
#[must_use]
pub fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    ((y + i64::from(m <= 2)) as i32, m, d)
}

/// Local (year, month, day) for `secs` in the device timezone.
#[must_use]
#[allow(unsafe_code)] // localtime_r on owned, zeroed storage — see below.
pub fn local_ymd(secs: i64) -> (i32, u32, u32) {
    // SAFETY: `tm` is a fully-owned, zeroed `libc::tm`; `localtime_r` only
    // reads `t` and writes into `tm`, both valid for the call duration.
    unsafe {
        let t = secs as libc::time_t;
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&t, &mut tm).is_null() {
            return civil_from_days(secs.div_euclid(86_400));
        }
        (tm.tm_year + 1900, (tm.tm_mon + 1) as u32, tm.tm_mday as u32)
    }
}

/// Local day number (days since epoch in local time) — for grouping.
#[must_use]
pub fn day_key(secs: i64) -> i64 {
    let (y, m, d) = local_ymd(secs);
    days_from_civil(y, m, d)
}

const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Section label for a photo timestamp: "Today", "Yesterday", "Mon 12 May",
/// with the year appended when it differs from `now`'s.
#[must_use]
pub fn format_day(secs: i64, now: i64) -> String {
    let key = day_key(secs);
    if key == day_key(now) {
        return "Today".to_string();
    }
    if key == day_key(now) - 1 {
        return "Yesterday".to_string();
    }
    let (y, m, d) = local_ymd(secs);
    let (ny, _, _) = local_ymd(now);
    let mut s = format!("{d} {}", MONTHS[(m - 1) as usize]);
    // Weekday from the day count: 1970-01-01 was a Thursday (index 4).
    let wday = WEEKDAYS[(((key + 4).rem_euclid(7)) as usize) % 7];
    s = format!("{wday} {s}");
    if y != ny {
        s = format!("{s} {y}");
    }
    s
}

/// Human file size for metadata lines ("1.2 MB").
#[must_use]
pub fn format_bytes(n: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{n} B")
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_ext_filter() {
        assert!(is_image_file(Path::new("/x/cat.JPG")));
        assert!(is_image_file(Path::new("/x/a.webp")));
        assert!(!is_image_file(Path::new("/x/a.txt")));
        assert!(!is_image_file(Path::new("/x/noext")));
    }

    #[test]
    fn civil_roundtrip() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 2, 29), 11016);
        for (y, m, d) in [(2024, 2, 29), (2025, 12, 31), (1999, 1, 1), (2038, 7, 4)] {
            let z = days_from_civil(y, m, d);
            assert_eq!(civil_from_days(z), (y, m, d), "{y}-{m}-{d}");
        }
    }

    #[test]
    fn day_key_uses_local_ymd() {
        // Noon UTC is same local day in every TZ from UTC-11..UTC+12.
        let secs = 1_700_000_000; // 2023-11-14T22:13:20Z
        let (y, m, d) = local_ymd(secs);
        assert_eq!(day_key(secs), days_from_civil(y, m, d));
    }

    #[test]
    fn format_day_labels() {
        // now = 2025-06-15T12:00:00Z; pick secs at local noon-ish UTC.
        let now = 1_750_000_000;
        assert_eq!(format_day(now, now), "Today");
        // One local day earlier: subtract 86400 — may straddle DST but the
        // label only needs the common case.
        assert_eq!(format_day(now - 86_400, now), "Yesterday");
        let old = format_day(1_400_000_000, now);
        assert!(old.contains("2014"), "got {old}");
    }

    #[test]
    fn scan_sorts_newest_first() {
        let dir = std::env::temp_dir().join(format!("rabbiz-photos-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("old.png"), b"x").unwrap();
        fs::write(dir.join("new.jpg"), b"x").unwrap();
        fs::write(dir.join("skip.txt"), b"x").unwrap();
        let photos = scan(std::slice::from_ref(&dir));
        assert_eq!(photos.len(), 2);
        assert!(photos[0].name == "new.jpg" || photos[0].name == "old.png");
        assert!(photos.windows(2).all(|w| w[0].mtime >= w[1].mtime));
        let _ = fs::remove_dir_all(&dir);
    }
}
