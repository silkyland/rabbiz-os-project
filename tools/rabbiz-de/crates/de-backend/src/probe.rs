use std::time::{Duration, Instant};

use serde::Serialize;
use smithay::backend::allocator::gbm::GbmDevice;
use smithay::backend::drm::compositor::FrameFlags;
use smithay::backend::drm::{DrmDevice, DrmDeviceFd, DrmEvent};
use smithay::backend::egl::{EGLContext, EGLDisplay};
use smithay::backend::renderer::element::memory::MemoryRenderBufferRenderElement;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::renderer::pixman::PixmanRenderer;
use smithay::backend::renderer::{Bind, Color32F, ImportDma, Renderer};
use smithay::reexports::calloop::EventLoop;

use crate::drm::{
    create_gbm_device, create_surface_and_compositor, format_fourcc, inspect_drm, open_drm_device,
    DrmInfo, DrmOutputSetup,
};
use crate::session::{open_drm_fd, open_session};

const PROBE_FRAMES: usize = 100;

#[derive(Debug, Clone, Serialize)]
pub struct GlProbeResult {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub renderer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub egl_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SoftProbeResult {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub formats: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProbeReport {
    pub frame_interval_ms: f64,
    pub drm: Option<DrmInfo>,
    pub gl: GlProbeResult,
    pub soft: SoftProbeResult,
    pub errors: Vec<String>,
}

struct ProbeLoopState {
    vblank_times: Vec<Instant>,
    compositor_ready: bool,
    last_error: Option<String>,
}

pub fn run_probe(require_gl: bool) -> (ProbeReport, i32) {
    let mut errors = Vec::new();

    let (mut session, _session_notifier) = match open_session() {
        Ok(s) => s,
        Err(err) => {
            errors.push(err.clone());
            return (
                ProbeReport {
                    frame_interval_ms: 0.0,
                    drm: None,
                    gl: GlProbeResult {
                        ok: false,
                        renderer: None,
                        egl_version: None,
                        error: Some(err.clone()),
                    },
                    soft: SoftProbeResult {
                        ok: false,
                        formats: None,
                        error: Some(err),
                    },
                    errors,
                },
                75,
            );
        }
    };

    let (fd, _path) = match open_drm_fd(&mut session) {
        Ok(res) => res,
        Err(err) => {
            errors.push(err.clone());
            return (
                ProbeReport {
                    frame_interval_ms: 0.0,
                    drm: None,
                    gl: GlProbeResult {
                        ok: false,
                        renderer: None,
                        egl_version: None,
                        error: Some(err.clone()),
                    },
                    soft: SoftProbeResult {
                        ok: false,
                        formats: None,
                        error: Some(err),
                    },
                    errors,
                },
                75,
            );
        }
    };

    let (mut drm, notifier, drm_fd) = match open_drm_device(fd) {
        Ok(res) => res,
        Err(err) => {
            errors.push(err.clone());
            return (
                ProbeReport {
                    frame_interval_ms: 0.0,
                    drm: None,
                    gl: GlProbeResult {
                        ok: false,
                        renderer: None,
                        egl_version: None,
                        error: Some(err.clone()),
                    },
                    soft: SoftProbeResult {
                        ok: false,
                        formats: None,
                        error: Some(err),
                    },
                    errors,
                },
                75,
            );
        }
    };

    let setup = match inspect_drm(&drm) {
        Ok(s) => s,
        Err(err) => {
            errors.push(err.clone());
            return (
                ProbeReport {
                    frame_interval_ms: 0.0,
                    drm: None,
                    gl: GlProbeResult {
                        ok: false,
                        renderer: None,
                        egl_version: None,
                        error: Some(err.clone()),
                    },
                    soft: SoftProbeResult {
                        ok: false,
                        formats: None,
                        error: Some(err),
                    },
                    errors,
                },
                75,
            );
        }
    };

    let gbm = match create_gbm_device(drm_fd.clone()) {
        Ok(g) => g,
        Err(err) => {
            errors.push(err.clone());
            return (
                ProbeReport {
                    frame_interval_ms: 0.0,
                    drm: Some(setup.info),
                    gl: GlProbeResult {
                        ok: false,
                        renderer: None,
                        egl_version: None,
                        error: Some(err.clone()),
                    },
                    soft: SoftProbeResult {
                        ok: false,
                        formats: None,
                        error: Some(err),
                    },
                    errors,
                },
                75,
            );
        }
    };

    let mut event_loop: EventLoop<'static, ProbeLoopState> = match EventLoop::try_new() {
        Ok(ev) => ev,
        Err(err) => {
            let msg = format!("calloop EventLoop::try_new failed: {err}");
            errors.push(msg.clone());
            return (
                ProbeReport {
                    frame_interval_ms: 0.0,
                    drm: Some(setup.info),
                    gl: GlProbeResult {
                        ok: false,
                        renderer: None,
                        egl_version: None,
                        error: Some(msg.clone()),
                    },
                    soft: SoftProbeResult {
                        ok: false,
                        formats: None,
                        error: Some(msg),
                    },
                    errors,
                },
                75,
            );
        }
    };

