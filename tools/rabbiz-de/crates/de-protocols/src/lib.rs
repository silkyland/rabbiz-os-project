//! `de-protocols` — Wayland protocol setup helpers for `rabbiz-de` (M1).
//!
//! The handler impls live in `de-core` (they must be implemented on the
//! compositor state type itself); this crate holds the reusable, stateless
//! pieces: advertised formats, dmabuf feedback construction and the
//! fullscreen/SSD policies.

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
#[must_use]
pub fn dmabuf_feedback(render_dev: DrmNode) -> Option<DmabufFeedback> {
    DmabufFeedbackBuilder::new(render_dev.dev_id(), dmabuf_formats())
        .build()
        .ok()
}

/// Handset toplevel state: fullscreen only — no maximized/resizing chrome.
pub const HANDSET_TOPLEVEL_STATE: xdg_toplevel::State = xdg_toplevel::State::Fullscreen;

/// xdg-decoration answer: always server-side (handset draws no CSD/SSD).
pub const DECORATION_MODE: zxdg_toplevel_decoration_v1::Mode =
    zxdg_toplevel_decoration_v1::Mode::ServerSide;

/// Handset policy: every xdg toplevel is fullscreen 720×1600 (no chrome —
/// decorations are declared server-side and drawn as nothing, M1-T2).
pub fn configure_fullscreen(surface: &ToplevelSurface, size: Size<i32, smithay::utils::Logical>) {
    surface.with_pending_state(|state| {
        state.states.set(HANDSET_TOPLEVEL_STATE);
        state.size = Some(size);
        state.bounds = Some(size);
    });
    // SurfaceState fields (fullscreen_output) are set by callers that hold
    // the output; the configure is sent lazily at first commit.
}

/// Desktop/Tablet policy: configure a floating, resizable window without
/// Fullscreen/Maximized states.
pub fn configure_floating(surface: &ToplevelSurface, size: Size<i32, smithay::utils::Logical>) {
    surface.with_pending_state(|state| {
        state.states.unset(xdg_toplevel::State::Fullscreen);
        state.states.unset(xdg_toplevel::State::Maximized);
        state.states.set(xdg_toplevel::State::Activated);
        state.size = Some(size);
        state.bounds = Some(size);
    });
}

/// Desktop/Tablet policy: configure a maximized window filling the work area.
pub fn configure_maximized(surface: &ToplevelSurface, size: Size<i32, smithay::utils::Logical>) {
    surface.with_pending_state(|state| {
        state.states.unset(xdg_toplevel::State::Fullscreen);
        state.states.set(xdg_toplevel::State::Maximized);
        state.states.set(xdg_toplevel::State::Activated);
        state.size = Some(size);
        state.bounds = Some(size);
    });
}

/// Compute initial `(x, y, width, height)` for the `index`-th floating window
/// in Desktop mode so multiple windows cascade cleanly inside `output`.
#[must_use]
pub fn desktop_window_geometry(
    output: Size<i32, smithay::utils::Logical>,
    index: usize,
) -> (i32, i32, Size<i32, smithay::utils::Logical>) {
    let margin_top = 64;
    let margin_bottom = 64;
    let avail_w = (output.w - 64).max(320);
    let avail_h = (output.h - margin_top - margin_bottom).max(240);
    let w = ((output.w * 65) / 100).clamp(320, avail_w.min(1040));
    let h = ((avail_h * 84) / 100).clamp(240, avail_h.min(720));
    let step = (index % 5) as i32;
    let max_x = (output.w - w - 24).max(24);
    let max_y = (output.h - margin_bottom - h).max(margin_top);
    let x = (((output.w - w) / 2) + step * 36).clamp(24, max_x);
    let y = (margin_top + 16 + step * 28).clamp(margin_top, max_y);
    (x, y, (w, h).into())
}

/// Apply [`DECORATION_MODE`] to a toplevel.
pub fn set_ssd(surface: &ToplevelSurface) {
    surface.with_pending_state(|state| {
        state.decoration_mode = Some(DECORATION_MODE);
    });
}

pub mod layer_shell;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shm_formats_advertise_argb_xrgb_only() {
        use smithay::reexports::wayland_server::protocol::wl_shm::Format;
        assert_eq!(shm_formats(), vec![Format::Argb8888, Format::Xrgb8888]);
    }

    #[test]
    fn dmabuf_formats_are_linear_argb_xrgb_only() {
        let fmts = dmabuf_formats();
        assert_eq!(fmts.len(), 2);
        for f in &fmts {
            assert_eq!(f.modifier, Modifier::Linear);
        }
        let codes: Vec<_> = fmts.iter().map(|f| f.code).collect();
        assert_eq!(codes, vec![Fourcc::Argb8888, Fourcc::Xrgb8888]);
    }

    /// M1-T2: the shell claims server-side decorations and configures every
    /// toplevel fullscreen against the output bounds — the tests pin the two
    /// policy values the handlers apply.
    #[test]
    fn handset_policy_is_fullscreen_with_server_side_decoration() {
        assert_eq!(HANDSET_TOPLEVEL_STATE, xdg_toplevel::State::Fullscreen);
        assert_eq!(
            DECORATION_MODE,
            zxdg_toplevel_decoration_v1::Mode::ServerSide
        );
    }

    #[test]
    fn desktop_window_geometry_cascades_within_landscape_output() {
        let out: Size<i32, smithay::utils::Logical> = (1600, 720).into();
        let (x0, y0, s0) = desktop_window_geometry(out, 0);
        let (x1, y1, s1) = desktop_window_geometry(out, 1);
        assert_eq!(s0, s1);
        assert!(s0.w >= 320 && s0.w <= 1040);
        assert!(s0.h >= 240 && s0.h <= 720);
        assert!(x1 > x0 && y1 > y0);
        assert!(x1 + s1.w <= out.w);
        assert!(y1 + s1.h <= out.h);
    }
}
