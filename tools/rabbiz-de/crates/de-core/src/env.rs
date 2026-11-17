/// Desktop contract environment variables exported for all session apps.
pub const DESKTOP_CONTRACT_ENV: &[(&str, &str)] = &[
    ("A21S_FORCE_SOFTWARE", "1"),
    ("QT_QUICK_BACKEND", "software"),
    ("QT_SCALE_FACTOR", "1"),
    ("XDG_CURRENT_DESKTOP", "Rabbiz"),
    ("XDG_SESSION_DESKTOP", "rabbiz-de"),
    ("XDG_SESSION_TYPE", "wayland"),
    ("XKB_DEFAULT_LAYOUT", "us,th"),
];

/// Legacy wlroots/Mesa overrides that must be unset for the session.
pub const DESKTOP_CONTRACT_UNSET: &[&str] = &[
    "MESA_LOADER_DRIVER_OVERRIDE",
    "WLR_RENDERER",
    "WLR_DRM_DEVICES",
    "QT_QPA_PLATFORMTHEME",
];

/// Format the desktop contract as `KEY='VALUE'` and `unset KEY` shell lines.
pub fn format_shell_env() -> String {
    let mut out = String::new();
    for (k, v) in DESKTOP_CONTRACT_ENV {
        out.push_str(&format!("{k}='{}'\n", v.replace('\'', "'\\''")));
    }
    for k in DESKTOP_CONTRACT_UNSET {
        out.push_str(&format!("unset {k}\n"));
    }
    out
}
