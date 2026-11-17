//! The app environment contract — the single authority for `rabbiz-de env
//! --shell` (§7.20). The `a21s-session` wrapper evaluates these lines; the
//! contract is also what `de-apps::launch` applies to spawned apps.

/// `KEY=VALUE` exports (single-quoted values, shell-safe).
const EXPORTS: &[(&str, &str)] = &[
    ("QT_QUICK_BACKEND", "software"),
    ("QT_QPA_PLATFORM", "wayland"),
    // Apps render software; wrappers may bump their own scale factor.
    ("A21S_FORCE_SOFTWARE", "1"),
    ("QT_SCALE_FACTOR", "1"),
    ("XDG_CURRENT_DESKTOP", "Rabbiz"),
    ("XDG_SESSION_DESKTOP", "rabbiz-de"),
    ("XDG_SESSION_TYPE", "wayland"),
    ("XKB_DEFAULT_LAYOUT", "us,th"),
    ("XKB_DEFAULT_OPTIONS", "grp:alt_shift_toggle"),
    ("WAYLAND_DISPLAY", "wayland-0"),
];

/// Variables cleared before the session starts (wlroots/Mesa GPU overrides
/// must not leak into the DE or its apps).
const UNSETS: &[&str] = &[
    "LD_PRELOAD",
    "DRI_PRIME",
    "__EGL_VENDOR_LIBRARY_FILENAMES",
    "QSG_RHI_BACKEND",
    "WLR_RENDERER",
    "WLR_BACKENDS",
    "RABBIZ_EXYNOS_SCANOUT",
];

/// Print the contract as `KEY='VALUE'` / `unset KEY` lines for `eval`.
pub fn shell_lines() -> String {
    let mut out = String::new();
    for (k, v) in EXPORTS {
        out.push_str(&format!("{k}='{v}'\n"));
    }
    for k in UNSETS {
        out.push_str(&format!("unset {k}\n"));
    }
    out
}

/// The same contract as a `Vec<(key, Option<value>)>` for `Command::env`
/// (`None` = remove).
pub fn app_env() -> Vec<(&'static str, Option<&'static str>)> {
    let mut v: Vec<_> = EXPORTS.iter().map(|(k, v)| (*k, Some(*v))).collect();
    v.extend(UNSETS.iter().map(|k| (*k, None)));
    v
}
