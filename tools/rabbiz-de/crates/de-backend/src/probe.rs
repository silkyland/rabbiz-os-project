//! `rabbiz-de probe` — spike S1 (M0-T3).
//!
//! Opens the card through libseat, enumerates connector/mode/planes, brings up
//! `DrmDevice` + `GbmDevice` + `EGLDevice` + `GbmAllocator` +
//! `GbmFramebufferExporter` + `DrmCompositor`, then runs 100 clear/commit
//! frames on GL and again on pixman. Exactly one JSON object on stdout
//! (§7.21 stdout purity); diagnostics go to stderr.

use std::time::{Duration, Instant};

use serde::Serialize;
use smithay::backend::allocator::gbm::{GbmAllocator, GbmDevice};
use smithay::backend::allocator::Fourcc as DrmFourcc;
use smithay::backend::drm::compositor::{DrmCompositor, FrameFlags};
use smithay::backend::drm::exporter::gbm::GbmFramebufferExporter;
use smithay::backend::drm::{DrmDeviceFd, DrmEvent};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::renderer::pixman::PixmanRenderer;
use smithay::output::OutputModeSource;
use smithay::reexports::calloop::EventLoop;
use smithay::utils::{Buffer, Scale, Size, Transform};

use crate::drm::{self, DrmSetup, OutputSelection};
use crate::session::SessionError;

/// Compositor type used everywhere in M0 (allocator + fb exporter + no user data).
pub type DeCompositor =
    DrmCompositor<GbmAllocator<DrmDeviceFd>, GbmFramebufferExporter<DrmDeviceFd>, (), DrmDeviceFd>;

/// Number of frames committed per renderer during the probe.
const PROBE_FRAMES: u32 = 100;

#[derive(Serialize)]
struct PlaneReport {
    name: String,
    #[serde(rename = "type")]
    type_: String,
    formats: Vec<String>,
}

#[derive(Serialize)]
struct DrmReport {
    connector: String,
    mode: String,
    refresh_hz: f64,
    atomic: bool,
    planes: Vec<PlaneReport>,
    formats: Vec<String>,
}

#[derive(Serialize)]
struct RendererReport {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    renderer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    egl_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    formats: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    frames: Option<u32>,
}

#[derive(Serialize)]
struct ProbeReport {
    frame_interval_ms: f64,
    drm: DrmReport,
    gl: RendererReport,
    soft: RendererReport,
    errors: Vec<String>,
}

/// Run the probe; returns `(report_json, any_renderer_ok)`.
#[allow(clippy::too_many_lines)] // one sequential bring-up, step order is the documentation
pub fn run(require_gl: bool) -> Result<(serde_json::Value, bool), SessionError> {
    let mut errors: Vec<String> = Vec::new();

    let card = drm::find_card().ok_or_else(|| SessionError::Session("no /dev/dri/card*".into()))?;
    eprintln!("probe: card {}", card.display());

    let (mut session, _seat_notifier) = crate::session::activate()?;
    let mut setup = drm::open_drm(&mut session, &card)?;

    let output = select_or_report(&mut setup, &mut errors)?;

    // GL bring-up (optional but recorded).
    let mut gl = RendererReport {
        ok: false,
        renderer: None,
        egl_version: None,
        formats: None,
        error: None,
        frames: None,
    };
    let mut gles: Option<GlesRenderer> = None;
    match drm::init_egl(&mut setup) {
        Ok(()) => {
            gl.egl_version.clone_from(&setup.egl_version);
            if let Some(display) = setup.egl_display.clone() {
                match de_render::gl::create(&display) {
                    Ok(mut r) => {
                        gl.renderer = Some(gl_renderer_name(&mut r));
                        gles = Some(r);
                        gl.ok = true;
                    }
                    Err(e) => {
                        gl.error = Some(e.to_string());
                        errors.push(format!("gles renderer: {e}"));
                    }
                }
            }
        }
        Err(e) => {
            gl.error = Some(e.to_string());
            errors.push(format!("egl: {e}"));
        }
    }
    if require_gl && gles.is_none() {
        errors.push("--require=gl: GL failed to initialize".to_string());
    }

    // Soft renderer (must at least construct).
    let mut soft = RendererReport {
        ok: false,
        renderer: Some("pixman".to_string()),
        egl_version: None,
        formats: Some(vec!["ARGB8888".to_string(), "XRGB8888".to_string()]),
        error: None,
        frames: None,
    };
    let pixman = match de_render::soft::create() {
        Ok(r) => {
            soft.ok = true;
            Some(r)
        }
        Err(e) => {
            soft.error = Some(format!("{e}"));
            errors.push(format!("pixman: {e}"));
            None
        }
    };

    let drm_report = build_drm_report(&mut setup, &output)?;

    // Frame pacing measurement — GL first, soft fallback per plan. The
    // vblank notifier is a single source shared by both renderer runs.
    let mut frame_interval_ms = 1000.0 / output.refresh_hz.max(1.0);
    let mut event_loop: EventLoop<VblankState> =
        EventLoop::try_new().map_err(|e| SessionError::Session(format!("event loop: {e}")))?;
    let notifier = setup
        .notifier
        .take()
        .ok_or_else(|| SessionError::Session("drm notifier consumed".into()))?;
    event_loop
        .handle()
        .insert_source(notifier, |event, _meta, state: &mut VblankState| {
            if let DrmEvent::VBlank(_) = event {
                state.vblanks += 1;
            }
        })
        .map_err(|e| SessionError::Session(format!("insert drm source: {e}")))?;
    let mut vblank_state = VblankState { vblanks: 0 };
    if let Some(r) = gles.as_mut() {
        match pace_frames(
            &mut setup,
            &output,
            Renderer::Gl(r),
            PROBE_FRAMES,
            &mut event_loop,
            &mut vblank_state,
        ) {
            Ok(ms) => {
                frame_interval_ms = ms;
                gl.frames = Some(PROBE_FRAMES);
            }
            Err(e) => {
                gl.ok = false;
                gl.error = Some(format!("frames: {e}"));
                errors.push(format!("gl frames: {e}"));
            }
        }
    }
    if let Some(mut r) = pixman {
        match pace_frames(
            &mut setup,
            &output,
            Renderer::Soft(&mut r),
            PROBE_FRAMES,
            &mut event_loop,
            &mut vblank_state,
        ) {
            Ok(ms) => {
                if gles.is_none() {
                    frame_interval_ms = ms;
                }
                soft.frames = Some(PROBE_FRAMES);
            }
            Err(e) => {
                soft.ok = false;
                soft.error = Some(format!("frames: {e}"));
                errors.push(format!("soft frames: {e}"));
            }
        }
    }

    let ok = (gles.is_some() && gl.ok) || soft.ok;
    let ok = if require_gl { gl.ok } else { ok };

    let report = ProbeReport {
        frame_interval_ms: round3(frame_interval_ms),
        drm: drm_report,
        gl,
        soft,
        errors,
    };
    Ok((serde_json::to_value(&report).unwrap_or_default(), ok))
}

