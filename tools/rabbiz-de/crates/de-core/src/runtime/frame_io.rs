//! Frame-loop I/O & telemetry helpers extracted from `runtime/frame.rs`.

use std::fs::File;
use std::io::Write;

use de_api::TimingSummary;
use smithay::reexports::wayland_server::Resource;
use smithay::wayland::seat::WaylandFocus;

/// Compute p50/p95/max percentiles over a sample slice (ms).
#[must_use]
pub(crate) fn summarize(v: &[f64]) -> TimingSummary {
    if v.is_empty() {
        return TimingSummary::default();
    }
    let mut s = v.to_vec();
    s.sort_by(f64::total_cmp);
    let p = |q: f64| s[((s.len() - 1) as f64 * q) as usize];
    TimingSummary {
        p50: p(0.50),
        p95: p(0.95),
        max: s.last().copied().unwrap_or(0.0),
    }
}

/// Write `$XDG_RUNTIME_DIR/rabbiz-de.ready` after the first successful page flip.
pub(crate) fn write_ready_file() {
    if let Ok(dir) = std::env::var("XDG_RUNTIME_DIR") {
        let pid = std::process::id();
        let ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64);
        let _ = std::fs::write(format!("{dir}/rabbiz-de.ready"), format!("{pid} {ms}"));
    }
}

/// Touch `$XDG_RUNTIME_DIR/rabbiz-de.shot` → dump the composited frame
/// (shell/app + layer surfaces, mirrored from `composite_output`) as
/// `rabbiz-de-shot.ppm`. dmabuf/EGL client content is not readable on CPU
/// and shows as the clear colour.
pub(crate) fn dump_frame_if_triggered(de: &crate::state::De) {
    let Ok(dir) = std::env::var("XDG_RUNTIME_DIR") else {
        return;
    };
    let trigger = format!("{dir}/rabbiz-de.shot");
    if std::fs::metadata(&trigger).is_err() {
        return;
    }
    let _ = std::fs::remove_file(&trigger);
    let census: Vec<String> = de
        .space
        .elements()
        .map(|w| {
            w.wl_surface().map_or_else(
                || "no-surface".into(),
                |s| format!("{:?} alive={}", s.id(), s.is_alive()),
            )
        })
        .collect();
    eprintln!(
        "rabbiz-de: shot census foreground={:?} space=[{}]",
        de.foreground,
        census.join(", ")
    );
    let Some(px) = crate::composite::composite_output(de) else {
        return;
    };
    let path = format!("{dir}/rabbiz-de-shot.ppm");
    if let Ok(mut f) = File::create(&path) {
        let _ = write!(f, "P6\n{} {}\n255\n", px.size.w, px.size.h);
        // Pixels are premultiplied RGBA — flatten over opaque black.
        let mut rgb = Vec::with_capacity((px.size.w * px.size.h * 3) as usize);
        for p in px.data.as_chunks::<4>().0 {
            let a = u16::from(p[3]) + 1;
            rgb.push((u16::from(p[0]) * a / 256) as u8);
            rgb.push((u16::from(p[1]) * a / 256) as u8);
            rgb.push((u16::from(p[2]) * a / 256) as u8);
        }
        let _ = f.write_all(&rgb);
    }
}

/// Whole minutes since UNIX epoch — used to trigger status-bar clock re-raster.
#[must_use]
pub(crate) fn minute_of_day() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() / 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarize_empty_and_percentiles() {
        assert_eq!(summarize(&[]).max, 0.0);
        let s = summarize(&[10.0, 2.0, 8.0, 4.0, 6.0]);
        assert!((s.p50 - 6.0).abs() < f64::EPSILON);
        assert!((s.max - 10.0).abs() < f64::EPSILON);
    }
}
