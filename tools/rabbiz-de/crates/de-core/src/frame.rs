use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::unix::io::AsFd;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use de_api::{RendererKind, Size};
use de_backend::drm::{
    create_gbm_device, create_surface_and_compositor, inspect_drm, open_drm_device,
    BackendDrmCompositor,
};
use de_backend::libinput::{create_libinput_backend, spawn_power_key_toggle};
use de_backend::session::{open_drm_fd, open_session};
use de_protocols::{ClientState, ProtocolState};
use de_render::scanout::ScanoutCounters;
use de_render::{texture_element, RenderTarget};
use smithay::backend::allocator::Fourcc;
use smithay::backend::drm::{DrmEvent, DrmNode};
use smithay::backend::renderer::element::solid::SolidColorRenderElement;
use smithay::backend::renderer::element::surface::{
    render_elements_from_surface_tree, WaylandSurfaceRenderElement,
};
use smithay::backend::renderer::element::texture::TextureRenderElement;
use smithay::backend::renderer::element::{Id, Kind};
use smithay::backend::renderer::gles::GlesTexture;
use smithay::backend::renderer::utils::draw_render_elements;
use smithay::backend::renderer::utils::CommitCounter;
use smithay::backend::renderer::{
    Bind, Color32F, ExportMem, Frame, ImportAll, ImportDma, Offscreen, Renderer, RendererSuper,
    Texture,
};
use smithay::desktop::utils::send_frames_surface_tree;
use smithay::reexports::calloop::generic::Generic;
use smithay::reexports::calloop::{EventLoop, Interest, Mode, PostAction};
use smithay::reexports::wayland_server::{Display, DisplayHandle};
use smithay::utils::{Buffer as BufferCoord, Physical, Rectangle, Size as SmSize, Transform};
use smithay::wayland::socket::ListeningSocketSource;

use crate::present::PRESENT_FLAGS;
use crate::seat::{SeatController, SeatEffect};
use crate::state::{
    choose_initial_renderer, read_cma_free_kb, read_rss_kb, renderer_label, round2,
    summarize_cpu_ms, FrameStatsLine, RendererChoice, RunConfig, StatsSummary,
};

static STOP_REQUESTED: AtomicBool = AtomicBool::new(false);

extern "C" fn handle_signal(_sig: libc::c_int) {
    STOP_REQUESTED.store(true, Ordering::SeqCst);
}

smithay::render_elements! {
    OutputElement<R> where R: ImportAll;
    Surface=WaylandSurfaceRenderElement<R>,
    Scene=TextureRenderElement<<R as RendererSuper>::TextureId>,
    Solid=SolidColorRenderElement,
}

struct LoopState {
    display: Display<ProtocolState>,
    display_handle: DisplayHandle,
    protocols: ProtocolState,
    seat_ctl: SeatController,
    painter: de_paint::Renderer,
    shell_minute: i64,
    compositor: BackendDrmCompositor,
    render_target: RenderTarget,
    output_width: u32,
    output_height: u32,
    bar_id: Id,
    pill_id: Id,
    spawn_cmd: Option<String>,
    spawned: bool,
    ready_written: bool,
    vblank_pending: bool,
    last_vblank: Option<Instant>,
    expected_interval_ms: f64,
    total_frames: u64,
    missed_frames: u64,
    scanout: ScanoutCounters,
    last_states_sig: String,
    window_frames: u64,
    window_cpu_ms: Vec<f64>,
    all_cpu_ms: Vec<f64>,
    damage_px_window: u64,
    frame_damage_px: u64,
    layers_count: usize,
    textures_bytes: u64,
    rss_kb_peak: u64,
    cma_free_min_kb: u64,
    stats_file: Option<File>,
    start_instant: Instant,
    last_window_instant: Instant,
    first_frame_done: bool,
}

