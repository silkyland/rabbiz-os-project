//! The app environment contract — the single authority for `rabbiz-de env
//! --shell` (§7.20). The `rabbiz-session` wrapper evaluates these lines; the
//! contract is also what `de-apps::launch` applies to spawned apps.

/// `KEY=VALUE` exports (single-quoted values, shell-safe).
const EXPORTS: &[(&str, &str)] = &[
    ("QT_QUICK_BACKEND", "software"),
    ("QT_QPA_PLATFORM", "wayland"),
    // Apps render software; wrappers may bump their own scale factor.
    ("RABBIZ_FORCE_SOFTWARE", "1"),
    ("QT_SCALE_FACTOR", "1"),
    ("XDG_CURRENT_DESKTOP", "Rabbiz"),
    ("XDG_SESSION_DESKTOP", "rabbiz-de"),
    ("XDG_SESSION_TYPE", "wayland"),
    ("XKB_DEFAULT_LAYOUT", "us,th"),
    ("XKB_DEFAULT_OPTIONS", "grp:alt_shift_toggle"),
    ("WAYLAND_DISPLAY", "wayland-0"),
    // Wayland for the common non-Qt toolkits, same as the labwc launcher.
    ("MOZ_ENABLE_WAYLAND", "1"),
    ("SDL_VIDEODRIVER", "wayland"),
    // Mobile DPI scaling for 3rd-party apps on the 720x1600 handset display
    // (720 / 1.5 = 480 dp), plus native Wayland for Electron/Chromium apps.
    ("GDK_DPI_SCALE", "1.5"),
    ("ELECTRON_OZONE_PLATFORM_HINT", "wayland"),
];

/// Variables cleared before the session starts (wlroots/Mesa GPU overrides
/// must not leak into the DE or its apps).
const UNSETS: &[&str] = &[
    "LD_PRELOAD",
    "DRI_PRIME",
    "__EGL_VENDOR_LIBRARY_FILENAMES",
    "QSG_RHI_BACKEND",
    "MESA_LOADER_DRIVER_OVERRIDE",
    "QT_QPA_PLATFORMTHEME",
    "WLR_RENDERER",
    "WLR_BACKENDS",
    "WLR_DRM_DEVICES",
    "RABBIZ_EXYNOS_SCANOUT",
];

/// Variable-name prefixes swept by `unset $(compgen -v PREFIX)` — catches
/// every `WLR_*` (labwc remnants) and `RABBIZ_EXYNOS_*` (scanout experiment
/// overrides) without enumerating them.
const UNSET_PREFIXES: &[&str] = &["WLR_", "RABBIZ_EXYNOS_"];

/// Print the contract as `export KEY='VALUE'` / `unset KEY` lines for `eval`.
/// The `export` is load-bearing: a bare `KEY='value'` creates an unexported
/// shell variable that never reaches `rabbiz-de run`, its spawned apps or
/// `dbus-update-activation-environment`.
#[must_use]
pub fn shell_lines() -> String {
    let mut out = String::new();
    for (k, v) in EXPORTS {
        out.push_str("export ");
        out.push_str(k);
        out.push_str("='");
        out.push_str(&v.replace('\'', "'\\''"));
        out.push_str("'\n");
    }
    for k in UNSETS {
        out.push_str("unset ");
        out.push_str(k);
        out.push('\n');
    }
    for prefix in UNSET_PREFIXES {
        out.push_str("unset $(compgen -v ");
        out.push_str(prefix);
        out.push_str(" || true)\n");
    }
    out
}

/// Variable names (exact + expanded prefixes) the launcher must remove from a
/// spawned child's environment. Used by `de-apps::launch`.
#[must_use]
pub fn unset_names() -> Vec<String> {
    let mut names: Vec<String> = UNSETS.iter().map(|s| (*s).to_string()).collect();
    for (key, _) in std::env::vars() {
        if UNSET_PREFIXES.iter().any(|p| key.starts_with(p)) && !names.contains(&key) {
            names.push(key);
        }
    }
    names
}

/// The same contract as a `Vec<(key, Option<value>)>` for `Command::env`
/// (`None` = remove). Exact unsets only; prefix expansion lives in
/// [`unset_names`].
#[must_use]
pub fn app_env() -> Vec<(&'static str, Option<&'static str>)> {
    let mut v: Vec<_> = EXPORTS.iter().map(|(k, v)| (*k, Some(*v))).collect();
    v.extend(UNSETS.iter().map(|k| (*k, None)));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_sets_force_software_and_session_vars() {
        let map: std::collections::BTreeMap<_, _> = EXPORTS.iter().copied().collect();
        assert_eq!(map["RABBIZ_FORCE_SOFTWARE"], "1");
        assert_eq!(map["QT_SCALE_FACTOR"], "1");
        assert_eq!(map["QT_QPA_PLATFORM"], "wayland");
        assert_eq!(map["XKB_DEFAULT_LAYOUT"], "us,th");
        assert_eq!(map["XKB_DEFAULT_OPTIONS"], "grp:alt_shift_toggle");
        assert_eq!(map["XDG_CURRENT_DESKTOP"], "Rabbiz");
        assert_eq!(map["XDG_SESSION_DESKTOP"], "rabbiz-de");
        assert_eq!(map["XDG_SESSION_TYPE"], "wayland");
        assert_eq!(map["MOZ_ENABLE_WAYLAND"], "1");
        assert_eq!(map["SDL_VIDEODRIVER"], "wayland");
    }

    #[test]
    fn contract_unsets_mesa_wlroots_and_rhi_overrides() {
        for name in [
            "LD_PRELOAD",
            "DRI_PRIME",
            "__EGL_VENDOR_LIBRARY_FILENAMES",
            "QSG_RHI_BACKEND",
        ] {
            assert!(UNSETS.contains(&name), "missing unset {name}");
        }
        assert!(UNSET_PREFIXES.contains(&"RABBIZ_EXYNOS_"));
        assert!(UNSET_PREFIXES.contains(&"WLR_"));
    }

    #[test]
    fn shell_output_exports_vars_and_unsets_overrides() {
        let out = shell_lines();
        // `export` is required: eval'd `KEY='v'` alone stays a shell-local var.
        assert!(out.contains("export RABBIZ_FORCE_SOFTWARE='1'\n"));
        assert!(out.contains("export XDG_CURRENT_DESKTOP='Rabbiz'\n"));
        assert!(out.contains("unset LD_PRELOAD\n"));
        assert!(out.contains("unset $(compgen -v WLR_ || true)\n"));
        assert!(out.contains("unset $(compgen -v RABBIZ_EXYNOS_ || true)\n"));
    }
}
