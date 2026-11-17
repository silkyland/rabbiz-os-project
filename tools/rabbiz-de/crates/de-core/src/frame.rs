//! Frame pump + stats (M0-T4): present the brand layer every vblank, record
//! per-second JSONL stats, detect missed frames, emit the final summary.

use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use de_api::{FrameStats, StatsSummary, TimingSummary};
use de_render::{DeRenderer, DeTexture};
use smithay::backend::drm::compositor::FrameFlags;
use smithay::backend::renderer::element::texture::TextureRenderElement;
use smithay::backend::renderer::element::Kind;
use smithay::reexports::calloop::EventLoop;

use crate::state::{cma_free_kb, rss_kb, De};

/// Runtime context held across frames.
pub struct Run {
    pub de: De,
    /// JSONL stats sink (per-second records).
    stats_file: Option<BufWriter<File>>,
    summary_path: Option<std::path::PathBuf>,
    scenario: String,
    duration: Option<Duration>,
    // accumulators
    frames: u64,
    window_frames: u64,
    cpu_samples: Vec<f64>,
    all_cpu: Vec<f64>,
    missed: u64,
    rss_peak: u64,
    cma_min: u64,
    last_vblank: Option<Instant>,
    last_second: Instant,
    direct_scanout: u64,
    primary_composite: u64,
}

impl Run {
    pub fn new(
        de: De,
        stats_file: Option<&Path>,
        summary_path: Option<std::path::PathBuf>,
        scenario: String,
        duration: Option<Duration>,
    ) -> std::io::Result<Self> {
        let stats_file = stats_file
            .map(|p| {
                OpenOptions::new()
                    .create(true)
                    .write(true)
                    .truncate(true)
                    .open(p)
            })
            .transpose()?
            .map(BufWriter::new);
        Ok(Self {
            de,
            stats_file,
            summary_path,
            scenario,
            duration,
            frames: 0,
            window_frames: 0,
            cpu_samples: Vec::with_capacity(128),
            all_cpu: Vec::with_capacity(4096),
            missed: 0,
            rss_peak: 0,
            cma_min: u64::MAX,
            last_vblank: None,
            last_second: Instant::now(),
            direct_scanout: 0,
            primary_composite: 0,
        })
    }

    /// Render + queue one frame on the output. Alternates the brand
    /// position by one physical pixel so every frame carries damage and
    /// the vblank chain stays alive for the duration of a bench run.
    pub fn present(&mut self) -> Result<(), String> {
        let t0 = Instant::now();
        let x = (self.frames % 2) as f64;
        match (&mut self.de.renderer, &self.de.brand) {
            (DeRenderer::Gl(r), DeTexture::Gl(buf)) => {
                let el = TextureRenderElement::from_texture_buffer(
                    smithay::utils::Point::<f64, smithay::utils::Physical>::from((x, 0.0)),
                    buf,
                    None,
                    None,
                    None,
                    Kind::Unspecified,
                );
                self.de
                    .compositor
                    .render_frame(r, &[el], [0.086, 0.063, 0.102, 1.0], FrameFlags::DEFAULT)
                    .map_err(|e| format!("render: {e:?}"))?;
            }
            (DeRenderer::Soft(r), DeTexture::Soft(buf)) => {
                let el = TextureRenderElement::from_texture_buffer(
                    smithay::utils::Point::<f64, smithay::utils::Physical>::from((x, 0.0)),
                    buf,
                    None,
                    None,
                    None,
                    Kind::Unspecified,
                );
                self.de
                    .compositor
                    .render_frame(r, &[el], [0.086, 0.063, 0.102, 1.0], FrameFlags::DEFAULT)
                    .map_err(|e| format!("render: {e:?}"))?;
            }
            _ => return Err("renderer/texture kind mismatch".to_string()),
        }
        // EmptyFrame means the queued frame carried no damage — nothing to
        // commit, not an error.
        if let Err(e) = self.de.compositor.queue_frame(()) {
            if !format!("{e:?}").contains("EmptyFrame") {
                return Err(format!("queue: {e:?}"));
            }
        }
        let cpu = t0.elapsed().as_secs_f64() * 1000.0;
        self.cpu_samples.push(cpu);
        self.all_cpu.push(cpu);
        self.primary_composite += 1;
        Ok(())
    }