pub fn run(config: RunConfig) -> i32 {
    if config.force_fail {
        return 75;
    }

    STOP_REQUESTED.store(false, Ordering::SeqCst);
    unsafe {
        libc::signal(
            libc::SIGINT,
            handle_signal as *const () as libc::sighandler_t,
        );
        libc::signal(
            libc::SIGTERM,
            handle_signal as *const () as libc::sighandler_t,
        );
    }

    // Bind fixed wayland-0 socket before first frame (§7.20).
    let mut display: Display<ProtocolState> = match Display::new() {
        Ok(d) => d,
        Err(err) => {
            eprintln!("wayland Display::new failed: {err}");
            return 75;
        }
    };
    let display_handle = display.handle();

    let socket_source = match ListeningSocketSource::with_name("wayland-0") {
        Ok(s) => s,
        Err(err) => {
            eprintln!("ListeningSocketSource::with_name(wayland-0) failed: {err}");
            return 75;
        }
    };
    std::env::set_var("WAYLAND_DISPLAY", socket_source.socket_name());

    let (mut session, _session_notifier) = match open_session() {
        Ok(s) => s,
        Err(err) => {
            eprintln!("{err}");
            return 75;
        }
    };

    let (fd, _path) = match open_drm_fd(&mut session) {
        Ok(res) => res,
        Err(err) => {
            eprintln!("{err}");
            return 75;
        }
    };

    let (mut drm, notifier, drm_fd) = match open_drm_device(fd) {
        Ok(res) => res,
        Err(err) => {
            eprintln!("{err}");
            return 75;
        }
    };

    let drm_dev_id = DrmNode::from_file(&drm_fd).ok().map(|n| n.dev_id());

    let setup = match inspect_drm(&drm) {
        Ok(s) => s,
        Err(err) => {
            eprintln!("{err}");
            return 75;
        }
    };

    let gbm = match create_gbm_device(drm_fd.clone()) {
        Ok(g) => g,
        Err(err) => {
            eprintln!("{err}");
            return 75;
        }
    };

    let cma_kb = read_cma_free_kb();
    let initial_kind = choose_initial_renderer(config.renderer, config.require_gl, cma_kb);

    let mut render_target = match RenderTarget::new(initial_kind, Some(gbm.clone())) {
        Ok(rt) => rt,
        Err(err) => {
            if config.require_gl || config.renderer == RendererChoice::Gl {
                eprintln!("GL renderer init failed: {err}");
                return 75;
            }
            if initial_kind == RendererKind::Gl {
                eprintln!("GL renderer init failed ({err}), falling back to soft");
                match RenderTarget::new(RendererKind::Soft, None) {
                    Ok(rt) => rt,
                    Err(soft_err) => {
                        eprintln!("Soft fallback failed: {soft_err}");
                        return 75;
                    }
                }
            } else {
                eprintln!("Soft renderer init failed: {err}");
                return 75;
            }
        }
    };

    let formats = render_target.dmabuf_formats();
    let compositor = match create_surface_and_compositor(&mut drm, &drm_fd, gbm, &setup, formats) {
        Ok(c) => c,
        Err(err) => {
            eprintln!("{err}");
            return 75;
        }
    };

    // Build and rasterize P0 boot scene once.
    let scene = de_ui::boot::build(Size::new(setup.width, setup.height));
    let layers_count = scene.layers.len();
    let mut painter = de_paint::Renderer::new();
    let pixmap = painter.render(&scene);
    let textures_bytes = match render_target.upload_scene(&pixmap) {
        Ok(b) => b,
        Err(err) => {
            eprintln!("scene texture upload failed: {err}");
            return 75;
        }
    };

    let refresh_mhz = (setup.refresh_hz * 1000.0).round() as i32;
    let keymap_layout = std::env::var("XKB_DEFAULT_LAYOUT").unwrap_or_else(|_| "us,th".to_string());
    let keymap_options =
        std::env::var("XKB_DEFAULT_OPTIONS").unwrap_or_else(|_| "grp:alt_shift_toggle".to_string());
    let protocols = ProtocolState::new(
        &display_handle,
        &setup.info.connector,
        setup.width,
        setup.height,
        refresh_mhz,
        drm_dev_id,
        &keymap_layout,
        Some(&keymap_options),
    );

    let libinput_backend = match create_libinput_backend(&session, &protocols.seat.name()) {
        Ok(b) => Some(b),
        Err(err) => {
            eprintln!("libinput init warning (continuing without input): {err}");
            None
        }
    };

    let stats_file = match &config.stats_file {
        Some(path) => match OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)
        {
            Ok(f) => Some(f),
            Err(err) => {
                eprintln!("open stats file {} failed: {err}", path.display());
                None
            }
        },
        None => None,
    };

    let mut event_loop: EventLoop<'static, LoopState> = match EventLoop::try_new() {
        Ok(ev) => ev,
        Err(err) => {
            eprintln!("EventLoop::try_new failed: {err}");
            return 75;
        }
    };

    if let Err(err) = event_loop
        .handle()
        .insert_source(notifier, |event, _, st| match event {
            DrmEvent::VBlank(_) => {
                let now = Instant::now();
                if let Some(prev) = st.last_vblank {
                    let dt = now.duration_since(prev).as_secs_f64() * 1000.0;
                    if dt > 1.5 * st.expected_interval_ms {
                        st.missed_frames += 1;
                    }
                }
                st.last_vblank = Some(now);
                st.vblank_pending = false;
                let _ = st.compositor.frame_submitted();
                if !st.ready_written {
                    write_ready_file();
                    st.ready_written = true;
                }
                if !st.spawned {
                    st.spawned = true;
                    if let Some(cmd) = st.spawn_cmd.clone() {
                        spawn_detached_shell(&cmd);
                    }
                }
            }
            DrmEvent::Error(err) => {
                eprintln!("DRM error event: {err}");
            }
        })
    {
        eprintln!("insert DRM notifier failed: {err}");
        return 75;
    }

    if let Err(err) = event_loop
        .handle()
        .insert_source(socket_source, |stream, _, st| {
            if let Err(err) = st
                .display_handle
                .insert_client(stream, Arc::new(ClientState::default()))
            {
                eprintln!("insert_client failed: {err}");
            }
        })
    {
        eprintln!("insert Wayland socket source failed: {err}");
        return 75;
    }

    if let Ok(poll_fd) = display.backend().poll_fd().as_fd().try_clone_to_owned() {
        let _ = event_loop.handle().insert_source(
            Generic::new(poll_fd, Interest::READ, Mode::Level),
            |_, _, _| Ok(PostAction::Continue),
        );
    }

    if let Some(libinput) = libinput_backend {
        if let Err(err) = event_loop.handle().insert_source(libinput, |event, _, st| {
            for effect in st.seat_ctl.handle(&mut st.protocols, event) {
                match effect {
                    SeatEffect::GoHome => {
                        if let Some(toplevel) = st
                            .protocols
                            .space
                            .elements()
                            .last()
                            .and_then(|w| w.toplevel())
                        {
                            toplevel.send_close();
                        }
                    }
                    SeatEffect::TogglePower => spawn_power_key_toggle(),
                }
            }
        }) {
            eprintln!("insert libinput source failed: {err}");
        }
    }

    let expected_interval_ms = if setup.refresh_hz > 0.0 {
        1000.0 / setup.refresh_hz
    } else {
        16.67
    };

    let now = Instant::now();
    let initial_rss = read_rss_kb();
    let mut state = LoopState {
        display,
        display_handle,
        protocols,
        seat_ctl: SeatController::new(setup.width as f64, setup.height as f64),
        painter,
        shell_minute: -1,
        compositor,
        render_target,
        output_width: setup.width,
        output_height: setup.height,
        bar_id: Id::new(),
        pill_id: Id::new(),
        spawn_cmd: config.spawn.clone(),
        spawned: false,
        ready_written: false,
        vblank_pending: false,
        last_vblank: None,
        expected_interval_ms,
        total_frames: 0,
        missed_frames: 0,
        scanout: ScanoutCounters::default(),
        last_states_sig: String::new(),
        window_frames: 0,
        window_cpu_ms: Vec::with_capacity(128),
        all_cpu_ms: Vec::with_capacity(1024),
        damage_px_window: 0,
        frame_damage_px: u64::from(setup.width) * u64::from(setup.height),
        layers_count,
        textures_bytes,
        rss_kb_peak: initial_rss,
        cma_free_min_kb: cma_kb,
        stats_file,
        start_instant: now,
        last_window_instant: now,
        first_frame_done: false,
    };

    let duration_limit = config.duration.map(Duration::from_secs_f64);

    let mut screenshot_done = false;
    while !STOP_REQUESTED.load(Ordering::SeqCst) {
        if let Some(limit) = duration_limit {
            if state.start_instant.elapsed() >= limit {
                break;
            }
        }

        let _ = state.display.dispatch_clients(&mut state.protocols);
        process_pending_dmabufs(&mut state);
        state.protocols.space.refresh();
        refresh_shell_stub(&mut state);

        if !state.vblank_pending {
            if let Err(err) = submit_frame(&mut state) {
                eprintln!("frame error: {err}");
                if !state.first_frame_done {
                    return 75;
                }
                break;
            }
            state.first_frame_done = true;
        }

        if let Some(path) = &config.screenshot {
            if !screenshot_done
                && state.start_instant.elapsed().as_secs_f64() >= config.screenshot_at
            {
                screenshot_done = true;
                match capture_screenshot(&mut state, path) {
                    Ok(()) => eprintln!("screenshot written: {}", path.display()),
                    Err(err) => eprintln!("screenshot failed: {err}"),
                }
            }
        }

        let _ = state.display.flush_clients();
        let _ = event_loop.dispatch(Some(Duration::from_millis(16)), &mut state);

        if state.last_window_instant.elapsed() >= Duration::from_secs(1) {
            flush_stats_window(&mut state);
        }
    }

    let total_duration_ms = state.start_instant.elapsed().as_millis() as u64;
    let summary = StatsSummary {
        scenario: config.scenario.clone(),
        duration_ms: total_duration_ms,
        frames: state.total_frames,
        cpu_ms: summarize_cpu_ms(&state.all_cpu_ms),
        gpu_ms: None,
        missed_frames: state.missed_frames,
        rss_kb_peak: state.rss_kb_peak,
        cma_free_min_kb: state.cma_free_min_kb,
        renderer: renderer_label(state.render_target.kind()),
    };

    if let Some(summary_path) = &config.stats_summary {
        if let Ok(json) = serde_json::to_string(&summary) {
            if let Err(err) = std::fs::write(summary_path, format!("{json}\n")) {
                eprintln!("write stats summary failed: {err}");
            }
        }
    }

    if config.stats {
        if let Ok(json) = serde_json::to_string(&summary) {
            println!("{json}");
        }
    }

    0
}

