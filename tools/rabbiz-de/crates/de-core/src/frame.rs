//! Frame pump + stats (M0-T4/M1-T4): present the scene every vblank while
//! damage exists, count direct-scanout vs primary-composite, record
//! per-second JSONL stats, emit the final summary.

use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use de_api::{FrameStats, StatsSummary, TimingSummary};
use de_protocols::layer_shell::{origin, ABOVE_WINDOWS, BELOW_WINDOWS};
use de_render::DeRenderer;
use smithay::backend::drm::compositor::{FrameFlags, PrimaryPlaneElement};
use smithay::backend::renderer::element::render_elements;
use smithay::backend::renderer::element::solid::SolidColorRenderElement;
use smithay::backend::renderer::element::surface::{
    render_elements_from_surface_tree, WaylandSurfaceRenderElement,
};
use smithay::backend::renderer::element::texture::TextureRenderElement;
use smithay::backend::renderer::element::AsRenderElements;
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::{ImportAll, ImportMem, Renderer, Texture};
use smithay::desktop::utils::send_frames_surface_tree;
use smithay::desktop::{layer_map_for_output, LayerMap};
use smithay::reexports::calloop::generic::Generic;
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::reexports::calloop::{EventLoop, Interest, Mode, PostAction};
use smithay::reexports::wayland_server::{Display, Resource};
use smithay::utils::{Physical, Point, Scale};
use smithay::wayland::seat::WaylandFocus;
use smithay::wayland::shell::wlr_layer::Layer;
use smithay::wayland::socket::ListeningSocketSource;

use crate::morph::{Phase, WindowMorph};
use crate::state::{cma_free_kb, rss_kb, De, DeClientState};

render_elements! {
    /// Elements submitted to `DrmCompositor::render_frame` each frame.
    pub DeElement<R> where R: ImportAll + ImportMem, R::TextureId: Texture + Clone + 'static;
    Texture = TextureRenderElement<R::TextureId>,
    Surface = WaylandSurfaceRenderElement<R>,
    Solid = SolidColorRenderElement,
}

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
    trace_motion: bool,
    last_morph_frame: Option<(Phase, Instant)>,
    last_panel_frame: Option<Instant>,
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
        self.de.advance_panel(t0);
        self.de.power.advance(t0);
        self.de.advance_lock(t0);
        // Panel off: the whole raster/render path is skipped — timers and
        // wake sources keep running on the 500 ms poll.
        if self.de.sleeping() {
            return Ok(());
        }
        self.de.panel_prepare();
        let size = de_api::Size::new(self.de.output.width as i32, self.de.output.height as i32);
        self.de.lock_prepare(size);
        self.de.menu_prepare(size);
        if self.de.shot_fx.poll(t0) {
            self.de.needs_present = true;
        }
        self.de.shot_prepare(t0);
        // Shell surface: re-raster on dirty or when the clock minute rolls.
        let minute = minute_of_day();
        // While the pager moves the home screen is drawn from layers (see
        // `home_ui.rs`): no whole-screen re-raster, no whole-screen upload.
        let layered = self.de.home_layers_active();
        if layered {
            self.de.home_layers_frame();
        }
        if !layered
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
            power,
            ..
        } = de;
        let mut parts = FrameParts {
            compositor,
            space,
            shell_tex,
            shell_px,
            gesture_bar,
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
            power,
            layered,
            now: t0,
        };
        let queued = match renderer {
            DeRenderer::Gl(r) => render_with(&mut parts, &mut **r)?,
            DeRenderer::Soft(r) => render_with(&mut parts, r)?,
        };
        // `copy_with_damage` completes on the frame that carried damage.
        if queued {
            self.de.screencopy_flush_pending();
        }
        let cpu = t0.elapsed().as_secs_f64() * 1000.0;
        self.cpu_samples.push(cpu);
        self.all_cpu.push(cpu);
        if self.trace_motion && queued && self.de.panels.pull.animating() {
            let gap = self.last_panel_frame.replace(t0)
                .map_or(0.0, |last| t0.duration_since(last).as_secs_f64() * 1000.0);
            eprintln!("panel-frame cpu_ms={cpu:.3} queue_gap_ms={gap:.3} dragging={}",
                self.de.panels.pull.dragging());
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
}