enum Renderer<'a> {
    Gl(&'a mut GlesRenderer),
    Soft(&'a mut PixmanRenderer),
}

fn select_or_report(
    setup: &mut DrmSetup,
    errors: &mut Vec<String>,
) -> Result<OutputSelection, SessionError> {
    if let Some(o) = drm::select_output(&mut setup.device)? {
        return Ok(o);
    }
    errors.push("no connected connector".to_string());
    Err(SessionError::Session("no connected connector".into()))
}

/// `glGetString(GL_RENDERER)` through the smithay context — e.g. "Mali-G52".
#[allow(unsafe_code)] // raw GL string pointer read inside the EGL context
fn gl_renderer_name(r: &mut GlesRenderer) -> String {
    r.with_context(|gl| unsafe {
        let ptr = gl.GetString(smithay::backend::renderer::gles::ffi::RENDERER);
        if ptr.is_null() {
            "GLES".to_string()
        } else {
            std::ffi::CStr::from_ptr(ptr.cast::<std::ffi::c_char>())
                .to_string_lossy()
                .into_owned()
        }
    })
    .unwrap_or_else(|_| "GLES".to_string())
}

fn build_drm_report(
    setup: &mut DrmSetup,
    output: &OutputSelection,
) -> Result<DrmReport, SessionError> {
    let planes = setup
        .device
        .planes(&output.crtc)
        .map_err(|e| SessionError::Session(format!("planes: {e}")))?;
    let mut plane_reports = Vec::new();
    for (kind, list) in [
        ("Primary", &planes.primary),
        ("Overlay", &planes.overlay),
        ("Cursor", &planes.cursor),
    ] {
        for p in list {
            plane_reports.push(PlaneReport {
                name: format!("{}-{}", kind.to_lowercase(), u32::from(p.handle)),
                type_: kind.to_string(),
                formats: p.formats.iter().map(|f| fourcc_name(f.code)).collect(),
            });
        }
    }
    Ok(DrmReport {
        connector: output.connector_name.clone(),
        mode: format!("{}x{}", output.width, output.height),
        refresh_hz: round3(output.refresh_hz),
        atomic: setup.device.is_atomic(),
        planes: plane_reports,
        formats: planes
            .primary
            .iter()
            .flat_map(|p| p.formats.iter().map(|f| fourcc_name(f.code)))
            .collect(),
    })
}

/// Commit `frames` texture/clear frames and return the mean vblank interval
/// in milliseconds. Blocks the calling thread; ~1.7 s at 60 Hz.
#[allow(clippy::too_many_lines)] // vblank pacing is one sequential loop
fn pace_frames(
    setup: &mut DrmSetup,
    output: &OutputSelection,
    mut renderer: Renderer<'_>,
    frames: u32,
    event_loop: &mut EventLoop<VblankState>,
    state: &mut VblankState,
) -> Result<f64, SessionError> {
    let surface = setup
        .device
        .create_surface(output.crtc, output.mode, &[output.connector])
        .map_err(|e| SessionError::Session(format!("surface: {e}")))?;
    let planes = setup
        .device
        .planes(&output.crtc)
        .map_err(|e| SessionError::Session(format!("planes: {e}")))?;
    let (allocator, exporter) = drm::allocators(setup);
    let gbm: GbmDevice<DrmDeviceFd> = setup.gbm.clone();
    let mut compositor: DeCompositor = DrmCompositor::new(
        OutputModeSource::Static {
            size: Size::from((output.width as i32, output.height as i32)),
            scale: Scale::from(1.0),
            transform: Transform::Normal,
        },
        surface,
        Some(planes),
        allocator,
        exporter,
        [DrmFourcc::Argb8888, DrmFourcc::Xrgb8888],
        match &renderer {
            Renderer::Gl(r) => r
                .egl_context()
                .dmabuf_render_formats()
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            Renderer::Soft(_) => [
                smithay::backend::allocator::Format {
                    code: DrmFourcc::Argb8888,
                    modifier: smithay::backend::allocator::Modifier::Linear,
                },
                smithay::backend::allocator::Format {
                    code: DrmFourcc::Xrgb8888,
                    modifier: smithay::backend::allocator::Modifier::Linear,
                },
            ]
            .into_iter()
            .collect(),
        },
        Size::<u32, Buffer>::from((64, 64)),
        Some(gbm),
    )
    .map_err(|e| SessionError::Session(format!("compositor: {e:?}")))?;

    // Upload a small solid texture so render_frame has real work.
    let mut intervals = Vec::with_capacity(frames as usize);
    let mut last = Instant::now();
    for _ in 0..frames {
        match &mut renderer {
            Renderer::Gl(r) => {
                let buf = make_tex_gl(r)?;
                let el = de_render::target::texture_element::<
                    smithay::backend::renderer::gles::GlesRenderer,
                >(&buf, (0.0, 0.0).into());
                compositor
                    .render_frame(
                        &mut **r,
                        &[el],
                        [0.086, 0.063, 0.102, 1.0],
                        FrameFlags::DEFAULT,
                    )
                    .map_err(|e| SessionError::Session(format!("render: {e:?}")))?;
            }
            Renderer::Soft(r) => {
                let buf = make_tex_soft(r)?;
                let el = de_render::target::texture_element::<
                    smithay::backend::renderer::pixman::PixmanRenderer,
                >(&buf, (0.0, 0.0).into());
                compositor
                    .render_frame(
                        &mut **r,
                        &[el],
                        [0.086, 0.063, 0.102, 1.0],
                        FrameFlags::DEFAULT,
                    )
                    .map_err(|e| SessionError::Session(format!("render: {e:?}")))?;
            }
        }
        compositor
            .queue_frame(())
            .map_err(|e| SessionError::Session(format!("queue: {e:?}")))?;

        // Wait for the vblank, then release the pending frame — without
        // `frame_submitted` the compositor never submits the next queued
        // frame and every later vblank wait times out.
        let target = state.vblanks + 1;
        let deadline = Instant::now() + Duration::from_secs(2);
        while state.vblanks < target {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(SessionError::Session("vblank timeout".into()));
            }
            event_loop
                .dispatch(Some(left), state)
                .map_err(|e| SessionError::Session(format!("dispatch: {e}")))?;
        }
        compositor
            .frame_submitted()
            .map_err(|e| SessionError::Session(format!("submitted: {e:?}")))?;
        let now = Instant::now();
        intervals.push(now.duration_since(last).as_secs_f64() * 1000.0);
        last = now;
    }

    // Drop the first vsync-uncertain samples.
    let samples: Vec<f64> = intervals.iter().skip(2).copied().collect();
    Ok(if samples.is_empty() {
        0.0
    } else {
        samples.iter().sum::<f64>() / samples.len() as f64
    })
}

