//! Presentation policy (M1-T4): which `FrameFlags` the compositor asks for
//! and how each submitted frame is counted.
//!
//! The flags only *allow* smithay to try; whether direct scanout happened is
//! observed per-frame from `Frame::primary_element` via `de_render::scanout`.

use smithay::backend::drm::compositor::FrameFlags;

/// Frame flags for the session output.
///
/// `ALLOW_PRIMARY_PLANE_SCANOUT` is the minimal flag for the M1 goal — a
/// fullscreen dmabuf client may scan out directly on the primary plane.
/// Overlay/cursor planes are not enabled in M1 (exynos plane budget is tight
/// and the shell chrome can't produce plane buffers anyway).
pub const PRESENT_FLAGS: FrameFlags = FrameFlags::ALLOW_PRIMARY_PLANE_SCANOUT;
