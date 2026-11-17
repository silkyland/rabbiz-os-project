//! `de-protocols` — Wayland protocol setup helpers for `rabbiz-de` (M1).
//!
//! The handler impls live in `de-core` (they must be implemented on the
//! compositor state type itself); this crate holds the reusable, stateless
//! pieces: advertised formats, dmabuf feedback construction and the
//! fullscreen/SSD policies.

use smithay::backend::allocator::dmabuf::Dmabuf;
use smithay::backend::allocator::{Format, Fourcc, Modifier};
use smithay::backend::drm::DrmNode;
use smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1;
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;
use smithay::utils::Size;
use smithay::wayland::dmabuf::{DmabufFeedback, DmabufFeedbackBuilder};
use smithay::wayland::shell::xdg::ToplevelSurface;

/// `wl_shm` formats we advertise to clients (plan: ARGB8888/XRGB8888 only).
#[must_use]
pub fn shm_formats() -> Vec<smithay::reexports::wayland_server::protocol::wl_shm::Format> {
    use smithay::reexports::wayland_server::protocol::wl_shm::Format;
    vec![Format::Argb8888, Format::Xrgb8888]
}

/// Linear-only dmabuf formats (plan M1-T3: ARGB8888/XRGB8888 + Linear).
#[must_use]
pub fn dmabuf_formats() -> Vec<Format> {
    vec![
        Format {
            code: Fourcc::Argb8888,
            modifier: Modifier::Linear,
        },
        Format {
            code: Fourcc::Xrgb8888,
            modifier: Modifier::Linear,
        },
    ]
}

/// Default dmabuf feedback for the render device (v4 `get_default_feedback`).
pub fn dmabuf_feedback(render_dev: DrmNode) -> Option<DmabufFeedback> {
    DmabufFeedbackBuilder::new(render_dev.dev_id(), dmabuf_formats())
        .build()
        .ok()
}

/// Handset policy: every xdg toplevel is fullscreen 720×1600 (no chrome —
/// decorations are declared server-side and drawn as nothing, M1-T2).
pub fn configure_fullscreen(surface: &ToplevelSurface, size: Size<i32, smithay::utils::Logical>) {
    surface.with_pending_state(|state| {
        state.states.set(xdg_toplevel::State::Fullscreen);
        state.size = Some(size);
        state.bounds = Some(size);
    });
    // SurfaceState fields (fullscreen_output) are set by callers that hold
    // the output; the configure is sent lazily at first commit.
}

/// xdg-decoration answer: always server-side (handset draws no CSD/SSD).
pub fn set_ssd(surface: &ToplevelSurface) {
    surface.with_pending_state(|state| {
        state.decoration_mode = Some(zxdg_toplevel_decoration_v1::Mode::ServerSide);
    });
}

/// Import a dmabuf buffer for damage bookkeeping — placeholder for the
/// pre-commit blocker path used when dmabuf clients commit (kept tiny for
/// M1: `on_commit_buffer_handler` handles wl_shm correctly already).
pub fn dmabuf_needs_import(_buf: &Dmabuf) -> bool {
    false
}