/// Render the current scene into an offscreen GL texture and write it as PNG.
/// Independent of the DRM swapchain, so it also works when no frame is queued
/// (damage-driven idle). GL renderer only; the soft renderer reports an error.
fn capture_screenshot(state: &mut LoopState, path: &std::path::Path) -> Result<(), String> {
    let w = state.output_width as i32;
    let h = state.output_height as i32;
    let RenderTarget::Gl {
        renderer,
        scene_tex,
    } = &mut state.render_target
    else {
        return Err("screenshot needs the GL renderer".to_string());
    };
    let elements = build_elements(
        renderer,
        scene_tex.as_ref(),
        &state.protocols,
        w,
        h,
        &state.bar_id,
        &state.pill_id,
    );

    let buf_size: SmSize<i32, BufferCoord> = (w, h).into();
    let phys_size: SmSize<i32, Physical> = (w, h).into();
    let full: Rectangle<i32, Physical> = Rectangle::from_size(phys_size);
    let mut target: GlesTexture = renderer
        .create_buffer(Fourcc::Abgr8888, buf_size)
        .map_err(|e| format!("create_buffer: {e:?}"))?;
    let mut fb = renderer
        .bind(&mut target)
        .map_err(|e| format!("bind: {e:?}"))?;
    {
        let mut frame = renderer
            .render(&mut fb, phys_size, Transform::Normal)
            .map_err(|e| format!("render: {e:?}"))?;
        frame
            .clear(Color32F::new(0.086, 0.063, 0.102, 1.0), &[full])
            .map_err(|e| format!("clear: {e:?}"))?;
        draw_render_elements(&mut frame, 1.0, &elements, &[full])
            .map_err(|e| format!("draw: {e:?}"))?;
        let sync = frame.finish().map_err(|e| format!("finish: {e:?}"))?;
        let _ = sync.wait();
    }
    let mapping = renderer
        .copy_framebuffer(&fb, Rectangle::from_size(buf_size), Fourcc::Abgr8888)
        .map_err(|e| format!("copy_framebuffer: {e:?}"))?;
    let bytes = renderer
        .map_texture(&mapping)
        .map_err(|e| format!("map_texture: {e:?}"))?
        .to_vec();

    let size = tiny_skia::IntSize::from_wh(w as u32, h as u32).ok_or("bad size")?;
    let pixmap = tiny_skia::Pixmap::from_vec(bytes, size).ok_or("pixmap size mismatch")?;
    pixmap.save_png(path).map_err(|e| format!("save_png: {e}"))
}

