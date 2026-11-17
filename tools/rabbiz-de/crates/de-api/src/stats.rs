//! Frame statistics shared by the compositor, the CLI and the bench scripts.
//!
//! Serialization schema is frozen by plan §7.21: per-second JSONL records go to
//! `--stats-file`; the run summary goes to `--stats-summary` / `--stats` stdout.

use serde::{Deserialize, Serialize};

use crate::geom::Size;

/// Which rasterizer/compositor path is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RendererKind {
    Gl,
    Soft,
}

impl std::fmt::Display for RendererKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Gl => "gl",
            Self::Soft => "soft",
        })
    }
}

/// Output description reported by the backend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputInfo {
    /// Connector name, e.g. `"DSI-1"`.
    pub name: String,
    pub width: u32,
    pub height: u32,
    /// Refresh in Hz (e.g. 60.0).
    pub refresh_hz: f64,
    /// Output scale; v1 is always 1.0 (plan Decision 7).
    pub scale: f64,
}

impl OutputInfo {
    #[must_use]
    pub fn size(&self) -> Size {
        Size::new(self.width as i32, self.height as i32)
    }

    /// Expected vblank interval in milliseconds.
    #[must_use]
    pub fn frame_interval_ms(&self) -> f64 {
        if self.refresh_hz > 0.0 {
            1000.0 / self.refresh_hz
        } else {
            16.67
        }
    }
}

/// Percentile set for a millisecond timing series.
#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TimingSummary {
    pub p50: f64,
    pub p95: f64,
    pub max: f64,
}

/// One JSONL record emitted per elapsed second to `--stats-file`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameStats {
    /// Milliseconds since run start at the end of this window.
    pub t_ms: u64,
    pub frames: u64,
    /// Frame preparation + commit time.
    pub cpu_ms: TimingSummary,
    /// GPU time via EGL timer query on GL; `null` on soft.
    pub gpu_ms: Option<f64>,
    /// Pixels submitted in the window.
    pub damage_px: u64,
    pub layers: u32,
    /// Live texture bytes at window end, in MiB.
    pub textures_mb: f64,
    pub cma_free_kb: u64,
    pub rss_kb: u64,
    /// Cumulative counters since run start.
    pub direct_scanout: u64,
    pub primary_composite: u64,
    /// Frames whose vblank interval exceeded 1.5x `frame_interval_ms`.
    pub missed_frames: u64,
    pub renderer: RendererKind,
}

/// Final run summary written to `--stats-summary` / printed by `--stats`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsSummary {
    pub scenario: String,
    pub duration_ms: u64,
    pub frames: u64,
    pub cpu_ms: TimingSummary,
    pub gpu_ms: Option<f64>,
    pub missed_frames: u64,
    pub rss_kb_peak: u64,
    pub cma_free_min_kb: u64,
    pub renderer: RendererKind,
}
