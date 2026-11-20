//! DE prefs (M3-T7, Decision 14): the DE owns
//! `~/.config/rabbiz-de/prefs.json` — atomic writes, `.bak` backup, and a
//! one-time read of the legacy `~/.config/rabbiz-shell/prefs.json` for defaults.
//! The legacy file is never written (the QML `ShellPrefs.save()` serialises a
//! fixed key set and would erase unknown keys).

use std::collections::BTreeSet;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Defaults from Decision 14: brand accent `copper`, wallpaper `ember`.
pub const DEFAULT_ACCENT: &str = "copper";
pub const DEFAULT_WALLPAPER: &str = "ember";
pub const DEFAULT_THEME: &str = "light";
pub const DEFAULT_KEYBOARD: &str = "rabbiz-osk";
pub const DEFAULT_LOCK_METHOD: &str = "swipe";
pub const DEFAULT_LOCKSCREEN_NOTIFICATIONS: &str = "show_all";

/// Supported `Prefs::lock_method` values (`docs/DE-KEYGUARD-AUTH.md` §3).
pub const VALID_LOCK_METHODS: &[&str] = &["swipe", "pin", "passphrase", "biometric"];
/// Supported `Prefs::lockscreen_notifications` values (`docs/DE-NOTIFICATIONS-SERVICES.md` §2.4).
pub const VALID_LOCKSCREEN_NOTIFICATIONS: &[&str] = &["show_all", "hide_sensitive", "hide_all"];

fn default_lock_method() -> String {
    DEFAULT_LOCK_METHOD.into()
}

fn default_lockscreen_notifications() -> String {
    DEFAULT_LOCKSCREEN_NOTIFICATIONS.into()
}

/// Shell-owned preferences persisted as flat JSON keys (`deAccent`, ...).
///
/// Unknown keys in the file are ignored on read and dropped on write — the
/// file is ours, so a dropped key is a bug worth seeing loudly in the diff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Prefs {
    /// Accent id (`deAccent`) — resolved by `de-design`.
    #[serde(rename = "deAccent")]
    pub accent: String,
    /// Wallpaper id (`deWallpaperId`) — resolved by `de-paint::wallpaper`.
    #[serde(rename = "deWallpaperId")]
    pub wallpaper_id: String,
    /// `light` | `dark` — mirrored into `~/.config/rabbiz-de/theme` by the
    /// session wrapper; the JSON key exists so prefs survives a mode wipe.
    #[serde(rename = "deTheme")]
    pub theme_mode: String,
    /// Desktop-ids hidden from the drawer/home (M3-T3).
    pub hidden_apps: BTreeSet<String>,
    /// Pinned dock entries, left to right (desktop-ids, ≤ 6).
    pub dock: Vec<String>,
    /// Optional explicit home-grid order (desktop-ids). Unlisted apps follow
    /// in catalog order.
    pub home_order: Vec<String>,
    /// Suppress notification interruptions; history remains available.
    pub do_not_disturb: bool,
    /// Seconds of no input before the screen dims and then sleeps; `0` =
    /// never. Default 600 matches the owner's ~10-minute expectation (P17).
    #[serde(rename = "deScreenTimeoutSec")]
    pub screen_timeout_sec: u32,
    /// Active on-screen keyboard id (`rabbiz-osk` or a 3rd-party keyboard).
    #[serde(rename = "deKeyboard")]
    pub keyboard_id: String,
    /// Whether 3rd-party / market keyboards are permitted in Settings and policy.
    #[serde(rename = "deAllowThirdPartyKeyboards")]
    pub allow_third_party_keyboards: bool,
    /// Keyguard authentication method (`swipe` | `pin` | `passphrase` | `biometric`).
    #[serde(default = "default_lock_method")]
    pub lock_method: String,
    /// Lockscreen notification privacy (`show_all` | `hide_sensitive` | `hide_all`).
    #[serde(default = "default_lockscreen_notifications")]
    pub lockscreen_notifications: String,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            accent: DEFAULT_ACCENT.into(),
            wallpaper_id: DEFAULT_WALLPAPER.into(),
            theme_mode: DEFAULT_THEME.into(),
            hidden_apps: BTreeSet::new(),
            dock: Vec::new(),
            home_order: Vec::new(),
            do_not_disturb: false,
            screen_timeout_sec: 600,
            keyboard_id: DEFAULT_KEYBOARD.into(),
            allow_third_party_keyboards: true,
            lock_method: default_lock_method(),
            lockscreen_notifications: default_lockscreen_notifications(),
        }
    }
}