fn process_pending_dmabufs(state: &mut LoopState) {
    if state.protocols.pending_dmabufs.is_empty() {
        return;
    }
    let pending: Vec<_> = state.protocols.pending_dmabufs.drain(..).collect();
    for (dmabuf, notifier) in pending {
        let ok = match &mut state.render_target {
            RenderTarget::Gl { renderer, .. } => renderer.import_dmabuf(&dmabuf, None).is_ok(),
            RenderTarget::Soft { renderer, .. } => renderer.import_dmabuf(&dmabuf, None).is_ok(),
        };
        if ok {
            let _ = notifier.successful::<ProtocolState>();
        } else {
            notifier.failed();
        }
    }
}

fn build_elements<R>(
    renderer: &mut R,
    scene_tex: Option<&smithay::backend::renderer::element::texture::TextureBuffer<R::TextureId>>,
    protocols: &ProtocolState,
    width: i32,
    height: i32,
    bar_id: &Id,
    pill_id: &Id,
) -> Vec<OutputElement<R>>
where
    R: Renderer + ImportAll,
    R::TextureId: Texture + Clone + 'static,
{
    let mut elements: Vec<OutputElement<R>> = Vec::new();

    // 32 px bottom gesture bar overlay (pill on translucent plum bar); same
    // geometry the shell stub draws when no app is mapped.
    if protocols.space.elements().next().is_some() {
        let pill = de_ui::shell_stub::nav_pill_rect(width as f32, height as f32);
        let pill_rect: Rectangle<i32, Physical> = Rectangle::new(
            (pill.x as i32, pill.y as i32).into(),
            (pill.w as i32, pill.h as i32).into(),
        );
        elements.push(OutputElement::Solid(SolidColorRenderElement::new(
            pill_id.clone(),
            pill_rect,
            CommitCounter::from(0),
            Color32F::new(0.95, 0.92, 0.88, 0.9),
            Kind::Unspecified,
        )));

        let bar = de_ui::shell_stub::nav_bar_rect(width as f32, height as f32);
        let bar_rect: Rectangle<i32, Physical> = Rectangle::new(
            (bar.x as i32, bar.y as i32).into(),
            (bar.w as i32, bar.h as i32).into(),
        );
        elements.push(OutputElement::Solid(SolidColorRenderElement::new(
            bar_id.clone(),
            bar_rect,
            CommitCounter::from(0),
            Color32F::new(0.086, 0.063, 0.102, 0.85),
            Kind::Unspecified,
        )));
    }

    // Mapped xdg_toplevel windows (front-to-back).
    for window in protocols.space.elements().rev() {
        if let Some(toplevel) = window.toplevel() {
            let win_elems: Vec<OutputElement<R>> = render_elements_from_surface_tree(
                renderer,
                toplevel.wl_surface(),
                (0, 0),
                1.0,
                1.0,
                Kind::Unspecified,
            );
            elements.extend(win_elems);
        }
    }

    // Background / boot scene texture.
    if let Some(tex) = scene_tex {
        elements.push(OutputElement::Scene(texture_element(tex)));
    }

    elements
}

