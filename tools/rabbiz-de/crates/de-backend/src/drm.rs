//! DRM/GBM/EGL bring-up for the exynos card (M0-T3, spike S1).
//!
//! One card, one connector, one CRTC, one mode — the A21s handset has exactly
//! one DSI panel. Multi-output comes never in v1.

use std::os::unix::io::OwnedFd;
use std::path::{Path, PathBuf};

use smithay::backend::allocator::gbm::{GbmAllocator, GbmBufferFlags, GbmDevice};
use smithay::backend::drm::exporter::gbm::GbmFramebufferExporter;
use smithay::backend::drm::{DrmDevice, DrmDeviceFd, DrmDeviceNotifier};
use smithay::backend::egl::{EGLDevice, EGLDisplay};
use smithay::backend::session::libseat::LibSeatSession;
use smithay::reexports::drm::control::{connector, crtc, Device as _, ModeTypeFlags};
use smithay::reexports::drm::{self};
use smithay::utils::DeviceFd;

use crate::session::SessionError;

/// Preferred device path on the A21s (stable, survives card renumbering).
pub const EXYNOS_CARD_BY_PATH: &str = "/dev/dri/by-path/platform-exynos-drm-card";

/// A connected connector + chosen CRTC + mode.
#[derive(Debug)]
pub struct OutputSelection {
    pub connector_name: String,
    pub connector: connector::Handle,
    pub crtc: crtc::Handle,
    pub mode: drm::control::Mode,
    /// Connector width in px.
    pub width: u32,
    /// Connector height in px.
    pub height: u32,
    /// Vertical refresh in Hz (mode clock-derived).
    pub refresh_hz: f64,
}

/// Everything opened for one DRM card.
pub struct DrmSetup {
    pub device: DrmDevice,
    /// Taken once by the event loop that owns frame pacing.
    pub notifier: Option<DrmDeviceNotifier>,
    pub gbm: GbmDevice<DrmDeviceFd>,
    pub fd: DrmDeviceFd,
    /// EGL display bound to the GBM device (GL path only).
    pub egl_display: Option<EGLDisplay>,
    /// GL render node (may differ from the display node).
    pub render_node: Option<smithay::backend::drm::DrmNode>,
    /// GL_RENDERER string when EGL is up.
    pub gl_renderer: Option<String>,
    pub egl_version: Option<String>,
}

/// Find the DRM card path: prefer the exynos by-path symlink, else the first
/// `cardN` that exists.
pub fn find_card() -> Option<PathBuf> {
    let by_path = PathBuf::from(EXYNOS_CARD_BY_PATH);
    if by_path.exists() {
        return Some(by_path);
    }
    for n in 0..8u8 {
        let p = PathBuf::from(format!("/dev/dri/card{n}"));
        if p.exists() {
            return Some(p);
        }
    }
    None
}

/// Open the card through the session and create `DrmDevice` + `GbmDevice`.
pub fn open_drm(session: &mut LibSeatSession, path: &Path) -> Result<DrmSetup, SessionError> {
    let fd = crate::session::open_device(session, path)?;
    init_from_fd(fd, path)
}

/// Initialize from an already-open fd (used by tests / direct-open fallback).
pub fn init_from_fd(fd: OwnedFd, _path: &Path) -> Result<DrmSetup, SessionError> {
    let drm_fd = DrmDeviceFd::new(DeviceFd::from(fd));
    let (device, notifier) = DrmDevice::new(drm_fd.clone(), false)
        .map_err(|e| SessionError::Session(format!("drm: {e}")))?;
    let gbm =
        GbmDevice::new(drm_fd.clone()).map_err(|e| SessionError::Session(format!("gbm: {e}")))?;
    Ok(DrmSetup {
        device,
        notifier: Some(notifier),
        gbm,
        fd: drm_fd,
        egl_display: None,
        render_node: None,
        gl_renderer: None,
        egl_version: None,
    })
}

/// Bring up EGL on the GBM device (GL renderer prerequisite).
///
/// On success fills `egl_display`, `render_node`, `gl_renderer`,
/// `egl_version`. Failure leaves them `None` and returns `Err`.
pub fn init_egl(setup: &mut DrmSetup) -> Result<(), SessionError> {
    // SAFETY: display is created from a live GBM device owned by this setup.
    let display = unsafe {
        EGLDisplay::new(setup.gbm.clone())
            .map_err(|e| SessionError::Session(format!("egl: {e}")))?
    };
    let egl_device = EGLDevice::device_for_display(&display)
        .map_err(|e| SessionError::Session(format!("egl device: {e}")))?;
    let render_node = egl_device.try_get_render_node().ok().flatten();
    let (major, minor) = display.get_egl_version();
    setup.egl_version = Some(format!("{major}.{minor}"));
    setup.render_node = render_node;
    setup.egl_display = Some(display);
    Ok(())
}

/// Enumerate connectors and pick the first connected one with a mode, plus a
/// compatible CRTC. Prefers the PREFERRED mode, else the first listed.
pub fn select_output(device: &mut DrmDevice) -> Result<Option<OutputSelection>, SessionError> {
    let res = device
        .resource_handles()
        .map_err(|e| SessionError::Session(format!("res handles: {e}")))?;
    for conn_handle in &res.connectors {
        let Ok(conn) = device.get_connector(*conn_handle, true) else {
            continue;
        };
        if conn.state() != connector::State::Connected {
            continue;
        }
        let modes = conn.modes();
        if modes.is_empty() {
            continue;
        }
        let mode = modes
            .iter()
            .copied()
            .find(|m| m.mode_type().contains(ModeTypeFlags::PREFERRED))
            .unwrap_or(modes[0]);

        // connector -> encoder -> possible_crtcs
        let crtc =
            conn.current_encoder()
                .and_then(|e| device.get_encoder(e).ok())
                .and_then(|enc| res.filter_crtcs(enc.possible_crtcs()).into_iter().next())
                .or_else(|| {
                    conn.encoders().iter().find_map(|e| {
                        device.get_encoder(*e).ok().and_then(|enc| {
                            res.filter_crtcs(enc.possible_crtcs()).into_iter().next()
                        })
                    })
                });
        let Some(crtc) = crtc else { continue };

        let name = format!("{}-{}", conn.interface().as_str(), conn.interface_id());
        let (w, h) = mode.size();
        // vrefresh() returns integer Hz; derive more precisely from clock.
        let refresh = mode.vrefresh() as f64;
        return Ok(Some(OutputSelection {
            connector_name: name,
            connector: *conn_handle,
            crtc,
            mode,
            width: w as u32,
            height: h as u32,
            refresh_hz: refresh,
        }));
    }
    Ok(None)
}

/// Build the GBM allocator + framebuffer exporter used by `DrmCompositor`.
pub fn allocators(
    setup: &DrmSetup,
) -> (
    GbmAllocator<DrmDeviceFd>,
    GbmFramebufferExporter<DrmDeviceFd>,
) {
    let allocator = GbmAllocator::new(
        setup.gbm.clone(),
        GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT,
    );
    let exporter = GbmFramebufferExporter::new(setup.gbm.clone(), setup.render_node);
    (allocator, exporter)
}