    /// Called on each vblank: release the pending frame, account timing,
    /// emit stats, request the next frame.
    pub fn on_vblank(&mut self) {
        if let Err(e) = self.de.compositor.frame_submitted() {
            eprintln!("frame_submitted: {e:?}");
        }
        let now = Instant::now();
        if let Some(last) = self.last_vblank.replace(now) {
            let dt = last.elapsed().as_secs_f64() * 1000.0;
            let expected = self.de.output.refresh_hz.max(1.0);
            if dt > 1.5 * (1000.0 / expected) {
                self.missed += 1;
            }
        }
        self.frames += 1;
        self.window_frames += 1;
        if !self.de.first_flip {
            self.de.first_flip = true;
            write_ready_file();
        }
        self.rss_peak = self.rss_peak.max(rss_kb());
        self.cma_min = self.cma_min.min(cma_free_kb());

        if now.duration_since(self.last_second) >= Duration::from_secs(1) {
            self.flush_second(now);
        }
        let _ = self.present().map_err(|e| eprintln!("present: {e}"));
    }

    fn flush_second(&mut self, now: Instant) {
        if let Some(f) = &mut self.stats_file {
            let rec = FrameStats {
                t_ms: self.de.started.elapsed().as_millis() as u64,
                frames: self.window_frames,
                cpu_ms: summarize(&self.cpu_samples),
                gpu_ms: None,
                damage_px: (self.de.output.width * self.de.output.height) as u64
                    * self.window_frames,
                layers: 1,
                textures_mb: 0.0,
                cma_free_kb: cma_free_kb(),
                rss_kb: rss_kb(),
                direct_scanout: self.direct_scanout,
                primary_composite: self.primary_composite,
                missed_frames: self.missed,
                renderer: self.de.kind(),
            };
            if let Ok(line) = serde_json::to_string(&rec) {
                let _ = writeln!(f, "{line}");
                let _ = f.flush();
            }
        }
        self.window_frames = 0;
        self.cpu_samples.clear();
        self.last_second = now;
    }

    /// Emit the run summary to `--stats-summary` and (with `--stats`) stdout.
    pub fn finish(&self, print: bool) {
        let summary = StatsSummary {
            scenario: self.scenario.clone(),
            duration_ms: self.de.started.elapsed().as_millis() as u64,
            frames: self.frames,
            cpu_ms: summarize(&self.all_cpu),
            gpu_ms: None,
            missed_frames: self.missed,
            rss_kb_peak: self.rss_peak,
            cma_free_min_kb: if self.cma_min == u64::MAX {
                0
            } else {
                self.cma_min
            },
            renderer: self.de.kind(),
        };
        if let Some(p) = &self.summary_path {
            if let Ok(s) = serde_json::to_string(&summary) {
                let _ = std::fs::write(p, s);
            }
        }
        if print {
            if let Ok(s) = serde_json::to_string(&summary) {
                println!("{s}");
            }
        }
    }
}

fn summarize(v: &[f64]) -> TimingSummary {
    if v.is_empty() {
        return TimingSummary::default();
    }
    let mut s = v.to_vec();
    s.sort_by(f64::total_cmp);
    let p = |q: f64| s[((s.len() - 1) as f64 * q) as usize];
    TimingSummary {
        p50: p(0.50),
        p95: p(0.95),
        max: *s.last().unwrap(),
    }
}

fn write_ready_file() {
    if let Ok(dir) = std::env::var("XDG_RUNTIME_DIR") {
        let pid = std::process::id();
        let ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let _ = std::fs::write(format!("{dir}/rabbiz-de.ready"), format!("{pid} {ms}"));
    }
}

/// Run the event loop until `run.duration` elapses; without a duration the
/// loop runs until the process is signaled (real session mode).
pub fn event_loop(mut run: Run, print_stats: bool) -> Result<u8, String> {
    let mut event_loop: EventLoop<Run> =
        EventLoop::try_new().map_err(|e| format!("event loop: {e}"))?;
    let handle = event_loop.handle();

    // libseat notifier: keep the session dispatchable.
    let seat = run
        .de
        .seat_notifier
        .take()
        .ok_or("seat notifier consumed")?;
    handle
        .insert_source(seat, |_, _, _: &mut Run| {})
        .map_err(|e| format!("seat source: {e}"))?;

    // DRM vblank source.
    let notifier = run
        .de
        .setup
        .notifier
        .take()
        .ok_or("drm notifier consumed")?;
    handle
        .insert_source(notifier, |event, _meta, run: &mut Run| {
            if let smithay::backend::drm::DrmEvent::VBlank(_) = event {
                run.on_vblank();
            }
        })
        .map_err(|e| format!("drm source: {e}"))?;

    let deadline = run.duration.map(|d| Instant::now() + d);
    // Kick off the first frame.
    run.present()?;

    loop {
        let wait = deadline.map(|d| d.saturating_duration_since(Instant::now()));
        event_loop
            .dispatch(wait, &mut run)
            .map_err(|e| format!("dispatch: {e}"))?;
        if deadline.is_some_and(|d| Instant::now() >= d) {
            break;
        }
    }
    run.finish(print_stats);
    Ok(0)
}