impl Prefs {
    /// Own file: `$XDG_CONFIG_HOME/rabbiz-de/prefs.json` (or `~/.config`).
    #[must_use]
    pub fn path() -> PathBuf {
        config_home().join("rabbiz-de").join("prefs.json")
    }

    /// Legacy QML shell file — read-only for us.
    #[must_use]
    pub fn legacy_path() -> PathBuf {
        config_home().join("rabbiz-shell").join("prefs.json")
    }

    /// Pre-rename legacy path (`a21s-shell` → `rabbiz-shell`, 2026-11): kept
    /// so a device whose config dir was never migrated still seeds defaults.
    #[must_use]
    fn legacy_a21s_path() -> PathBuf {
        config_home().join("a21s-shell").join("prefs.json")
    }

    /// Load the own file; when absent, seed defaults from the legacy file
    /// (one-time hint read — still never written).
    #[must_use]
    pub fn load() -> Self {
        let own = Self::load_from(&Self::path(), &Self::legacy_path());
        if own == Self::default() {
            return Self::load_from(&Self::path(), &Self::legacy_a21s_path());
        }
        own
    }

    /// Load `own`, falling back to legacy-seeded defaults.
    #[must_use]
    pub fn load_from(own: &std::path::Path, legacy: &std::path::Path) -> Self {
        if let Ok(text) = std::fs::read_to_string(own) {
            if let Ok(p) = Self::from_json(&text) {
                return p;
            }
        }
        Self::defaults_seeded(legacy)
    }