fn submit_frame(state: &mut LoopState) -> Result<(), String> {
    let t0 = Instant::now();
    let clear = Color32F::new(0.086, 0.063, 0.102, 1.0);
    let flags = PRESENT_FLAGS;
    let w = state.output_width as i32;
    let h = state.output_height as i32;

    match &mut state.render_target {
        RenderTarget::Gl {
            renderer,
            scene_tex,
        } => {
            let elements = build_elements(
                renderer,
                scene_tex.as_ref(),
                &state.protocols,
                w,
                h,
                &state.bar_id,
                &state.pill_id,
            );
            let res = state
                .compositor
                .render_frame(renderer, &elements, clear, flags)
                .map_err(|err| format!("render_frame(gl) failed: {err:?}"))?;
            if res.is_empty {
                return Ok(());
            }
            observe_frame(state, &res);
        }
        RenderTarget::Soft {
            renderer,
            scene_tex,
        } => {
            let elements = build_elements(
                renderer,
                scene_tex.as_ref(),
                &state.protocols,
                w,
                h,
                &state.bar_id,
                &state.pill_id,
            );
            let res = state
                .compositor
                .render_frame(renderer, &elements, clear, flags)
                .map_err(|err| format!("render_frame(soft) failed: {err:?}"))?;
            if res.is_empty {
                return Ok(());
            }
            observe_frame(state, &res);
        }
    };

    let now_dur = state.start_instant.elapsed();
    let output = &state.protocols.output;
    for window in state.protocols.space.elements() {
        if let Some(toplevel) = window.toplevel() {
            send_frames_surface_tree(
                toplevel.wl_surface(),
                output,
                now_dur,
                Some(Duration::ZERO),
                |_, _| Some(output.clone()),
            );
        }
    }

    state
        .compositor
        .queue_frame(())
        .map_err(|err| format!("queue_frame failed: {err:?}"))?;
    state.vblank_pending = true;

    let elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
    state.total_frames += 1;
    state.window_frames += 1;
    state.damage_px_window += state.frame_damage_px;
    state.window_cpu_ms.push(elapsed_ms);
    state.all_cpu_ms.push(elapsed_ms);
    Ok(())
}