/// Field-split view of `De` for [`render_with`], so the renderer can be
/// borrowed mutably at the same time as the scene state.
struct FrameParts<'a> {
    compositor: &'a mut de_backend::probe::DeCompositor,
    space: &'a mut smithay::desktop::Space<smithay::desktop::Window>,
    shell_tex: &'a mut Option<Box<dyn std::any::Any>>,
    shell_px: &'a mut Option<de_api::Pixels>,
    gesture_bar: &'a mut Option<Box<dyn std::any::Any>>,
    output: &'a de_backend::drm::OutputSelection,
    output_obj: &'a smithay::output::Output,
    clock: &'a smithay::utils::Clock<smithay::utils::Monotonic>,
    primary_composite: &'a mut u64,
    direct_scanout: &'a mut u64,
    foreground: crate::state::Foreground,
    morph: &'a mut Option<WindowMorph>,
    home: &'a mut crate::home_ui::HomeUi,
    panel: &'a mut crate::panel_ui::PanelUi,
    navigation: &'a mut crate::navigation::Navigation,
    lock: &'a mut crate::lock_ui::LockUi,
    shot_fx: &'a mut crate::shot_ui::ShotFx,
    power: &'a mut crate::power::Power,
    /// Draw the home screen from `home` layers instead of `shell_tex`.
    layered: bool,
    now: Instant,
}

