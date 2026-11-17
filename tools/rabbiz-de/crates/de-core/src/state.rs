use std::path::PathBuf;

use de_api::RendererKind;
use serde::Serialize;

/// 96 MiB CMA floor in KiB for auto-selecting GL vs software rendering.
pub const CMA_GL_FLOOR_KB: u64 = 96 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RendererChoice {
    Auto,
    Gl,
    Soft,
}

#[derive(Debug, Clone)]
pub struct RunConfig {
    pub renderer: RendererChoice,
    pub require_gl: bool,
    pub stats: bool,
    pub stats_file: Option<PathBuf>,
    pub stats_summary: Option<PathBuf>,
    pub scenario: String,
    pub duration: Option<f64>,
    pub force_fail: bool,
    pub spawn: Option<String>,
    /// Write a PNG of the composed scene (GL renderer only) once, at `screenshot_at` seconds.
    pub screenshot: Option<PathBuf>,
    pub screenshot_at: f64,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            renderer: RendererChoice::Auto,
            require_gl: false,
            stats: false,
            stats_file: None,
            stats_summary: None,
            scenario: "boot".to_string(),
            duration: None,
            force_fail: false,
            spawn: None,
            screenshot: None,
            screenshot_at: 3.0,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CpuStats {
    pub p50: f64,
    pub p95: f64,
    pub max: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct FrameStatsLine {
    pub t_ms: u64,
    pub frames: u64,
    pub cpu_ms: CpuStats,
    pub gpu_ms: Option<f64>,
    pub damage_px: u64,
    pub layers: usize,
    pub textures_mb: f64,
    pub cma_free_kb: u64,
    pub rss_kb: u64,
    pub direct_scanout: u64,
    pub primary_composite: u64,
    pub missed_frames: u64,
    pub renderer: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct StatsSummary {
    pub scenario: String,
    pub duration_ms: u64,
    pub frames: u64,
    pub cpu_ms: CpuStats,
    pub gpu_ms: Option<f64>,
    pub missed_frames: u64,
    pub rss_kb_peak: u64,
    pub cma_free_min_kb: u64,
    pub renderer: &'static str,
}

pub fn renderer_label(kind: RendererKind) -> &'static str {
    match kind {
        RendererKind::Gl => "gl",
        RendererKind::Soft => "soft",
    }
}

pub fn choose_initial_renderer(choice: RendererChoice, require_gl: bool, cma_kb: u64) -> RendererKind {
    if require_gl {
        return RendererKind::Gl;
    }
    match choice {
        RendererChoice::Gl => RendererKind::Gl,
        RendererChoice::Soft => RendererKind::Soft,
        RendererChoice::Auto => {
            if cma_kb > 0 && cma_kb < CMA_GL_FLOOR_KB {
                RendererKind::Soft
            } else {
                RendererKind::Gl
            }
        }
    }
}

pub fn summarize_cpu_ms(samples: &[f64]) -> CpuStats {
    if samples.is_empty() {
        return CpuStats {
            p50: 0.0,
            p95: 0.0,
            max: 0.0,
        };
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let n = sorted.len();
    let p50 = sorted[(n - 1) * 50 / 100];
    let p95 = sorted[(n - 1) * 95 / 100];
    let max = sorted[n - 1];
    CpuStats {
        p50: round2(p50),
        p95: round2(p95),
        max: round2(max),
    }
}

pub fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

pub fn read_cma_free_kb() -> u64 {
    let Ok(contents) = std::fs::read_to_string("/proc/meminfo") else {
        return 0;
    };
    for line in contents.lines() {
        if let Some(rest) = line.strip_prefix("CmaFree:") {
            if let Some(num) = rest.split_whitespace().next() {
                if let Ok(kb) = num.parse::<u64>() {
                    return kb;
                }
            }
        }
    }
    0
}

pub fn read_rss_kb() -> u64 {
    let Ok(contents) = std::fs::read_to_string("/proc/self/status") else {
        return 0;
    };
    for line in contents.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            if let Some(num) = rest.split_whitespace().next() {
                if let Ok(kb) = num.parse::<u64>() {
                    return kb;
                }
            }
        }
    }
    0
}
