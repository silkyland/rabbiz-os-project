//! Screen scenes. No I/O, no renderer, no OS: a screen returns a [`Scene`].
//!
//! A screen module is a pure function of the output size (and later the shell
//! state). Registration is explicit so the host simulator and the compositor
//! agree on the scene vocabulary.

use de_api::{Scene, Size};

pub mod app_window;
pub mod boot;
pub mod drawer;
pub mod error;
pub mod first_run;
pub mod home;
pub mod i18n;
pub mod launch;
pub mod lock;
pub mod osd;
pub mod power_menu;
pub mod quick_settings;
pub mod recents;
pub mod screenshot;
pub mod search;
pub mod settings;
pub mod shade;
pub mod shell_stub;
pub mod status;
pub mod toast;

pub const SCENES: &[&str] = &[
    "brand",
    "lock",
    "home",
    "drawer",
    "recents",
    "status",
    "shade",
    "quick-settings",
    "power-menu",
    "volume",
    "brightness",
    "launch",
    "app-window",
    "search",
    "settings",
    "settings-network",
    "settings-bluetooth",
    "settings-display",
    "settings-sound",
    "settings-about",
    "settings-session",
    "screenshot",
    "error",
    "first-run",
    "toast",
    "shell-stub",
];

pub fn boot_frame(size: Size) -> Scene {
    boot::build(size)
}

/// Resolve a scene name from the CLI or a future route table.
pub fn scene(name: &str, size: Size) -> Option<Scene> {
    match name {
        "brand" => Some(boot::build(size)),
        "lock" => Some(lock::build(size)),
        "home" => Some(home::build(size)),
        "drawer" => Some(drawer::build(size)),
        "recents" => Some(recents::build(size)),
        "status" => Some(status::build(size)),
        "shade" => Some(shade::build(size)),
        "quick-settings" => Some(quick_settings::build(size)),
        "power-menu" => Some(power_menu::build(size)),
        "volume" => Some(osd::volume(size)),
        "brightness" => Some(osd::brightness(size)),
        "launch" => Some(launch::build(size)),
        "app-window" => Some(app_window::build(size)),
        "search" => Some(search::build(size)),
        "settings" => Some(settings::index(size)),
        "settings-network" => Some(settings::network(size)),
        "settings-bluetooth" => Some(settings::bluetooth(size)),
        "settings-display" => Some(settings::display(size)),
        "settings-sound" => Some(settings::sound(size)),
        "settings-about" => Some(settings::about(size)),
        "settings-session" => Some(settings::session(size)),
        "screenshot" => Some(screenshot::build(size)),
        "error" => Some(error::build(size)),
        "first-run" => Some(first_run::build(size)),
        "toast" => Some(toast::build(size)),
        "shell-stub" => Some(shell_stub::build(size, "09:41", "Tue, Nov 17")),
        _ => None,
    }
}