/// Map a DRM fourcc to the plan's uppercase name (§M0-T3 schema), falling
/// back to the raw 4-character code.
fn fourcc_name(code: DrmFourcc) -> String {
    match code {
        DrmFourcc::Argb8888 => "ARGB8888".into(),
        DrmFourcc::Xrgb8888 => "XRGB8888".into(),
        DrmFourcc::Abgr8888 => "ABGR8888".into(),
        DrmFourcc::Xbgr8888 => "XBGR8888".into(),
        DrmFourcc::Rgb565 => "RGB565".into(),
        other => {
            let raw = (other as u32).to_le_bytes();
            raw.iter()
                .map(|b| (*b as char).to_ascii_uppercase())
                .collect()
        }
    }
}

struct VblankState {
    vblanks: u64,
}

fn make_tex_gl(
    r: &mut GlesRenderer,
) -> Result<
    smithay::backend::renderer::element::texture::TextureBuffer<
        smithay::backend::renderer::gles::GlesTexture,
    >,
    SessionError,
> {
    use smithay::backend::renderer::element::texture::TextureBuffer;
    let data = [0xD0u8, 0x8A, 0x5A, 0xFF].repeat(64 * 64); // premultiplied ABGR? see texture.rs
    TextureBuffer::from_memory(
        r,
        &data,
        DrmFourcc::Abgr8888,
        smithay::utils::Size::<i32, smithay::utils::Buffer>::from((64, 64)),
        false,
        1,
        Transform::Normal,
        None,
    )
    .map_err(|e| SessionError::Session(format!("tex: {e}")))
}