    /// Defaults plus whatever the legacy file offers (wallpaper/accent hints).
    #[must_use]
    pub fn defaults_seeded(legacy: &std::path::Path) -> Self {
        let mut p = Self::default();
        let Ok(text) = std::fs::read_to_string(legacy) else {
            return p;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
            return p;
        };
        // Legacy wallpaper id (aurora|nebula|sunset|emerald|oled). Kept
        // verbatim — `de-paint::wallpaper` falls back to ember when the id
        // has no DE asset, so a stale legacy id is harmless.
        if let Some(w) = v.get("wallpaperId").and_then(|x| x.as_str()) {
            if !w.is_empty() {
                p.wallpaper_id = w.into();
            }
        }
        if let Some(a) = v.get("accentId").and_then(|x| x.as_str()) {
            if !a.is_empty() {
                p.accent = a.into();
            }
        }
        // The QML shell's "Display & Sleep" setting carries over so the
        // device keeps the timeout the user already chose.
        if let Some(secs) = v.get("sleepTimeoutSec").and_then(|x| x.as_u64()) {
            p.screen_timeout_sec = u32::try_from(secs).unwrap_or(u32::MAX);
        }
        // `lightTheme` is deliberately not seeded: the legacy default was
        // dark, the DE default is Cream Glass light (Decision 9) — carrying
        // the old default over would silently undo the owner's choice.

        // Sibling `desktop.json` (QML shell layout state) seeds the dock +
        // home order so the DE opens with the arrangement the user already
        // had. Ids are canonicalized (`a21s-*` → `rabbiz-*`; our retired
        // `google-chrome.desktop` wrapper → `rabbiz-browser`).
        if let Some(dir) = legacy.parent() {
            if let Ok(text) = std::fs::read_to_string(dir.join("desktop.json")) {
                if let Ok(d) = serde_json::from_str::<serde_json::Value>(&text) {
                    let ids = |key: &str| {
                        d.get(key)
                            .and_then(|x| x.as_array())
                            .map(|a| {
                                a.iter()
                                    .filter_map(|x| x.as_str())
                                    .map(normalize_seed_id)
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default()
                    };
                    p.dock = ids("dock");
                    p.home_order = ids("desktop");
                }
            }
        }
        p
    }

    /// Atomic write: `prefs.json.tmp` → rename; previous file → `.bak`.
    pub fn save(&self) -> std::io::Result<()> {
        self.save_to(&Self::path())
    }

    /// Atomic write to `path` (create the parent dir; back up any existing
    /// file to `<name>.bak` first so a bad write never eats the prefs).
    pub fn save_to(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        if path.exists() {
            std::fs::copy(path, path.with_extension("json.bak"))?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, self.to_json())?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    /// Transactionally mutate a clone and commit it to `path`; failed writes
    /// leave `self` unchanged.
    pub fn commit_to<F>(&mut self, path: &std::path::Path, mutate: F) -> bool
    where
        F: FnOnce(&mut Self) -> bool,
    {
        let mut next = self.clone();
        if !mutate(&mut next) {
            return false;
        }
        if next.save_to(path).is_err() {
            return false;
        }
        *self = next;
        true
    }

    /// Choose the wallpaper by id; persists atomically.
    pub fn set_wallpaper(&mut self, id: &str) -> bool {
        self.commit_to(&Self::path(), |p| {
            if p.wallpaper_id == id {
                return false;
            }
            p.wallpaper_id = id.to_string();
            true
        })
    }

    /// Persist Do-Not-Disturb before reflecting it in the UI.
    pub fn set_do_not_disturb(&mut self, on: bool) -> bool {
        self.commit_to(&Self::path(), |p| {
            p.do_not_disturb = on;
            true
        })
    }

    /// Select active keyboard id; persists atomically.
    pub fn set_keyboard(&mut self, id: &str) -> bool {
        self.commit_to(&Self::path(), |p| {
            if p.keyboard_id == id {
                return false;
            }
            p.keyboard_id = id.to_string();
            true
        })
    }

    /// Toggle whether 3rd-party / market keyboards are allowed; persists atomically.
    pub fn set_allow_third_party_keyboards(&mut self, allow: bool) -> bool {
        self.commit_to(&Self::path(), |p| {
            if p.allow_third_party_keyboards == allow {
                return false;
            }
            p.allow_third_party_keyboards = allow;
            if !allow {
                p.keyboard_id = crate::keyboards::DEFAULT_KEYBOARD_ID.into();
            }
            true
        })
    }

    /// Returns `true` when `method` is a valid Keyguard authentication mode.
    #[must_use]
    pub fn is_valid_lock_method(method: &str) -> bool {
        VALID_LOCK_METHODS.contains(&method)
    }

    /// Returns `true` when `mode` is a valid lockscreen notification privacy mode.
    #[must_use]
    pub fn is_valid_lockscreen_notifications(mode: &str) -> bool {
        VALID_LOCKSCREEN_NOTIFICATIONS.contains(&mode)
    }

    /// Validate and atomically persist `lock_method` to `path`; failed
    /// validation or I/O leaves `self` unchanged.
    pub fn set_lock_method_to(
        &mut self,
        path: &std::path::Path,
        method: &str,
    ) -> Result<(), String> {
        if !Self::is_valid_lock_method(method) {
            return Err(format!(
                "invalid lock_method '{method}'; expected one of: {}",
                VALID_LOCK_METHODS.join(", ")
            ));
        }
        if self.lock_method == method {
            return Ok(());
        }
        let mut next = self.clone();
        next.lock_method = method.to_string();
        next.save_to(path)
            .map_err(|e| format!("failed to persist lock_method: {e}"))?;
        *self = next;
        Ok(())
    }

    /// Validate and atomically persist `lock_method` to the default prefs path.
    pub fn set_lock_method(&mut self, method: &str) -> Result<(), String> {
        self.set_lock_method_to(&Self::path(), method)
    }

    /// Validate and atomically persist `lockscreen_notifications` to `path`;
    /// failed validation or I/O leaves `self` unchanged.
    pub fn set_lockscreen_notifications_to(
        &mut self,
        path: &std::path::Path,
        mode: &str,
    ) -> Result<(), String> {
        if !Self::is_valid_lockscreen_notifications(mode) {
            return Err(format!(
                "invalid lockscreen_notifications '{mode}'; expected one of: {}",
                VALID_LOCKSCREEN_NOTIFICATIONS.join(", ")
            ));
        }
        if self.lockscreen_notifications == mode {
            return Ok(());
        }
        let mut next = self.clone();
        next.lockscreen_notifications = mode.to_string();
        next.save_to(path)
            .map_err(|e| format!("failed to persist lockscreen_notifications: {e}"))?;
        *self = next;
        Ok(())
    }

    /// Validate and atomically persist `lockscreen_notifications` to the
    /// default prefs path.
    pub fn set_lockscreen_notifications(&mut self, mode: &str) -> Result<(), String> {
        self.set_lockscreen_notifications_to(&Self::path(), mode)
    }

    /// Serialise (pretty so `git diff`/debugging stays readable).
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into())
    }

    /// Parse; unknown keys ignored, missing keys default.
    pub fn from_json(text: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(text)
    }

    /// Drawer/home visibility.
    #[must_use]
    pub fn is_hidden(&self, desktop_id: &str) -> bool {
        self.hidden_apps.contains(desktop_id)
    }

    /// Toggle hide state; returns the new state.
    pub fn set_hidden(&mut self, desktop_id: &str, hidden: bool) -> bool {
        if hidden {
            self.hidden_apps.insert(desktop_id.into());
        } else {
            self.hidden_apps.remove(desktop_id);
        }
        hidden
    }

    /// Pin `desktop_id` into the dock at the end (≤ 6 entries, no dupes).
    pub fn pin_dock(&mut self, desktop_id: &str) {
        self.dock.retain(|d| d != desktop_id);
        if self.dock.len() < 6 {
            self.dock.push(desktop_id.into());
        }
    }

    /// Remove `desktop_id` from the dock.
    pub fn unpin_dock(&mut self, desktop_id: &str) {
        self.dock.retain(|d| d != desktop_id);
    }

    /// Drop `desktop_id` on the dock at `slot` (a drag rearrange).
    ///
    /// Membership is exclusive: a dock app is not also on the grid. If the
    /// dock is full and `slot` holds another app, that app is swapped out
    /// to the grid at `swap_to` (the dragged icon's old place). `grid` is
    /// the current grid order (ids, dock apps excluded).
    pub fn drop_on_dock(&mut self, desktop_id: &str, slot: usize, swap_to: usize, grid: &[String]) {
        const MAX: usize = 6;
        let was_in_dock = self.dock.iter().any(|d| d == desktop_id);
        let mut dock: Vec<String> = self
            .dock
            .iter()
            .filter(|d| *d != desktop_id)
            .cloned()
            .collect();
        let mut displaced = None;
        if dock.len() >= MAX && !was_in_dock {
            let at = slot.min(dock.len() - 1);
            displaced = Some(dock.remove(at));
            dock.insert(at, desktop_id.into());
        } else {
            dock.insert(slot.min(dock.len()), desktop_id.into());
        }
        self.dock = dock;
        let mut order: Vec<String> = grid.iter().filter(|g| *g != desktop_id).cloned().collect();
        if let Some(d) = displaced {
            order.insert(swap_to.min(order.len()), d);
        }
        self.home_order = order;
    }

    /// Drop `desktop_id` on the grid at global index `index` (a dock app
    /// leaves the dock). `grid` is the current grid order.
    pub fn drop_on_grid(&mut self, desktop_id: &str, index: usize, grid: &[String]) {
        self.unpin_dock(desktop_id);
        let mut order: Vec<String> = grid.iter().filter(|g| *g != desktop_id).cloned().collect();
        order.insert(index.min(order.len()), desktop_id.into());
        self.home_order = order;
    }
}

/// Canonical desktop-id for a seed read from the legacy shell's
/// `desktop.json` (retired `rabbiz-browser`/`a21s-browser` → `google-chrome`;
/// other `a21s-*` → `rabbiz-*`).
fn normalize_seed_id(id: &str) -> String {
    match id {
        "rabbiz-browser" | "a21s-browser" | "com.google.Chrome" => return "google-chrome".into(),
        _ => {}
    }
    if let Some(rest) = id.strip_prefix("a21s-") {
        return format!("rabbiz-{rest}");
    }
    id.into()
}

fn config_home() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(test)]
mod drop_tests {
    use super::*;

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn grid_to_dock_inserts_and_leaves_the_grid() {
        let mut p = Prefs {
            dock: ids(&["a", "b"]),
            ..Prefs::default()
        };
        p.drop_on_dock("x", 1, 0, &ids(&["w", "x", "y"]));
        assert_eq!(p.dock, ids(&["a", "x", "b"]));
        assert_eq!(p.home_order, ids(&["w", "y"]));
    }

