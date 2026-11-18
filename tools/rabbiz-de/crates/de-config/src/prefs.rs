//! DE prefs (M3-T7, Decision 14): the DE owns
//! `~/.config/rabbiz-de/prefs.json` — atomic writes, `.bak` backup, and a
//! one-time read of the legacy `~/.config/a21s-shell/prefs.json` for defaults.
//! The legacy file is never written (the QML `ShellPrefs.save()` serialises a
//! fixed key set and would erase unknown keys).

use std::collections::BTreeSet;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Defaults from Decision 14: brand accent `copper`, wallpaper `ember`.
pub const DEFAULT_ACCENT: &str = "copper";
pub const DEFAULT_WALLPAPER: &str = "ember";
pub const DEFAULT_THEME: &str = "light";

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
        config_home().join("a21s-shell").join("prefs.json")
    }

    /// Load the own file; when absent, seed defaults from the legacy file
    /// (one-time hint read — still never written).
    #[must_use]
    pub fn load() -> Self {
        Self::load_from(&Self::path(), &Self::legacy_path())
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
        // `lightTheme` is deliberately not seeded: the legacy default was
        // dark, the DE default is Cream Glass light (Decision 9) — carrying
        // the old default over would silently undo the owner's choice.
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
}

fn config_home() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."))
}
