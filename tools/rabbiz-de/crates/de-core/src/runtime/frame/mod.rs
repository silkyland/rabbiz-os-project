//! Frame pump + stats (M0-T4/M1-T4): present the scene every vblank while
//! damage exists, count direct-scanout vs primary-composite, record
//! per-second JSONL stats, emit the final summary.

mod event_loop;
mod render;

use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use de_api::{FrameStats, StatsSummary};
use de_render::DeRenderer;
use smithay::desktop::layer_map_for_output;

use crate::morph::Phase;
pub(crate) use crate::runtime::frame_io::minute_of_day;
use crate::runtime::frame_io::{summarize, write_ready_file};
use crate::state::{cma_free_kb, rss_kb, De};

pub use event_loop::event_loop;
use event_loop::run_de_shell_im_wake;
pub use render::DeElement;
use render::{render_with, FrameParts};

/// Runtime context held across frames.
pub struct Run {
    pub de: De,
    /// JSONL stats sink (per-second records).
    stats_file: Option<BufWriter<File>>,
    summary_path: Option<std::path::PathBuf>,
    scenario: String,
    pub(super) duration: Option<Duration>,
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
    trace_motion: bool,
    last_morph_frame: Option<(Phase, Instant)>,
    last_panel_frame: Option<Instant>,
    vblank_pending: bool,
}

impl Run {
    pub fn new(
        de: De,
        stats_file: Option<&Path>,
        summary_path: Option<std::path::PathBuf>,
        scenario: String,
        duration: Option<Duration>,
    ) -> Result<Self, String> {
        let stats_file = match stats_file {
            Some(p) => Some(BufWriter::new(
                OpenOptions::new()
                    .create(true)
                    .write(true)
                    .truncate(true)
                    .open(p)
                    .map_err(|e| format!("stats: {e}"))?,
            )),
            None => None,
        };
        Ok(Self {
            de,
            stats_file,
            summary_path,
            scenario,
            duration,
            frames: 0,
            window_frames: 0,
            cpu_samples: Vec::new(),
            all_cpu: Vec::new(),
            missed: 0,
            rss_peak: 0,
            cma_min: u64::MAX,
            last_vblank: None,
            last_second: Instant::now(),
            trace_motion: std::env::var_os("RABBIZ_DE_MOTION_TRACE").is_some(),
            last_morph_frame: None,
            last_panel_frame: None,
            vblank_pending: false,
        })
    }