    #[test]
    fn full_dock_swaps_with_the_target_slot() {
        let mut p = Prefs {
            dock: ids(&["d0", "d1", "d2", "d3", "d4", "d5"]),
            ..Prefs::default()
        };
        p.drop_on_dock("x", 2, 1, &ids(&["w", "x", "y"]));
        assert_eq!(p.dock, ids(&["d0", "d1", "x", "d3", "d4", "d5"]));
        // The displaced app takes the dragged icon's old grid place.
        assert_eq!(p.home_order, ids(&["w", "d2", "y"]));
    }

    #[test]
    fn reorder_inside_the_dock() {
        let mut p = Prefs {
            dock: ids(&["a", "b", "c"]),
            ..Prefs::default()
        };
        p.drop_on_dock("a", 2, 0, &ids(&["w"]));
        assert_eq!(p.dock, ids(&["b", "c", "a"]));
        assert_eq!(p.home_order, ids(&["w"]));
    }

    #[test]
    fn dock_to_grid_unpins_and_places() {
        let mut p = Prefs {
            dock: ids(&["a", "b"]),
            ..Prefs::default()
        };
        p.drop_on_grid("a", 1, &ids(&["w", "y"]));
        assert_eq!(p.dock, ids(&["b"]));
        assert_eq!(p.home_order, ids(&["w", "a", "y"]));
    }