fn make_tex_soft(
    r: &mut PixmanRenderer,
) -> Result<
    smithay::backend::renderer::element::texture::TextureBuffer<
        smithay::backend::renderer::pixman::PixmanTexture,
    >,
    SessionError,
> {
    use smithay::backend::renderer::element::texture::TextureBuffer;
    let data = [0xD0u8, 0x8A, 0x5A, 0xFF].repeat(64 * 64);
    TextureBuffer::from_memory(
        r,
        &data,
        DrmFourcc::Abgr8888,
        smithay::utils::Size::<i32, smithay::utils::Buffer>::from((64, 64)),
        false,
        1,
        Transform::Normal,
        None,
    )
    .map_err(|e| SessionError::Session(format!("tex: {e}")))
}

fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn round3_keeps_three_decimals() {
        assert_eq!(round3(16.66666), 16.667);
        assert_eq!(round3(16.6), 16.6);
        assert_eq!(round3(0.0004), 0.0);
        // .round() is half away from zero.
        assert_eq!(round3(-1.2345), -1.235);
    }

    #[test]
    fn fourcc_name_covers_known_and_fallback_formats() {
        assert_eq!(fourcc_name(DrmFourcc::Argb8888), "ARGB8888");
        assert_eq!(fourcc_name(DrmFourcc::Xrgb8888), "XRGB8888");
        assert_eq!(fourcc_name(DrmFourcc::Abgr8888), "ABGR8888");
        assert_eq!(fourcc_name(DrmFourcc::Xbgr8888), "XBGR8888");
        assert_eq!(fourcc_name(DrmFourcc::Rgb565), "RGB565");
        // Unknown codes fall back to the raw fourcc characters.
        assert_eq!(fourcc_name(DrmFourcc::Yuyv), "YUYV");
    }

    /// M0-T3: `probe --json` is a machine-readable contract — pin the keys
    /// tooling can rely on.
    #[test]
    fn probe_report_json_shape_is_stable() {
        let report = ProbeReport {
            frame_interval_ms: 16.667,
            drm: DrmReport {
                connector: "DSI-1".into(),
                mode: "720x1600".into(),
                refresh_hz: 60.0,
                atomic: true,
                planes: vec![PlaneReport {
                    name: "plane0".into(),
                    type_: "primary".into(),
                    formats: vec!["ARGB8888".into()],
                }],
                formats: vec!["ARGB8888".into(), "XRGB8888".into()],
            },
            gl: RendererReport {
                ok: false,
                renderer: None,
                egl_version: None,
                formats: None,
                error: Some("egl: none".into()),
                frames: None,
            },
            soft: RendererReport {
                ok: true,
                renderer: Some("pixman".into()),
                egl_version: None,
                formats: Some(vec!["ARGB8888".into()]),
                error: None,
                frames: Some(PROBE_FRAMES),
            },
            errors: vec![],
        };
        let v = serde_json::to_value(&report).unwrap();
        assert_eq!(v["drm"]["connector"], json!("DSI-1"));
        assert_eq!(v["drm"]["atomic"], json!(true));
        assert_eq!(v["drm"]["planes"][0]["type"], json!("primary"));
        assert_eq!(v["gl"]["ok"], json!(false));
        // skip_serializing_if: absent fields must not appear at all.
        assert!(v["gl"].get("renderer").is_none());
        assert!(v["gl"].get("frames").is_none());
        assert_eq!(v["gl"]["error"], json!("egl: none"));
        assert_eq!(v["soft"]["frames"], json!(PROBE_FRAMES));
        assert!(v["soft"].get("error").is_none());
        assert!(v.get("errors").is_some());
        assert!(v.get("frame_interval_ms").is_some());
    }
}