    let mut state = ProbeLoopState {
        vblank_times: Vec::with_capacity(PROBE_FRAMES),
        compositor_ready: false,
        last_error: None,
    };

    if let Err(err) = event_loop
        .handle()
        .insert_source(notifier, |event, _, st| match event {
            DrmEvent::VBlank(_) => {
                st.vblank_times.push(Instant::now());
                st.compositor_ready = true;
            }
            DrmEvent::Error(e) => {
                st.last_error = Some(format!("DRM event error: {e}"));
            }
        })
    {
        let msg = format!("insert DRM notifier failed: {err}");
        errors.push(msg.clone());
        return (
            ProbeReport {
                frame_interval_ms: 0.0,
                drm: Some(setup.info),
                gl: GlProbeResult {
                    ok: false,
                    renderer: None,
                    egl_version: None,
                    error: Some(msg.clone()),
                },
                soft: SoftProbeResult {
                    ok: false,
                    formats: None,
                    error: Some(msg),
                },
                errors,
            },
            75,
        );
    }

    let mut frame_interval_ms = 0.0;

    let gl_res = probe_gl(
        &mut drm,
        &drm_fd,
        gbm.clone(),
        &setup,
        &mut event_loop,
        &mut state,
    );
    let gl_probe = match gl_res {
        Ok((renderer_name, egl_version, interval)) => {
            frame_interval_ms = interval;
            GlProbeResult {
                ok: true,
                renderer: Some(renderer_name),
                egl_version: Some(egl_version),
                error: None,
            }
        }
        Err(err) => {
            errors.push(format!("gl: {err}"));
            GlProbeResult {
                ok: false,
                renderer: None,
                egl_version: None,
                error: Some(err),
            }
        }
    };

    let soft_res = probe_soft(&mut drm, &drm_fd, gbm, &setup, &mut event_loop, &mut state);
    let soft_probe = match soft_res {
        Ok((formats, interval)) => {
            if frame_interval_ms <= 0.0 {
                frame_interval_ms = interval;
            }
            SoftProbeResult {
                ok: true,
                formats: Some(formats),
                error: None,
            }
        }
        Err(err) => {
            errors.push(format!("soft: {err}"));
            SoftProbeResult {
                ok: false,
                formats: None,
                error: Some(err),
            }
        }
    };

    let exit_code = if require_gl {
        if gl_probe.ok {
            0
        } else {
            1
        }
    } else if gl_probe.ok || soft_probe.ok {
        0
    } else {
        1
    };

    let report = ProbeReport {
        frame_interval_ms,
        drm: Some(setup.info),
        gl: gl_probe,
        soft: soft_probe,
        errors,
    };