    /// Render + queue one frame on the output (only when there is damage —
    /// `EmptyFrame` is a no-op and simply lets the screen hold still).
    /// Present one frame, retrying on a busy DRM device. Restarting right
    /// after another compositor exits races the kernel's deferred rmfb/
    /// restore work and the first flips can come back EBUSY — a wait is the
    /// difference between a clean re-exec and a false 75. Generous before
    /// the first flip (startup/restart edge), single attempt after that.
    pub fn present(&mut self) -> Result<(), String> {
        let mut last = String::new();
        let tries = if self.de.first_flip { 1 } else { 120 };
        for _ in 0..tries {
            match self.present_once() {
                Err(e) if e.contains("busy") => last = e,
                r => return r,
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        Err(last)
    }

    fn present_once(&mut self) -> Result<(), String> {
        let t0 = Instant::now();
        self.de.needs_present = false;
        // A pending launch that never produced a toplevel fails visibly —
        // the error screen, not a wedged morph.
        if let Some(id) = self.de.launch.check_timeout() {
            eprintln!("launch: {id} timed out waiting for a toplevel");
            self.de.launch_error = Some(format!("{id}: did not start"));
            self.de.morph = None;
            self.de.foreground = crate::state::Foreground::Shell;
            self.de.nav.go(de_ui::nav::Screen::Error);
            self.de.shell_dirty = true;
        }
        self.de.advance_morph(t0);
        self.de.advance_home(t0);
        self.de.advance_recents_fade(t0);
        self.de.advance_nav_bar(t0);
        if self.de.foreground == crate::state::Foreground::Shell
            && self.de.nav.current == de_ui::nav::Screen::Drawer
            && !self.de.navigation.drawer_dragging
            && !self.de.navigation.drawer_slide.settled()
        {
            let dt = self
                .de
                .navigation
                .drawer_last_tick
                .replace(t0)
                .map_or(0.0, |last| t0.duration_since(last).as_secs_f32());
            self.de
                .navigation
                .drawer_slide
                .advance(dt, &self.de.motion_clock);
            // Pixel-scale settle snap (0..1600 px travel) so a 0.001 epsilon
            // never burns frames in a sub-pixel tail.
            let h = self.de.output.height.max(1) as f32;
            if self.de.navigation.drawer_closing && self.de.navigation.drawer_slide.value >= h - 1.0
            {
                self.de.navigation.drawer_slide.snap();
            }
            if (self.de.navigation.drawer_slide.target - self.de.navigation.drawer_slide.value)
                .abs()
                < 1.0
                && self.de.navigation.drawer_slide.velocity.abs() < 2.0
            {
                self.de.navigation.drawer_slide.snap();
            }
            self.de.needs_present = true;
        } else if !self.de.navigation.drawer_dragging {
            self.de.navigation.drawer_last_tick = None;
        }
        if self.de.foreground == crate::state::Foreground::Shell
            && self.de.nav.current == de_ui::nav::Screen::Drawer
            && self.de.navigation.drawer_closing
            && !self.de.navigation.drawer_dragging
            && self.de.navigation.drawer_slide.settled()
        {
            self.de.navigation.drawer_closing = false;
            self.de.navigation.drawer_pull_y = 0.0;
            self.de.navigation.drawer_slide.value = 0.0;
            self.de.navigation.drawer_slide.target = 0.0;
            self.de.navigation.drawer_slide.velocity = 0.0;
            self.de.navigation.home_underlay_px = None;
            self.de.navigation.home_underlay_tex = None;
            self.de.show_shell(de_ui::nav::Screen::Home);
        }
        if self.de.foreground == crate::state::Foreground::Shell
            && self.de.nav.current == de_ui::nav::Screen::Recents
            && self.de.morph.is_none()
            && !self.de.navigation.recents_pager.dragging()
            && self.de.navigation.recents_pager.animating()
        {
            let dt = self
                .de
                .navigation
                .recents_last_tick
                .replace(t0)
                .map_or(0.0, |last| t0.duration_since(last).as_secs_f32());
            self.de
                .navigation
                .recents_pager
                .advance(dt, &self.de.motion_clock);
            if !self.de.recents_layers_active() {
                self.de.shell_dirty = true;
            }
            self.de.needs_present = true;
        } else {
            self.de.navigation.recents_last_tick = None;
        }
        self.de.advance_card_dismiss(t0);
        self.de.advance_panel(t0);
        self.de.power.advance(t0);
        self.de.advance_lock(t0);
        // Panel off: the whole raster/render path is skipped — timers and
        // wake sources keep running on the 500 ms poll.
        if self.de.sleeping() {
            return Ok(());
        }
        self.de.panels.use_gpu_blur = matches!(self.de.renderer, DeRenderer::Gl(_));
        self.de.panel_prepare();
        self.de.poll_recents_backdrop();
        let size = de_api::Size::new(self.de.output.width as i32, self.de.output.height as i32);
        self.de.lock_prepare(size);
        self.de.menu_prepare(size);
        if self.de.shot_fx.poll(t0) {
            self.de.needs_present = true;
        }
        self.de.shot_prepare(t0);
        if self.de.osd_fx.poll(t0) {
            self.de.needs_present = true;
        }
        self.de.osd_prepare(t0);
        let now_ms = self.de.uptime_ms(t0);
        self.de.lifecycle_tick(now_ms);
        if self.de.hun_fx.tick(now_ms).is_some() {
            self.de.needs_present = true;
        }
        self.de.hun_prepare(size);
        self.de.media_frame(t0);
        // Shell surface: re-raster on dirty or when the clock minute rolls.
        let minute = minute_of_day();
        // While the pager moves the home screen is drawn from layers (see
        // `home_ui.rs`): no whole-screen re-raster, no whole-screen upload.
        let layered = self.de.home_layers_active();
        if layered {
            self.de.home_layers_frame();
        }
        let recents_layered = self.de.recents_layers_active();
        if recents_layered {
            self.de.recents_layers_prepare(size, minute);
        }
        let recents_lift = if recents_layered {
            let windows = self.de.recent_windows();
            self.de.card_lift(&windows)
        } else {
            None
        };
        if !layered
            && !recents_layered
            && (matches!(self.de.foreground, crate::state::Foreground::Shell)
                || self.de.morph.is_some())
            && (self.de.shell_dirty || self.de.shell_minute != minute || self.de.shell_px.is_none())
        {
            self.de.shell_minute = minute;
            let t_raster = Instant::now();
            self.de.shell_px = Some(self.de.shell_pixels());
            let raster_ms = t_raster.elapsed().as_secs_f64() * 1000.0;
            if raster_ms > 25.0 && self.de.home_ui_paging() {
                eprintln!("home raster slow: {raster_ms:.1} ms while paging");
            }
            self.de.shell_dirty = false;
            self.de.shell_tex = None; // force texture re-upload
        }
        // Split `de` into field borrows so `renderer` can be passed
        // mutably alongside the rest of the scene state.
        let de = &mut self.de;
        let crate::state::De {
            renderer,
            compositor,
            space,
            shell_tex,
            shell_px,
            gesture_bar,
            theme,
            output,
            output_obj,
            clock,
            primary_composite,
            direct_scanout,
            foreground,
            morph,
            home_ui,
            panels,
            navigation,
            lock,
            shot_fx,
            osd_fx,
            hun_fx,
            media,
            power,
            pointer,
            pointer_active,
            cursor_px,
            cursor_tex,
            present_seq,
            ..
        } = de;
        let mut parts = FrameParts {
            compositor,
            space,
            shell_tex,
            shell_px,
            gesture_bar,
            theme,
            output,
            output_obj,
            clock,
            primary_composite,
            direct_scanout,
            foreground: *foreground,
            morph,
            home: home_ui,
            panel: panels,
            navigation,
            lock,
            shot_fx,
            osd_fx,
            hun_fx,
            media,
            power,
            pointer,
            pointer_active: *pointer_active,
            cursor_px,
            cursor_tex,
            present_seq,
            layered,
            recents_layered,
            recents_lift,
            now: t0,
        };
        let queued = match renderer {
            DeRenderer::Gl(r) => render_with(&mut parts, &mut **r)?,
            DeRenderer::Soft(r) => render_with(&mut parts, r)?,
        };
        let cpu = t0.elapsed().as_secs_f64() * 1000.0;
        self.cpu_samples.push(cpu);
        self.all_cpu.push(cpu);
        // `copy_with_damage` completes on the frame that carried damage.
        if queued {
            if self.de.compositor.is_some() {
                self.vblank_pending = true;
                self.de.screencopy_flush_pending();
            } else {
                self.on_virtual_present();
            }
        }
        if self.trace_motion && queued && self.de.panels.pull.animating() {
            let gap = self
                .last_panel_frame
                .replace(t0)
                .map_or(0.0, |last| t0.duration_since(last).as_secs_f64() * 1000.0);
            eprintln!(
                "panel-frame cpu_ms={cpu:.3} queue_gap_ms={gap:.3} dragging={}",
                self.de.panels.pull.dragging()
            );
        } else if !self.de.panels.pull.animating() {
            self.last_panel_frame = None;
        }
        if self.trace_motion && queued {
            if let Some(morph) = self.de.morph.as_ref() {
                let last = self.last_morph_frame.replace((morph.phase, t0));
                let gap = last
                    .filter(|(phase, _)| *phase == morph.phase)
                    .map_or(0.0, |(_, time)| {
                        t0.duration_since(time).as_secs_f64() * 1000.0
                    });
                eprintln!(
                    "morph-frame phase={:?} cpu_ms={cpu:.3} queue_gap_ms={gap:.3}",
                    morph.phase
                );
            } else if let Some((phase, _)) = self.last_morph_frame.take() {
                eprintln!("morph-handoff phase={phase:?}");
            }
        }
        Ok(())
    }

    pub(super) fn motion_wake_at(&self) -> Option<Instant> {
        let interval = Duration::from_secs_f64(0.95 / self.de.output.refresh_hz.max(1.0));
        let now = Instant::now();
        let search_blink = (self.de.foreground == crate::state::Foreground::Shell
            && (self.de.nav.current == de_ui::nav::Screen::Search
                || (self.de.nav.current == de_ui::nav::Screen::Drawer
                    && !self.de.navigation.search_query.is_empty())))
        .then_some(now + Duration::from_millis(530));
        let recents_wake = (self.de.foreground == crate::state::Foreground::Shell
            && self.de.nav.current == de_ui::nav::Screen::Recents
            && self.de.morph.is_none()
            && !self.de.navigation.recents_pager.dragging()
            && self.de.navigation.recents_pager.animating())
        .then_some(now + interval);
        let drawer_wake = (self.de.foreground == crate::state::Foreground::Shell
            && self.de.nav.current == de_ui::nav::Screen::Drawer
            && !self.de.navigation.drawer_dragging
            && !self.de.navigation.drawer_slide.settled())
        .then_some(now + interval);
        let nav_bar_wake = (!self.de.navigation.nav_bar_active.settled()).then_some(now + interval);
        let shell_im_wake = run_de_shell_im_wake(self).then_some(now + Duration::from_millis(16));
        [
            shell_im_wake,
            nav_bar_wake,
            self.de.media.wake_at(interval),
            self.de.launch.deadline(),
            self.de
                .morph
                .as_ref()
                .and_then(|morph| morph.next_frame_at(interval)),
            self.de.home_wake_at(interval),
            self.de.panel_wake_at(interval),
            self.de.lock_wake_at(interval),
            self.de.power.wake_at(now),
            self.de.shot_fx.wake_at(now),
            self.de.osd_fx.wake_at(now),
            search_blink,
            recents_wake,
            self.de
                .navigation
                .recents_fade
                .as_ref()
                .map(|_| now + interval),
            self.de
                .navigation
                .card_dismiss
                .as_ref()
                .filter(|d| !d.dragging)
                .map(|_| now + interval),
            drawer_wake,
        ]
        .into_iter()
        .flatten()
        .min()
    }

    /// Called after client/input dispatch — present only when a commit or
    /// shell change marked the scene dirty.
    pub fn pump(&mut self) {
        // Prune dead elements and update output enter/leave — anvil runs
        // this once per event-loop turn.
        self.de.space.refresh();
        self.de.popups.cleanup();
        layer_map_for_output(&self.de.output_obj).cleanup();
        self.de.sync_exclusive_layer_focus();
        // Coalesce rapid input/client events into one present per vblank: if a
        // DRM frame is already in flight, `on_vblank` will present the latest
        // state when the hardware is ready.
        if self.de.needs_present && !self.vblank_pending {
            self.de.needs_present = false;
            if !self.de.sleeping() {
                let _ = self.present().map_err(|e| eprintln!("present: {e}"));
            }
        }
        // Input handlers and commits queue server->client events on every
        // source (libinput taps, frame callbacks, configures), not just the
        // display source — flush here so they reach clients immediately.
        if let Err(e) = self.de.display_handle.flush_clients() {
            eprintln!("flush_clients: {e}");
        }
    }

    /// Called on each vblank: release the pending frame, account timing,
    /// emit stats, request the next frame.
    pub fn on_vblank(&mut self) {
        self.vblank_pending = false;
        if let Some(comp) = self.de.compositor.as_mut() {
            if let Err(e) = comp.frame_submitted() {
                eprintln!("frame_submitted: {e:?}");
            }
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
        // window.send_frame() inside present() queued frame.done events —
        // flush so clients see them this vblank, not on their next request.
        if let Err(e) = self.de.display_handle.flush_clients() {
            eprintln!("flush_clients: {e}");
        }
    }

    /// Virtual-backend present accounting (no hardware vblank notifier).
    fn on_virtual_present(&mut self) {
        let now = Instant::now();
        if !self.de.first_flip {
            self.de.first_flip = true;
            write_ready_file();
        }
        self.frames += 1;
        self.window_frames += 1;
        self.rss_peak = self.rss_peak.max(rss_kb());
        self.cma_min = self.cma_min.min(cma_free_kb());
        self.de.screencopy_flush_pending();
        self.de.broadcast_emu_frame();
        if now.duration_since(self.last_second) >= Duration::from_secs(1) {
            self.flush_second(now);
        }
    }

    fn flush_second(&mut self, now: Instant) {
        if let Some(f) = &mut self.stats_file {
            let rec = FrameStats {
                t_ms: self.de.started.elapsed().as_millis() as u64,
                frames: self.window_frames,
                cpu_ms: summarize(&self.cpu_samples),
                gpu_ms: None,
                damage_px: u64::from(self.de.output.width * self.de.output.height)
                    * self.window_frames,
                layers: 1,
                textures_mb: 0.0,
                cma_free_kb: cma_free_kb(),
                rss_kb: rss_kb(),
                direct_scanout: self.de.direct_scanout,
                primary_composite: self.de.primary_composite,
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