    #[test]
    fn grid_reorder_and_out_of_range_index() {
        let mut p = Prefs::default();
        p.drop_on_grid("w", 99, &ids(&["w", "x", "y"]));
        assert_eq!(p.home_order, ids(&["x", "y", "w"]));
    }
}

#[cfg(test)]
mod keyguard_privacy_tests {
    use super::*;

    #[test]
    fn missing_json_fields_fall_back_to_defaults() {
        let legacy_json = r#"{"deAccent":"copper","deWallpaperId":"ember","deTheme":"light"}"#;
        let parsed = Prefs::from_json(legacy_json).expect("legacy prefs json must parse");
        assert_eq!(parsed.lock_method, DEFAULT_LOCK_METHOD);
        assert_eq!(parsed.lock_method, "swipe");
        assert_eq!(
            parsed.lockscreen_notifications,
            DEFAULT_LOCKSCREEN_NOTIFICATIONS
        );
        assert_eq!(parsed.lockscreen_notifications, "show_all");
    }

    #[test]
    fn set_lock_method_accepts_valid_values_and_persists() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prefs.json");
        let mut prefs = Prefs::default();

        for method in ["pin", "passphrase", "biometric", "swipe"] {
            prefs
                .set_lock_method_to(&path, method)
                .expect("valid lock_method must succeed");
            assert_eq!(prefs.lock_method, method);

            let reloaded =
                Prefs::from_json(&std::fs::read_to_string(&path).unwrap()).expect("valid json");
            assert_eq!(reloaded.lock_method, method);
        }
    }

    #[test]
    fn set_lock_method_rejects_invalid_values_without_mutating() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prefs.json");
        let mut prefs = Prefs::default();
        prefs.set_lock_method_to(&path, "pin").unwrap();

        for invalid in ["", "pattern", "PIN", "face", "none"] {
            let err = prefs
                .set_lock_method_to(&path, invalid)
                .expect_err("invalid lock_method must be rejected");
            assert!(err.contains("invalid lock_method"), "unexpected err: {err}");
            assert_eq!(prefs.lock_method, "pin");
        }
    }

    #[test]
    fn set_lockscreen_notifications_accepts_valid_values_and_persists() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prefs.json");
        let mut prefs = Prefs::default();

        for mode in ["hide_sensitive", "hide_all", "show_all"] {
            prefs
                .set_lockscreen_notifications_to(&path, mode)
                .expect("valid lockscreen_notifications must succeed");
            assert_eq!(prefs.lockscreen_notifications, mode);

            let reloaded =
                Prefs::from_json(&std::fs::read_to_string(&path).unwrap()).expect("valid json");
            assert_eq!(reloaded.lockscreen_notifications, mode);
        }
    }

    #[test]
    fn set_lockscreen_notifications_rejects_invalid_values_without_mutating() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prefs.json");
        let mut prefs = Prefs::default();
        prefs
            .set_lockscreen_notifications_to(&path, "hide_sensitive")
            .unwrap();

        for invalid in ["", "hide", "ShowAll", "redact_all"] {
            let err = prefs
                .set_lockscreen_notifications_to(&path, invalid)
                .expect_err("invalid lockscreen_notifications must be rejected");
            assert!(
                err.contains("invalid lockscreen_notifications"),
                "unexpected err: {err}"
            );
            assert_eq!(prefs.lockscreen_notifications, "hide_sensitive");
        }
    }
}