    (report, exit_code)
}

fn probe_gl(
    drm: &mut DrmDevice,
    drm_fd: &DrmDeviceFd,
    gbm: GbmDevice<DrmDeviceFd>,
    setup: &DrmOutputSetup,
    event_loop: &mut EventLoop<'static, ProbeLoopState>,
    state: &mut ProbeLoopState,
) -> Result<(String, String, f64), String> {
    let display = unsafe { EGLDisplay::new(gbm.clone()) }
        .map_err(|err| format!("EGLDisplay::new failed: {err}"))?;
    let egl_version = format!("{:?}", display.get_egl_version());
    let context =
        EGLContext::new(&display).map_err(|err| format!("EGLContext::new failed: {err}"))?;
    let mut renderer = unsafe { GlesRenderer::new(context) }
        .map_err(|err| format!("GlesRenderer::new failed: {err}"))?;

    let gl_renderer_name = renderer
        .with_context(|gl| unsafe {
            let ptr = gl.GetString(smithay::backend::renderer::gles::ffi::RENDERER);
            if ptr.is_null() {
                "unknown".to_string()
            } else {
                std::ffi::CStr::from_ptr(ptr as *const std::ffi::c_char)
                    .to_string_lossy()
                    .into_owned()
            }
        })
        .unwrap_or_else(|_| "unknown".to_string());

    let formats: Vec<_> = renderer.dmabuf_formats().into_iter().collect();
    let mut compositor = create_surface_and_compositor(drm, drm_fd, gbm, setup, formats)?;

    let interval = run_100_clear_frames(&mut compositor, &mut renderer, event_loop, state)?;
    Ok((gl_renderer_name, egl_version, interval))
}

fn probe_soft(
    drm: &mut DrmDevice,
    drm_fd: &DrmDeviceFd,
    gbm: GbmDevice<DrmDeviceFd>,
    setup: &DrmOutputSetup,
    event_loop: &mut EventLoop<'static, ProbeLoopState>,
    state: &mut ProbeLoopState,
) -> Result<(Vec<String>, f64), String> {
    let mut renderer =
        PixmanRenderer::new().map_err(|err| format!("PixmanRenderer::new failed: {err}"))?;
    let formats: Vec<_> = renderer.dmabuf_formats().into_iter().collect();
    let mut format_names: Vec<String> = formats.iter().map(|f| format_fourcc(f.code)).collect();
    format_names.sort();
    format_names.dedup();

    let mut compositor = create_surface_and_compositor(drm, drm_fd, gbm, setup, formats)?;

    let interval = run_100_clear_frames(&mut compositor, &mut renderer, event_loop, state)?;
    Ok((format_names, interval))
}

fn run_100_clear_frames<R>(
    compositor: &mut crate::drm::BackendDrmCompositor,
    renderer: &mut R,
    event_loop: &mut EventLoop<'static, ProbeLoopState>,
    state: &mut ProbeLoopState,
) -> Result<f64, String>
where
    R: Renderer
        + Bind<smithay::backend::allocator::dmabuf::Dmabuf>
        + smithay::backend::renderer::ImportMem,
    R::TextureId: smithay::backend::renderer::Texture + 'static,
{
    state.vblank_times.clear();
    state.last_error = None;

    let empty: [MemoryRenderBufferRenderElement<R>; 0] = [];

    for i in 0..PROBE_FRAMES {
        let clear = if i % 2 == 0 {
            Color32F::new(0.086, 0.063, 0.102, 1.0)
        } else {
            Color32F::new(0.090, 0.067, 0.106, 1.0)
        };
        let res = compositor
            .render_frame(renderer, &empty, clear, FrameFlags::empty())
            .map_err(|err| format!("render_frame failed: {err:?}"))?;

        if res.is_empty {
            continue;
        }

        compositor
            .queue_frame(())
            .map_err(|err| format!("queue_frame failed: {err:?}"))?;

        state.compositor_ready = false;
        let deadline = Instant::now() + Duration::from_millis(500);
        while !state.compositor_ready && Instant::now() < deadline {
            event_loop
                .dispatch(Some(Duration::from_millis(50)), state)
                .map_err(|err| format!("event_loop dispatch failed: {err}"))?;
            if let Some(err) = state.last_error.take() {
                return Err(err);
            }
        }
        let _ = compositor.frame_submitted();
    }

    if state.vblank_times.len() < 2 {
        return Ok(16.67);
    }

    let mut sum_ms = 0.0;
    for pair in state.vblank_times.windows(2) {
        sum_ms += pair[1].duration_since(pair[0]).as_secs_f64() * 1000.0;
    }
    let mean = sum_ms / ((state.vblank_times.len() - 1) as f64);
    Ok((mean * 100.0).round() / 100.0)
}
