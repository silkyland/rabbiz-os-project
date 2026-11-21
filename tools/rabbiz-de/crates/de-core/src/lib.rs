//! `de-core` — compositor state, Wayland protocol handlers, runtime, and shell
//! UI controllers (Linux only).

pub mod protocols;
pub mod runtime;
pub mod shell_ui;

// Re-export module paths at crate root so internal and downstream imports stay concise.
pub use protocols::{comp, output_dmabuf, screencopy, seat, selection, virtual_pointer, xdg};
pub(crate) use protocols::{popups, privileged};
pub use runtime::{composite, frame, input, launch, snapshots, state};
pub(crate) use runtime::{ctl_ipc, cursor, display_stack, haptics, osk, power};
pub use shell_ui::shell;
pub(crate) use shell_ui::{
    home_ui, hun_ui, lock_ui, media_ui, morph, navigation, osd_ui, panel_ui, shell_menu, shot_ui,
};

pub use runtime::frame::{event_loop, Run};
pub use runtime::state::{init, De, DeClientState, InitError};
