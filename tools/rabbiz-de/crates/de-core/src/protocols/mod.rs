//! Wayland protocol handler implementations for `De`.

pub mod comp;
pub(crate) mod idle_inhibit;
pub(crate) mod layer_shell;
pub mod output_dmabuf;
pub(crate) mod pointer_constraints;
pub(crate) mod popups;
pub(crate) mod privileged;
pub mod screencopy;
pub mod seat;
pub mod selection;
pub mod virtual_pointer;
pub mod xdg;
