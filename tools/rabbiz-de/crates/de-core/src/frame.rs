//! Frame pump + stats (M0-T4/M1-T4): present the scene every vblank while
//! damage exists, count direct-scanout vs primary-composite, record
//! per-second JSONL stats, emit the final summary.

use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use de_api::{FrameStats, StatsSummary, TimingSummary};
use de_render::DeRenderer;
use smithay::backend::drm::compositor::{FrameFlags, PrimaryPlaneElement};
use smithay::backend::renderer::element::render_elements;
use smithay::backend::renderer::element::surface::WaylandSurfaceRenderElement;
use smithay::backend::renderer::element::texture::TextureRenderElement;
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::{ImportAll, ImportMem, Renderer, Texture};
use smithay::backend::renderer::element::AsRenderElements;
use smithay::reexports::calloop::{EventLoop, Interest, Mode, PostAction};
use smithay::reexports::calloop::generic::Generic;
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::reexports::wayland_server::Display;
use smithay::utils::{Physical, Point, Scale};
use smithay::wayland::socket::ListeningSocketSource;

use crate::state::{cma_free_kb, rss_kb, De, DeClientState};

render_elements! {
    /// Elements submitted to `DrmCompositor::render_frame` each frame.
    pub DeElement<R> where R: ImportAll + ImportMem, R::TextureId: Texture + Clone + 'static;
    Texture = TextureRenderElement<R::TextureId>,
    Surface = WaylandSurfaceRenderElement<R>,
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
        })
    }

    /// Render + queue one frame on the output (only when there is damage —
    /// `EmptyFrame` is a no-op and simply lets the screen hold still).
    pub fn present(&mut self) -> Result<(), String> {
        let t0 = Instant::now();
        // Split `de` into field borrows so `renderer` can be passed
        // mutably alongside the rest of the scene state.
        let de = &mut self.de;
        let crate::state::De {
            renderer,
            compositor,
            space,
            shell_stub,
            gesture_bar,
            stub_minute,
            output,
            output_obj,
            clock,
            primary_composite,
            direct_scanout,
            ..
        } = de;
        let mut parts = FrameParts {
            compositor,
            space,
            shell_stub,
            gesture_bar,
            stub_minute,
            output,
            output_obj,
            clock,
            primary_composite,
            direct_scanout,
        };
        match renderer {
            DeRenderer::Gl(r) => render_with(&mut parts, r)?,
            DeRenderer::Soft(r) => render_with(&mut parts, r)?,
        }
        let cpu = t0.elapsed().as_secs_f64() * 1000.0;
        self.cpu_samples.push(cpu);
        self.all_cpu.push(cpu);
        Ok(())
    }
}

/// Field-split view of `De` for [`render_with`], so the renderer can be
/// borrowed mutably at the same time as the scene state.
struct FrameParts<'a> {
    compositor: &'a mut de_backend::probe::DeCompositor,
    space: &'a mut smithay::desktop::Space<smithay::desktop::Window>,
    shell_stub: &'a mut Option<Box<dyn std::any::Any>>,
    gesture_bar: &'a mut Option<Box<dyn std::any::Any>>,
    stub_minute: &'a mut u64,
    output: &'a de_backend::drm::OutputSelection,
    output_obj: &'a smithay::output::Output,
    clock: &'a smithay::utils::Clock<smithay::utils::Monotonic>,
    primary_composite: &'a mut u64,
    direct_scanout: &'a mut u64,
}

