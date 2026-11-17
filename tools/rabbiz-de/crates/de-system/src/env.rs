//! The desktop contract environment — the single authority (DE-PLAN M1-T7).
//!
//! `rabbiz-de env --shell` prints this contract as `KEY='value'` and
//! `unset KEY` lines; `a21s-session` evals them before starting the
//! compositor, and `rabbiz-de launch <desktop-id>` applies the same table to
//! every spawned app. Nothing else may duplicate it.

/// Variables every session app inherits.
pub const DESKTOP_CONTRACT_ENV: &[(&str, &str)] = &[
    // The CMA/CMA-import workaround: Qt Quick and our own wrappers stay
    // software unless a verified opt-in wrapper (a21s-qt-gpu) overrides it.
    ("A21S_FORCE_SOFTWARE", "1"),
    ("QT_QPA_PLATFORM", "wayland"),
    ("QT_QUICK_BACKEND", "software"),
    ("QT_SCALE_FACTOR", "1"),
    // Session identity for portals, toolkits and the OSK.
    ("XDG_CURRENT_DESKTOP", "Rabbiz"),
    ("XDG_SESSION_DESKTOP", "rabbiz-de"),
    ("XDG_SESSION_TYPE", "wayland"),
    // Seat keymap: English + Thai, Alt+Shift toggles.
    ("XKB_DEFAULT_LAYOUT", "us,th"),
    ("XKB_DEFAULT_OPTIONS", "grp:alt_shift_toggle"),
    // Wayland for the common non-Qt toolkits, same as the labwc launcher.
    ("MOZ_ENABLE_WAYLAND", "1"),
    ("SDL_VIDEODRIVER", "wayland"),
];

/// Legacy wlroots/Mesa/toolkit overrides that must NOT leak into the session.
/// Exact names only — prefixes live in [`DESKTOP_CONTRACT_UNSET_PREFIXES`].
pub const DESKTOP_CONTRACT_UNSET: &[&str] = &[
    "LD_PRELOAD",
    "DRI_PRIME",
    "__EGL_VENDOR_LIBRARY_FILENAMES",
    "QSG_RHI_BACKEND",
    "MESA_LOADER_DRIVER_OVERRIDE",
    "QT_QPA_PLATFORMTHEME",
];

/// Variable-name prefixes swept by `unset $(compgen -v PREFIX)` — catches
/// every `WLR_*` (labwc remnants) and `RABBIZ_EXYNOS_*` (scanout experiment
/// overrides) without enumerating them.
pub const DESKTOP_CONTRACT_UNSET_PREFIXES: &[&str] = &["WLR_", "RABBIZ_EXYNOS_"];

/// Format the desktop contract as `KEY='value'` plus `unset` lines, ready for
/// `eval "$(rabbiz-de env --shell)"` in a POSIX shell.
pub fn format_shell_env() -> String {
    let mut out = String::new();
    for (k, v) in DESKTOP_CONTRACT_ENV {
        out.push_str(&format!("{k}='{}'\n", v.replace('\'', "'\\''")));
    }
    for k in DESKTOP_CONTRACT_UNSET {
        out.push_str(&format!("unset {k}\n"));
    }
    for prefix in DESKTOP_CONTRACT_UNSET_PREFIXES {
        out.push_str(&format!("unset $(compgen -v {prefix} || true)\n"));
    }
    out
}

/// Variable names (exact + expanded prefixes) the launcher must remove from a
/// spawned child's environment. Used by `rabbiz-de launch`.
pub fn unset_names() -> Vec<String> {
    let mut names: Vec<String> = DESKTOP_CONTRACT_UNSET
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    for (key, _) in std::env::vars() {
        if DESKTOP_CONTRACT_UNSET_PREFIXES
            .iter()
            .any(|p| key.starts_with(p))
            && !names.contains(&key)
        {
            names.push(key);
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_sets_force_software_and_scale_factor_1() {
        let map: std::collections::BTreeMap<_, _> = DESKTOP_CONTRACT_ENV.iter().copied().collect();
        assert_eq!(map["A21S_FORCE_SOFTWARE"], "1");
        assert_eq!(map["QT_SCALE_FACTOR"], "1");
        assert_eq!(map["QT_QPA_PLATFORM"], "wayland");
        assert_eq!(map["XKB_DEFAULT_LAYOUT"], "us,th");
        assert_eq!(map["XKB_DEFAULT_OPTIONS"], "grp:alt_shift_toggle");
        assert_eq!(map["XDG_CURRENT_DESKTOP"], "Rabbiz");
        assert_eq!(map["XDG_SESSION_DESKTOP"], "rabbiz-de");
        assert_eq!(map["XDG_SESSION_TYPE"], "wayland");
    }

    #[test]
    fn contract_unsets_mesa_wlroots_and_rhi_overrides() {
        for name in [
            "LD_PRELOAD",
            "DRI_PRIME",
            "__EGL_VENDOR_LIBRARY_FILENAMES",
            "QSG_RHI_BACKEND",
        ] {
            assert!(
                DESKTOP_CONTRACT_UNSET.contains(&name),
                "missing unset {name}"
            );
        }
        assert!(DESKTOP_CONTRACT_UNSET_PREFIXES.contains(&"RABBIZ_EXYNOS_"));
        assert!(DESKTOP_CONTRACT_UNSET_PREFIXES.contains(&"WLR_"));
    }

    #[test]
    fn shell_output_is_quotable_lines_plus_unsets() {
        let out = format_shell_env();
        assert!(out.contains("A21S_FORCE_SOFTWARE='1'\n"));
        assert!(out.contains("QT_SCALE_FACTOR='1'\n"));
        assert!(out.contains("unset LD_PRELOAD\n"));
        assert!(out.contains("unset QSG_RHI_BACKEND\n"));
        assert!(out.contains("unset $(compgen -v RABBIZ_EXYNOS_ || true)\n"));
        assert!(out.contains("unset $(compgen -v WLR_ || true)\n"));
    }

    #[test]
    fn unset_names_covers_exact_and_prefix_matches() {
        std::env::set_var("RABBIZ_EXYNOS_TEST_SENTINEL", "1");
        let names = unset_names();
        assert!(names.contains(&"LD_PRELOAD".to_string()));
        assert!(names.contains(&"RABBIZ_EXYNOS_TEST_SENTINEL".to_string()));
        std::env::remove_var("RABBIZ_EXYNOS_TEST_SENTINEL");
    }
}