/// Observe what the frame assignment actually did (M1-T4): update the
/// direct_scanout/primary_composite counters and log the per-element
/// presentation states once per change (evidence for the S1 waiver).
fn observe_frame<B, F, E>(
    state: &mut LoopState,
    res: &smithay::backend::drm::compositor::RenderFrameResult<'_, B, F, E>,
) where
    B: smithay::backend::allocator::Buffer,
    F: smithay::backend::drm::Framebuffer,
{
    let sig = de_render::scanout::summarize_states(&res.states);
    if sig != state.last_states_sig {
        eprintln!("elements: {sig}");
        state.last_states_sig = sig;
    }
    de_render::scanout::record(&res.primary_element, &mut state.scanout);
}

fn flush_stats_window(state: &mut LoopState) {
    let rss = read_rss_kb();
    if rss > state.rss_kb_peak {
        state.rss_kb_peak = rss;
    }
    let cma = read_cma_free_kb();
    if cma > 0 && (state.cma_free_min_kb == 0 || cma < state.cma_free_min_kb) {
        state.cma_free_min_kb = cma;
    }

    let line = FrameStatsLine {
        t_ms: state.start_instant.elapsed().as_millis() as u64,
        frames: state.window_frames,
        cpu_ms: summarize_cpu_ms(&state.window_cpu_ms),
        gpu_ms: None,
        damage_px: state.damage_px_window,
        layers: state.layers_count,
        textures_mb: round2(state.textures_bytes as f64 / (1024.0 * 1024.0)),
        cma_free_kb: cma,
        rss_kb: rss,
        direct_scanout: state.scanout.direct_scanout,
        primary_composite: state.scanout.primary_composite,
        missed_frames: state.missed_frames,
        renderer: renderer_label(state.render_target.kind()),
    };

    if let Some(f) = &mut state.stats_file {
        if let Ok(json) = serde_json::to_string(&line) {
            let _ = writeln!(f, "{json}");
        }
    }

    state.window_frames = 0;
    state.window_cpu_ms.clear();
    state.damage_px_window = 0;
    state.last_window_instant = Instant::now();
}

/// Rebuild the shell-stub background when no toplevel is mapped and the
/// minute rolled over — the stub's only live datum is the clock.
fn refresh_shell_stub(state: &mut LoopState) {
    if state.protocols.space.elements().next().is_some() {
        return;
    }
    let minute = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| (d.as_secs() / 60) as i64)
        .unwrap_or(0);
    if minute == state.shell_minute {
        return;
    }
    state.shell_minute = minute;
    let (clock, date) = clock_and_date();
    let scene = de_ui::shell_stub::build(
        Size::new(state.output_width, state.output_height),
        &clock,
        &date,
    );
    let pixmap = state.painter.render(&scene);
    match state.render_target.upload_scene(&pixmap) {
        Ok(bytes) => state.textures_bytes = bytes,
        Err(err) => eprintln!("shell stub texture upload failed: {err}"),
    }
}

/// Local time as ("HH:MM", "Tue, Nov 17") via libc — no extra deps.
fn clock_and_date() -> (String, String) {
    const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&secs, &mut tm) };
    let clock = format!("{:02}:{:02}", tm.tm_hour, tm.tm_min);
    let weekday = WEEKDAYS.get(tm.tm_wday as usize).copied().unwrap_or("");
    let month = MONTHS.get(tm.tm_mon as usize).copied().unwrap_or("");
    (clock, format!("{weekday}, {month} {}", tm.tm_mday))
}

fn write_ready_file() {
    let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") else {
        return;
    };
    let path = std::path::PathBuf::from(runtime_dir).join("rabbiz-de.ready");
    let pid = std::process::id();
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let _ = std::fs::write(path, format!("{pid} {now_ms}\n"));
}

fn spawn_detached_shell(cmd_str: &str) {
    let mut cmd = Command::new("/bin/sh");
    cmd.arg("-c")
        .arg(cmd_str)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    unsafe {
        cmd.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    if let Err(err) = cmd.spawn() {
        eprintln!("--spawn '{cmd_str}' failed: {err}");
    }
}
