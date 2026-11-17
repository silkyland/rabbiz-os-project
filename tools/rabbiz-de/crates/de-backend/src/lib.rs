//! `de-backend` — libseat session, DRM/GBM/EGL bring-up, libinput.
//!
//! Linux-only crate (smithay). `probe` is the M0-T3 spike S1.

pub mod drm;
pub mod libinput;
pub mod probe;
pub mod session;