fn render_with<R>(de: &mut FrameParts<'_>, renderer: &mut R) -> Result<(), String>
where
    R: Renderer
        + ImportAll
        + ImportMem
        + smithay::backend::renderer::Bind<smithay::backend::allocator::dmabuf::Dmabuf>,
    R::TextureId: smithay::backend::renderer::Texture + Clone + 'static,
{
    // Lazy shell-stub texture (home screen w/ clock). Re-raster when the
    // minute changes — otherwise the cached texture gives no damage and
    // the frame is simply EmptyFrame (screen holds still).
    let minute = minute_of_day();
    if *de.stub_minute != minute || de.shell_stub.is_none() {
        *de.stub_minute = minute;
        *de.shell_stub = None;
        let px = de_ui::shell_stub::home(de_api::Size::new(
            de.output.width as i32,
            de.output.height as i32,
        ));
        *de.shell_stub = upload_any(renderer, &px);
    }
    if de.gesture_bar.is_none() {
        let px = de_ui::shell_stub::gesture_bar(de_api::Size::new(
            de.output.width as i32,
            de.output.height as i32,
        ));
        *de.gesture_bar = upload_any(renderer, &px);
    }

    let mut elements: Vec<DeElement<R>> = Vec::new();
    let has_windows = de.space.elements().next().is_some();
    if !has_windows {
        if let Some(tex) = texture_buffer::<R>(de.shell_stub.as_ref()) {
            elements.push(DeElement::from(tex_el::<R>(
                tex,
                Point::<f64, Physical>::from((0.0, 0.0)),
            )));
        }
    } else {
        // Windows bottom-to-top.
        for window in de.space.elements() {
            let loc = de
                .space
                .element_location(window)
                .unwrap_or_default()
                .to_physical(1);
            elements.extend(AsRenderElements::<R>::render_elements::<DeElement<R>>(
                window,
                renderer,
                loc,
                Scale::from(1.0),
                1.0,
            ));
        }
    }
    // Gesture bar on top of everything.
    let bar_y = de.output.height as f64 - de_ui::shell_stub::GESTURE_BAR_H as f64;
    if let Some(tex) = texture_buffer::<R>(de.gesture_bar.as_ref()) {
        elements.push(DeElement::from(tex_el::<R>(
            tex,
            Point::<f64, Physical>::from((0.0, bar_y)),
        )));
    }

    let res = de
        .compositor
        .render_frame(renderer, &elements, [0.086, 0.063, 0.102, 1.0], FrameFlags::DEFAULT)
        .map_err(|e| format!("render: {e:?}"))?;

    if let Err(e) = de.compositor.queue_frame(()) {
        if !format!("{e:?}").contains("EmptyFrame") {
            return Err(format!("queue: {e:?}"));
        }
        return Ok(());
    }

    // M1-T4 counters from the observed result, not a guess.
    match res.primary_element {
        PrimaryPlaneElement::Swapchain(_) => *de.primary_composite += 1,
        PrimaryPlaneElement::Element(_) => *de.direct_scanout += 1,
    }

    // Frame callbacks for the freshly-committed surfaces.
    let time = de.clock.now();
    let out = de.output_obj.clone();
    for window in de.space.elements() {
        window.send_frame(&out, time, Some(Duration::from_millis(16)), |_, _| {
            Some(out.clone())
        });
    }
    Ok(())
}

impl Run {

    /// Called after client/input dispatch — present only when a commit or
    /// shell change marked the scene dirty.
    pub fn pump(&mut self) {
        if self.de.needs_present {
            self.de.needs_present = false;
            let _ = self.present().map_err(|e| eprintln!("present: {e}"));
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
            cma_free_min_kb: if self.cma_min == u64::MAX { 0 } else { self.cma_min },
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

fn minute_of_day() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() / 60)
        .unwrap_or(0)
}

/// Run the session event loop until `run.duration` elapses (session mode:
/// no duration → run until killed).
pub fn event_loop(
    mut run: Run,
    display: Display<De>,
    print_stats: bool,
) -> Result<u8, String> {
    let mut event_loop: EventLoop<Run> =
        EventLoop::try_new().map_err(|e| format!("event loop: {e}"))?;
    let handle = event_loop.handle();

    // Wayland client socket: fixed name `wayland-0` (§7.20).
    let socket = ListeningSocketSource::with_name("wayland-0")
        .map_err(|e| format!("wayland socket: {e:?}"))?;
    run.de.socket_name = Some(socket.socket_name().to_string_lossy().into_owned());
    handle
        .insert_source(socket, |stream, _, run: &mut Run| {
            if let Err(err) = run
                .de
                .display_handle
                .insert_client(stream, Arc::new(DeClientState::default()))
            {
                eprintln!("insert_client: {err}");
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
        .insert_source(seat, |_, _, _: &mut Run| {})
        .map_err(|e| format!("seat source: {e}"))?;

    // libinput.
    let libinput = de_backend::libinput::init(&run.de.session, &run.de.seat_name);
    handle
        .insert_source(libinput, |event, _, run: &mut Run| {
            run.de.process_input_event(event);
            run.pump();
        })
        .map_err(|e| format!("libinput source: {e}"))?;

    // Clock tick — re-raster the home stub each minute.
    let timer = Timer::immediate();
    handle
        .insert_source(timer, |_, _, run: &mut Run| {
            if run.de.stub_minute != minute_of_day() {
                run.de.shell_stub_dirty = true;
                run.de.needs_present = true;
                run.pump();
            }
            TimeoutAction::ToDuration(Duration::from_secs(30))
        })
        .map_err(|e| format!("timer source: {e}"))?;

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
        if !run.de.running.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
        if deadline.is_some_and(|d| Instant::now() >= d) {
            break;
        }
    }
    run.finish(print_stats);
    Ok(0)
}

#[inline]
fn profiling_scope_display_dispatch() {}