fn render_with<R>(de: &mut FrameParts<'_>, renderer: &mut R) -> Result<bool, String>
where
    R: Renderer
        + ImportAll
        + ImportMem
        + smithay::backend::renderer::Bind<smithay::backend::allocator::dmabuf::Dmabuf>,
    R::TextureId: smithay::backend::renderer::Texture + Clone + 'static,
{
    // Upload the latest shell frame as a texture (re-rastered upstream in
    // `present_once` on dirty/minute change — the cached texture otherwise
    // produces no damage and the frame is simply EmptyFrame).
    if de.shell_tex.is_none() {
        if let Some(px) = de.shell_px.as_ref() {
            *de.shell_tex = upload_any(renderer, px);
        }
    }
    if de.gesture_bar.is_none() {
        let px = de_ui::nav_bar::gesture_bar(de_api::Size::new(
            de.output.width as i32,
            de.output.height as i32,
        ));
        *de.gesture_bar = upload_any(renderer, &px);
    }

    let mut elements: Vec<DeElement<R>> = Vec::new();
    let map = layer_map_for_output(de.output_obj);
    push_layer_elements(&mut elements, renderer, &map, ABOVE_WINDOWS);
    let transitioning = de.morph.is_some();
    let show_app = !transitioning
        && de.foreground == crate::state::Foreground::App
        && de.space.elements().next().is_some();
    if let Some(morph) = de.morph.as_mut() {
        let output = de_api::Rect::new(0, 0, de.output.width as i32, de.output.height as i32);
        push_morph_elements(&mut elements, renderer, morph, output, de.now);
    }
    if show_app {
        push_window_elements(&mut elements, renderer, de.space);
    } else if de.layered && de.home.layers_key().is_some() {
        sync_home_textures(renderer, de.home);
        push_home_layers::<R>(
            &mut elements,
            de.home,
            de_api::Size::new(de.output.width as i32, de.output.height as i32),
        );
    } else if let Some(tex) = texture_buffer::<R>(de.shell_tex.as_ref()) {
        elements.push(DeElement::from(tex_el::<R>(
            tex,
            Point::<f64, Physical>::from((0.0, 0.0)),
        )));
    }
    // The lifted icon rides on top of the home frame as its own texture.
    push_drag_overlay::<R>(&mut elements, renderer, de.home, de.output);
    push_panel::<R>(&mut elements, renderer, de.panel);
    // Overlays, topmost last: shot feedback → lock → locked menu → fade.
    push_shot::<R>(&mut elements, renderer, de.shot_fx, de.output, de.now);
    push_lock::<R>(&mut elements, renderer, de.lock, de.output, de.now);
    push_menu_over::<R>(&mut elements, renderer, de.power, de.output);
    push_fade::<R>(&mut elements, de.power, de.output, de.now);
    if let Some((px, at)) = de.navigation.back.as_ref() {
        if de.navigation.back_texture.is_none() {
            de.navigation.back_texture = upload_any(renderer, px);
        }
        if let Some(tex) = texture_buffer::<R>(de.navigation.back_texture.as_ref()) {
            elements.insert(0, DeElement::from(tex_el::<R>(tex,
                Point::<f64, Physical>::from((f64::from(at.x), f64::from(at.y))))));
        }
    }
    // Gesture bar on top of everything.
    let bar_y = f64::from(de.output.height) - f64::from(de_ui::chrome::NAV_H);
    if let Some(tex) = texture_buffer::<R>(de.gesture_bar.as_ref()) {
        let bar = DeElement::from(tex_el::<R>(tex, Point::<f64, Physical>::from((0.0, bar_y))));
        if transitioning {
            elements.insert(0, bar);
        } else {
            elements.push(bar);
        }
    }
    push_layer_elements(&mut elements, renderer, &map, BELOW_WINDOWS);

    let res = de
        .compositor
        .render_frame(
            renderer,
            &elements,
            [0.086, 0.063, 0.102, 1.0],
            if transitioning {
                FrameFlags::empty()
            } else {
                FrameFlags::DEFAULT
            },
        )
        .map_err(|e| format!("render: {e:?}"))?;

    if let Err(e) = de.compositor.queue_frame(()) {
        if !format!("{e:?}").contains("EmptyFrame") {
            return Err(format!("queue: {e:?}"));
        }
        return Ok(false);
    }

    // M1-T4 counters from the observed result, not a guess.
    match res.primary_element {
        PrimaryPlaneElement::Swapchain(_) => *de.primary_composite += 1,
        PrimaryPlaneElement::Element(_) => *de.direct_scanout += 1,
    }

    // Frame callbacks for the freshly-committed surfaces.
    let time = de.clock.now();
    let out = de.output_obj.clone();
    for layer in map.layers() {
        layer.send_frame(&out, time, Some(Duration::from_millis(16)), |_, _| {
            Some(out.clone())
        });
    }
    for window in de.space.elements() {
        window.send_frame(&out, time, Some(Duration::from_millis(16)), |_, _| {
            Some(out.clone())
        });
        for (popup, _) in crate::popups::popups_of(window) {
            send_frames_surface_tree(
                popup.wl_surface(),
                &out,
                time,
                Some(Duration::from_millis(16)),
                |_, _| Some(out.clone()),
            );
        }
    }
    Ok(true)
}

/// Elements of every mapped app window, each preceded by its popups (earlier elements are in front).
fn push_window_elements<R: Renderer + ImportAll + ImportMem>(
    elements: &mut Vec<DeElement<R>>,
    renderer: &mut R,
    space: &smithay::desktop::Space<smithay::desktop::Window>,
) where
    R::TextureId: Texture + Clone + 'static,
{
    for window in space.elements().rev() {
        let loc = space.element_location(window).unwrap_or_default().to_physical(1);
        for (popup, offset) in crate::popups::popups_of(window) {
            elements.extend(render_elements_from_surface_tree::<R, DeElement<R>>(
                renderer,
                popup.wl_surface(),
                loc + offset.to_physical(1),
                Scale::from(1.0),
                1.0,
                Kind::Unspecified,
            ));
        }
        elements.extend(AsRenderElements::<R>::render_elements::<DeElement<R>>(
            window,
            renderer,
            loc,
            Scale::from(1.0),
            1.0,
        ));
    }
}

fn push_layer_elements<R: Renderer + ImportAll + ImportMem>(
    elements: &mut Vec<DeElement<R>>,
    renderer: &mut R,
    map: &LayerMap,
    levels: [Layer; 2],
) where
    R::TextureId: Texture + Clone + 'static,
{
    for level in levels {
        for layer in map.layers_on(level).rev() {
            if let Some(loc) = origin(map, layer) {
                elements.extend(layer.render_elements::<DeElement<R>>(
                    renderer,
                    loc.to_physical(1),
                    Scale::from(1.0),
                    1.0,
                ));
            }
        }
    }
}

fn push_morph_elements<R: Renderer + ImportAll + ImportMem>(
    elements: &mut Vec<DeElement<R>>,
    renderer: &mut R,
    morph: &mut WindowMorph,
    output: de_api::Rect,
    now: Instant,
) where
    R::TextureId: Texture + Clone + 'static,
{
    let pose = morph.pose(output, now);
    // Smithay consumes front-to-back elements. The launcher texture sits
    // behind the solid backing, the scaled app and the cached icon.
    if pose.icon_alpha > 0.0 {
        if morph.icon_texture.is_none() {
            morph.icon_texture = upload_any(renderer, &morph.origin.icon);
        }
        if let Some(icon) = texture_buffer::<R>(morph.icon_texture.as_ref()) {
            elements.push(DeElement::from(TextureRenderElement::from_texture_buffer(
                (f64::from(pose.icon.x), f64::from(pose.icon.y)),
                icon,
                Some(pose.icon_alpha),
                None,
                None,
                Kind::Unspecified,
            )));
        }
    }
    let loc = Point::<i32, Physical>::from((pose.card.x, pose.card.y));
    if let Some(window) = morph.window.as_ref().filter(|_| pose.app_alpha > 0.0) {
        let scale = Scale::from((
            f64::from(pose.card.w) / f64::from(output.w),
            f64::from(pose.card.h) / f64::from(output.h),
        ));
        elements.extend(AsRenderElements::<R>::render_elements::<DeElement<R>>(
            window,
            renderer,
            loc,
            scale,
            pose.app_alpha,
        ));
    }
    if pose.card_alpha > 0.0 {
        morph.backing.resize((pose.card.w, pose.card.h));
        elements.push(DeElement::from(SolidColorRenderElement::from_buffer(
            &morph.backing,
            loc,
            1.0,
            pose.card_alpha,
            Kind::Unspecified,
        )));
    }
    if let Some(px) = morph.backdrop.as_ref() {
        if morph.backdrop_texture.is_none() {
            morph.backdrop_texture = upload_any(renderer, px);
        }
        if let Some(tex) = texture_buffer::<R>(morph.backdrop_texture.as_ref()) {
            elements.push(DeElement::from(TextureRenderElement::from_texture_buffer(
                (0.0, 0.0), tex, Some(morph.backdrop_alpha()), None, None, Kind::Unspecified,
            )));
        }
    }

}

impl Run {
    fn motion_wake_at(&self) -> Option<Instant> {
        let interval = Duration::from_secs_f64(1.5 / self.de.output.refresh_hz.max(1.0));
        let now = Instant::now();
        [
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
        if self.de.needs_present {
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
        // window.send_frame() inside present() queued frame.done events —
        // flush so clients see them this vblank, not on their next request.
        if let Err(e) = self.de.display_handle.flush_clients() {
            eprintln!("flush_clients: {e}");
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

/// Upload `px` into `renderer` as a `TextureBuffer`, boxed for storage.
fn upload_any<R: ImportMem>(r: &mut R, px: &de_api::Pixels) -> Option<Box<dyn std::any::Any>>
where
    R::TextureId: 'static,
{
    use smithay::backend::renderer::element::texture::TextureBuffer;
    TextureBuffer::from_memory(
        r,
        &px.data,
        smithay::backend::allocator::Fourcc::Abgr8888,
        smithay::utils::Size::from((px.size.w, px.size.h)),
        false,
        1,
        smithay::utils::Transform::Normal,
        None,
    )
    .ok()
    .map(|b| Box::new(b) as Box<dyn std::any::Any>)
}

/// The notification/quick panel: one texture slid by the pull fraction, in
/// front of everything (apps included). Fades in as it comes down.
fn push_panel<R>(
    elements: &mut Vec<DeElement<R>>,
    renderer: &mut R,
    panel: &mut crate::panel_ui::PanelUi,
) where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: smithay::backend::renderer::Texture + Clone + 'static,
{
    if !panel.pull.shown() {
        panel.tex = None;
        panel.tex_generation = u64::MAX;
        panel.veil_tex = None;
        panel.veil_tex_generation = u64::MAX;
        return;
    }
    if panel.tex.is_none() || panel.tex_generation != panel.generation {
        let Some(px) = panel.pixels.as_ref() else { return };
        panel.tex = upload_any(renderer, px);
        panel.tex_generation = panel.generation;
    }
    if panel.veil_tex.is_none() || panel.veil_tex_generation != panel.generation {
        if let Some(veil) = panel.veil.as_ref() {
            panel.veil_tex = upload_any(renderer, veil);
            panel.veil_tex_generation = panel.generation;
        }
    }
    let (Some(tex), Some(px)) = (texture_buffer::<R>(panel.tex.as_ref()), panel.pixels.as_ref())
    else {
        return;
    };
    let f = panel.pull.frac().clamp(0.0, 1.0);
    // The frosted sheet surface beneath the sliding controls: the blurred
    // scene under the scrim, revealed from the top edge in step with the
    // pull so every covered row stays aligned with the scene beneath it.
    if let (Some(veil), Some(vpx)) = (texture_buffer::<R>(panel.veil_tex.as_ref()), panel.veil.as_ref())
    {
        let covered = f64::from((f * vpx.size.h as f32).round());
        if covered > 0.0 {
            elements.insert(
                0,
                DeElement::from(TextureRenderElement::from_texture_buffer(
                    Point::<f64, Physical>::from((0.0, 0.0)),
                    veil,
                    None,
                    Some(smithay::utils::Rectangle::new(
                        Point::<f64, smithay::utils::Logical>::from((0.0, 0.0)),
                        smithay::utils::Size::<f64, smithay::utils::Logical>::from((
                            f64::from(vpx.size.w),
                            covered,
                        )),
                    )),
                    None,
                    Kind::Unspecified,
                )),
            );
        }
    }
    let y = -(1.0 - f) * px.size.h as f32;
    let alpha = (f * 1.6).min(1.0);
    elements.insert(
        0,
        DeElement::from(TextureRenderElement::from_texture_buffer(
            Point::<f64, Physical>::from((0.0, f64::from(y.round()))),
            tex,
            Some(alpha),
            None,
            None,
            Kind::Unspecified,
        )),
    );
}

/// Lock overlay texture: slides up with the unlock drag and fades out;
/// fp-fail shakes it horizontally. Inserted at the front (topmost).
fn push_lock<R>(
    elements: &mut Vec<DeElement<R>>,
    renderer: &mut R,
    lock: &mut crate::lock_ui::LockUi,
    output: &de_backend::drm::OutputSelection,
    now: Instant,
) where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: smithay::backend::renderer::Texture + Clone + 'static,
{
    if !lock.shown() {
        lock.tex = None;
        lock.tex_generation = u64::MAX;
        return;
    }
    if lock.tex.is_none() || lock.tex_generation != lock.generation {
        let Some(px) = lock.px.as_ref() else { return };
        lock.tex = upload_any(renderer, px);
        lock.tex_generation = lock.generation;
    }
    let Some(tex) = texture_buffer::<R>(lock.tex.as_ref()) else {
        return;
    };
    let _ = output;
    let x = lock.shake_x(now);
    let y = -f64::from(lock.slide_px());
    let alpha = 1.0 - lock.progress() * 0.55;
    elements.insert(
        0,
        DeElement::from(TextureRenderElement::from_texture_buffer(
            Point::<f64, Physical>::from((x, y.round())),
            tex,
            Some(alpha),
            None,
            None,
            Kind::Unspecified,
        )),
    );
}

/// Power menu floating above the lock (long-press combo while locked).
fn push_menu_over<R>(
    elements: &mut Vec<DeElement<R>>,
    renderer: &mut R,
    power: &mut crate::power::Power,
    _output: &de_backend::drm::OutputSelection,
) where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: smithay::backend::renderer::Texture + Clone + 'static,
{
    if !power.menu_over_lock {
        power.menu_tex = None;
        return;
    }
    if power.menu_tex.is_none() {
        let Some(px) = power.menu_px.as_ref() else { return };
        power.menu_tex = upload_any(renderer, px);
    }
    let Some(tex) = texture_buffer::<R>(power.menu_tex.as_ref()) else {
        return;
    };
    elements.insert(
        0,
        DeElement::from(TextureRenderElement::from_texture_buffer(
            Point::<f64, Physical>::from((0.0, 0.0)),
            tex,
            Some(1.0),
            None,
            None,
            Kind::Unspecified,
        )),
    );
}

/// Screenshot feedback: toast card bottom-right + the white flash (drawn
/// after the capture, so the flash never photographs itself).
fn push_shot<R>(
    elements: &mut Vec<DeElement<R>>,
    renderer: &mut R,
    shot: &mut crate::shot_ui::ShotFx,
    output: &de_backend::drm::OutputSelection,
    now: Instant,
) where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: smithay::backend::renderer::Texture + Clone + 'static,
{
    if !shot.toast_active(now) {
        shot.tex = None;
        shot.tex_generation = u64::MAX;
    } else {
        if shot.tex.is_none() || shot.tex_generation != shot.generation {
            if let Some(px) = shot.px.as_ref() {
                shot.tex = upload_any(renderer, px);
                shot.tex_generation = shot.generation;
            }
        }
        if let Some(tex) = texture_buffer::<R>(shot.tex.as_ref()) {
            let r = crate::shot_ui::ShotFx::toast_rect(de_api::Size::new(
                output.width as i32,
                output.height as i32,
            ));
            elements.insert(
                0,
                DeElement::from(TextureRenderElement::from_texture_buffer(
                    Point::<f64, Physical>::from((f64::from(r.x), f64::from(r.y))),
                    tex,
                    Some(1.0),
                    None,
                    None,
                    Kind::Unspecified,
                )),
            );
        }
    }
    if shot.flash_active(now) {
        let buf = smithay::backend::renderer::element::solid::SolidColorBuffer::new(
            (output.width as i32, output.height as i32),
            [1.0, 0.972, 0.941, 1.0],
        );
        elements.insert(
            0,
            DeElement::from(SolidColorRenderElement::from_buffer(
                &buf,
                Point::<i32, Physical>::from((0, 0)),
                1.0,
                1.0,
                Kind::Unspecified,
            )),
        );
    }
}

/// Black curtain for the sleep/wake fades (topmost of everything).
fn push_fade<R>(
    elements: &mut Vec<DeElement<R>>,
    power: &crate::power::Power,
    output: &de_backend::drm::OutputSelection,
    now: Instant,
) where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: smithay::backend::renderer::Texture + Clone + 'static,
{
    let alpha = power.fade_alpha(now);
    if alpha <= 0.0 {
        return;
    }
    let buf = smithay::backend::renderer::element::solid::SolidColorBuffer::new(
        (output.width as i32, output.height as i32),
        [0.0, 0.0, 0.0, 1.0],
    );
    elements.insert(
        0,
        DeElement::from(SolidColorRenderElement::from_buffer(
            &buf,
            Point::<i32, Physical>::from((0, 0)),
            1.0,
            alpha,
            Kind::Unspecified,
        )),
    );
}

/// Lifted icon + landing highlight, in front of everything else.
fn push_drag_overlay<R>(
    elements: &mut Vec<DeElement<R>>,
    renderer: &mut R,
    home: &mut crate::home_ui::HomeUi,
    output: &de_backend::drm::OutputSelection,
) where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: smithay::backend::renderer::Texture + Clone + 'static,
{
    if home.drag.is_none() {
        return;
    }
    let size = de_api::Size::new(output.width as i32, output.height as i32);
    sync_drag_textures(renderer, home);
    let Some((at, hl)) = home.drag_overlay(size) else {
        return;
    };
    if let (Some(rect), Some(tex)) = (hl, texture_buffer::<R>(home.hl_tex.as_ref())) {
        elements.insert(0, DeElement::from(tex_el_sized::<R>(tex, rect)));
    }
    if let Some(tex) = texture_buffer::<R>(home.drag_tex.as_ref()) {
        elements.insert(
            0,
            DeElement::from(tex_el::<R>(
                tex,
                Point::<f64, Physical>::from((f64::from(at.x), f64::from(at.y))),
            )),
        );
    }
}

/// Upload the lifted icon (and the shared highlight) once per drag.
fn sync_drag_textures<R: ImportMem>(r: &mut R, h: &mut crate::home_ui::HomeUi)
where
    R::TextureId: 'static,
{
    if h.drag_tex.is_none() {
        if let Some(d) = h.drag.as_ref() {
            h.drag_tex = upload_any(r, &d.px);
        }
    }
    if h.hl_tex.is_none() {
        let t = de_design::tokens::DEFAULT;
        let mut c = de_paint::Canvas::new(de_api::Size::new(112, 112));
        let mut col = t.accent;
        col[3] = 70;
        c.fill_rounded_rect(
            de_api::Rect::new(0, 0, 112, 112),
            de_design::tokens::ICON_TILE_RADIUS,
            col,
        );
        h.hl_tex = upload_any(r, &c.into_pixels());
    }
}

/// Upload the home layers that have no texture yet (drops stale ones when
/// the layer set was rebuilt).
fn sync_home_textures<R: ImportMem>(r: &mut R, h: &mut crate::home_ui::HomeUi)
where
    R::TextureId: 'static,
{
    let Some(key) = h.layers_key() else { return };
    if h.tex_key != key || h.base_tex.is_none() {
        h.tex_key = key;
        h.base_tex = None;
        h.page_tex.clear();
        h.dots_tex.clear();
    }
    let Some(layers) = h.layers.as_ref() else { return };
    if h.base_tex.is_none() {
        h.base_tex = upload_any(r, &layers.base);
    }
    for (p, px) in &layers.pages {
        if !h.page_tex.contains_key(p) {
            if let Some(t) = upload_any(r, px) {
                h.page_tex.insert(*p, t);
            }
        }
    }
    for (p, px) in &layers.dots {
        if !h.dots_tex.contains_key(p) {
            if let Some(t) = upload_any(r, px) {
                h.dots_tex.insert(*p, t);
            }
        }
    }
}

/// Push the home layers front to back: dots, page grids at their slid
/// positions, then the base. Positions are whole pixels.
fn push_home_layers<R: ImportAll>(
    elements: &mut Vec<DeElement<R>>,
    h: &crate::home_ui::HomeUi,
    size: de_api::Size,
) where
    R::TextureId: smithay::backend::renderer::Texture + Clone + 'static,
{
    let width = size.w;
    let pos = h.pos();
    let near = pos.round().max(0.0) as usize;
    if let Some(tex) = texture_buffer::<R>(h.dots_tex.get(&near)) {
        let y = de_ui::home::dots_y(size);
        elements.push(DeElement::from(tex_el::<R>(
            tex,
            Point::<f64, Physical>::from((0.0, f64::from(y))),
        )));
    }
    let mut pages: Vec<usize> = h.page_tex.keys().copied().collect();
    pages.sort_unstable();
    for p in pages {
        let x = ((p as f32 - pos) * width as f32).round();
        if x <= -(width as f32) || x >= width as f32 {
            continue;
        }
        if let Some(tex) = texture_buffer::<R>(h.page_tex.get(&p)) {
            elements.push(DeElement::from(tex_el::<R>(
                tex,
                Point::<f64, Physical>::from((f64::from(x), f64::from(de_ui::home::grid_band_y()))),
            )));
        }
    }
    if let Some(tex) = texture_buffer::<R>(h.base_tex.as_ref()) {
        elements.push(DeElement::from(tex_el::<R>(
            tex,
            Point::<f64, Physical>::from((0.0, 0.0)),
        )));
    }
}

fn texture_buffer<R: ImportAll>(
    tex: Option<&Box<dyn std::any::Any>>,
) -> Option<&smithay::backend::renderer::element::texture::TextureBuffer<R::TextureId>>
where
    R::TextureId: 'static,
{
    // The stored texture was uploaded by the active renderer, so the
    // TextureId type always matches R.
    tex?.downcast_ref()
}

fn tex_el<R: ImportAll>(
    buf: &smithay::backend::renderer::element::texture::TextureBuffer<R::TextureId>,
    loc: Point<f64, Physical>,
) -> TextureRenderElement<R::TextureId>
where
    R::TextureId: smithay::backend::renderer::Texture + Clone + 'static,
{
    TextureRenderElement::from_texture_buffer(loc, buf, None, None, None, Kind::Unspecified)
}

/// A texture scaled to fill `rect` (the landing highlight spans icons of
/// different sizes).
fn tex_el_sized<R: ImportAll>(
    buf: &smithay::backend::renderer::element::texture::TextureBuffer<R::TextureId>,
    rect: de_api::Rect,
) -> TextureRenderElement<R::TextureId>
where
    R::TextureId: smithay::backend::renderer::Texture + Clone + 'static,
{
    TextureRenderElement::from_texture_buffer(
        Point::<f64, Physical>::from((f64::from(rect.x), f64::from(rect.y))),
        buf,
        None,
        None,
        Some(smithay::utils::Size::<i32, smithay::utils::Logical>::from((rect.w, rect.h))),
        Kind::Unspecified,
    )
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
            .map_or(0, |d| d.as_millis() as u64);
        let _ = std::fs::write(format!("{dir}/rabbiz-de.ready"), format!("{pid} {ms}"));
    }
}

/// Touch `$XDG_RUNTIME_DIR/rabbiz-de.shot` → dump the composited frame
/// (shell/app + layer surfaces, mirrored from `composite_output`) as
/// `rabbiz-de-shot.ppm`. dmabuf/EGL client content is not readable on CPU
/// and shows as the clear colour.
fn dump_frame_if_triggered(de: &crate::state::De) {
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

fn minute_of_day() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() / 60)
}

/// Run the session event loop until `run.duration` elapses (session mode:
/// no duration → run until killed).
#[allow(unsafe_code)] // wayland-server's dispatch_clients is an unsafe FFI call
#[allow(clippy::too_many_lines)] // source registration order is the documentation
pub fn event_loop(mut run: Run, display: Display<De>, print_stats: bool) -> Result<u8, String> {
    let mut event_loop: EventLoop<Run> =
        EventLoop::try_new().map_err(|e| format!("event loop: {e}"))?;
    let handle = event_loop.handle();

    // Wayland client socket: fixed name `wayland-0` (§7.20).
    let socket = ListeningSocketSource::with_name("wayland-0")
        .map_err(|e| format!("wayland socket: {e:?}"))?;
    let socket_name = socket.socket_name().to_string_lossy().into_owned();
    run.de.socket_name = Some(socket_name.clone());
    handle
        .insert_source(socket, |stream, (), run: &mut Run| {
            let state = Arc::new(DeClientState::default());
            match run.de.display_handle.insert_client(stream, state.clone()) {
                Ok(client) => crate::privileged::identify(&run.de.display_handle, &client, &state),
                Err(err) => eprintln!("insert_client: {err}"),
            }
        })
        .map_err(|e| format!("socket source: {e}"))?;

    // Client dispatch source.
    handle
        .insert_source(
            Generic::new(display, Interest::READ, Mode::Level),
            |_, display, run: &mut Run| {
                profiling_scope_display_dispatch();
                unsafe {
                    display.get_mut().dispatch_clients(&mut run.de).unwrap();
                }
                // Events we queued during dispatch (registry globals,
                // configures, frame callbacks) only reach the client after
                // an explicit flush — without this clients stall after
                // wl_display.sync.
                if let Err(e) = run.de.display_handle.flush_clients() {
                    eprintln!("flush_clients: {e}");
                }
                run.pump();
                Ok(PostAction::Continue)
            },
        )
        .map_err(|e| format!("display source: {e}"))?;

    // libseat notifier: keep the session dispatchable.
    let seat = run
        .de
        .seat_notifier
        .take()
        .ok_or("seat notifier consumed")?;
    handle
        .insert_source(seat, |_, (), _: &mut Run| {})
        .map_err(|e| format!("seat source: {e}"))?;

    // libinput.
    let libinput = de_backend::libinput::init(&run.de.session, &run.de.seat_name);
    handle
        .insert_source(libinput, |event, (), run: &mut Run| {
            run.de.process_input_event(event);
            run.pump();
        })
        .map_err(|e| format!("libinput source: {e}"))?;

    // Gesture holds must work without manufactured touch motion.
    handle.insert_source(Timer::immediate(), |_, (), run: &mut Run| {
        run.de.tick_gestures();
        if run.de.needs_present { run.pump(); }
        TimeoutAction::ToDuration(Duration::from_millis(if run.de.gestures.active() { 16 } else { 100 }))
    }).map_err(|e| format!("gesture timer: {e}"))?;

    // Clock tick — re-raster the shell frame each minute.
    let timer = Timer::immediate();
    handle
        .insert_source(timer, |_, (), run: &mut Run| {
            if run.de.shell_minute != minute_of_day() {
                run.de.needs_present = true;
                run.pump();
            }
            TimeoutAction::ToDuration(Duration::from_secs(30))
        })
        .map_err(|e| format!("timer source: {e}"))?;

    // Frame dump trigger: touch `$XDG_RUNTIME_DIR/rabbiz-de.shot` → the next
    // tick writes the composited shell frame to `rabbiz-de-shot.ppm`. Device
    // debug path — clients use `zwlr_screencopy_manager_v1` (shm only, so
    // dmabuf/EGL content is not captured there either).
    let shot_timer = Timer::immediate();
    handle
        .insert_source(shot_timer, |_, (), run: &mut Run| {
            dump_frame_if_triggered(&run.de);
            run.de.check_osk();
            run.de.poll_panels();
            run.de.poll_power();
            if run.de.needs_present { run.pump(); }
            TimeoutAction::ToDuration(Duration::from_millis(500))
        })
        .map_err(|e| format!("shot timer source: {e}"))?;

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
    match crate::osk::spawn(&mut run.de.display_handle, &socket_name) {
        Ok(child) => {
            run.de.osk_spawned_at = Some(Instant::now());
            run.de.osk = Some(child);
        }
        Err(error) => {
            eprintln!("rabbiz-de: could not start the session OSK: {error}");
            run.de.osk_respawn_at = Some(Instant::now() + Duration::from_secs(30));
        }
    }

    loop {
        let wait = [deadline, run.motion_wake_at()]
            .into_iter()
            .flatten()
            .min()
            .map(|d| d.saturating_duration_since(Instant::now()));
        event_loop
            .dispatch(wait, &mut run)
            .map_err(|e| format!("dispatch: {e}"))?;
        if !run.de.running.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
        if deadline.is_some_and(|d| Instant::now() >= d) {
            break;
        }
        if run.motion_wake_at().is_some_and(|d| Instant::now() >= d) {
            run.de.needs_present = true;
            run.pump();
        }
    }
    run.finish(print_stats);
    Ok(0)
}

#[inline]
fn profiling_scope_display_dispatch() {}
