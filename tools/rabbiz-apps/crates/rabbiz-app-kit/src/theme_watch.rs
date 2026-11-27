//! Lightweight polling watcher for live Light/Dark theme + accent reactivity.
//!
//! Apps instantiate [`ThemeWatcher`] once at startup and call
//! [`ThemeWatcher::poll`] on frame ticks or periodic timers. `poll` only reads
//! and parses `prefs.json` when its filesystem modification time changes (or
//! when the file appears/disappears), and only returns `true` when the active
//! `theme_mode` or `deAccent` actually changed so callers know whether to
//! trigger a repaint. The returned [`Theme`] has the persisted accent applied
//! via `de_design::theme::theme_with`.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use de_config::Prefs;
use de_design::theme::theme_with;
use de_design::tokens::Theme;

/// Watches a `prefs.json` file for live `theme_mode`/`deAccent` changes.
#[derive(Debug, Clone)]
pub struct ThemeWatcher {
    prefs_path: PathBuf,
    last_mtime: Option<SystemTime>,
    mode: String,
    accent: String,
    theme: Theme,
}

impl ThemeWatcher {
    /// Create a watcher targeting the default [`Prefs::path()`] and seeded via
    /// [`Prefs::load()`].
    #[must_use]
    pub fn new() -> Self {
        let prefs_path = Prefs::path();
        let last_mtime = stat_mtime(&prefs_path);
        let prefs = Prefs::load();
        let theme = theme_with(&prefs.theme_mode, &prefs.accent);
        Self {
            prefs_path,
            last_mtime,
            mode: prefs.theme_mode,
            accent: prefs.accent,
            theme,
        }
    }

    /// Create a watcher targeting an explicit `prefs.json` path.
    ///
    /// If `path` is missing, unreadable, or contains invalid JSON, the watcher
    /// cleanly falls back to [`Prefs::default()`].
    #[must_use]
    pub fn from_path(path: PathBuf) -> Self {
        let last_mtime = stat_mtime(&path);
        let prefs = read_prefs_from(&path);
        let theme = theme_with(&prefs.theme_mode, &prefs.accent);
        Self {
            prefs_path: path,
            last_mtime,
            mode: prefs.theme_mode,
            accent: prefs.accent,
            theme,
        }
    }

    /// Currently active [`Theme`] palette.
    #[must_use]
    pub fn current(&self) -> &Theme {
        &self.theme
    }

    /// Currently active theme mode string (e.g. `"light"` or `"dark"`).
    #[must_use]
    pub fn mode(&self) -> &str {
        &self.mode
    }

    /// Currently active `deAccent` id (e.g. `"copper"` or `"blue"`).
    #[must_use]
    pub fn accent(&self) -> &str {
        &self.accent
    }

    /// Check the watched `prefs.json` modification time and reload if it
    /// changed (or if the file appeared/disappeared).
    ///
    /// Returns `true` only when the active `theme_mode` or `deAccent`
    /// actually changed.
    pub fn poll(&mut self) -> bool {
        let mtime = stat_mtime(&self.prefs_path);
        if mtime == self.last_mtime {
            return false;
        }
        self.last_mtime = mtime;
        self.apply_reloaded_prefs()
    }

    /// Unconditionally reload `prefs.json` regardless of modification time.
    ///
    /// Returns `true` only when the active `theme_mode` or `deAccent`
    /// actually changed.
    pub fn force_reload(&mut self) -> bool {
        self.last_mtime = stat_mtime(&self.prefs_path);
        self.apply_reloaded_prefs()
    }

    fn apply_reloaded_prefs(&mut self) -> bool {
        let prefs = read_prefs_from(&self.prefs_path);
        if prefs.theme_mode == self.mode && prefs.accent == self.accent {
            return false;
        }
        self.theme = theme_with(&prefs.theme_mode, &prefs.accent);
        self.mode = prefs.theme_mode;
        self.accent = prefs.accent;
        true
    }
}

impl Default for ThemeWatcher {
    fn default() -> Self {
        Self::new()
    }
}

fn stat_mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

fn read_prefs_from(path: &Path) -> Prefs {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| Prefs::from_json(&text).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use de_design::tokens::{DARK, LIGHT};
    use std::fs::{self, File};
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, UNIX_EPOCH};

    static NEXT_TEST_ID: AtomicU64 = AtomicU64::new(1);

    struct TestDir(PathBuf);

    impl TestDir {
        fn new(label: &str) -> Self {
            let id = NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!(
                "rabbiz-theme-watch-{label}-{}-{id}",
                std::process::id()
            ));
            fs::create_dir_all(&dir).expect("create temp test dir");
            Self(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn write_prefs_with_mtime(path: &Path, theme_mode: &str, secs_since_epoch: u64) {
        let prefs = Prefs {
            theme_mode: theme_mode.to_string(),
            ..Prefs::default()
        };
        let mut file = File::create(path).expect("create prefs file");
        file.write_all(prefs.to_json().as_bytes())
            .expect("write prefs json");
        file.sync_all().ok();
        let ts = UNIX_EPOCH + Duration::from_secs(secs_since_epoch);
        file.set_modified(ts).expect("set file mtime");
    }

    #[test]
    fn missing_file_defaults_cleanly_without_panicking() {
        let dir = TestDir::new("missing");
        let path = dir.path().join("does-not-exist.json");

        let mut watcher = ThemeWatcher::from_path(path);
        assert_eq!(watcher.mode(), "light");
        assert_eq!(watcher.current().bg, LIGHT.bg);
        assert!(!watcher.poll());
        assert!(!watcher.force_reload());
    }

    #[test]
    fn unchanged_file_returns_false_on_poll() {
        let dir = TestDir::new("unchanged");
        let path = dir.path().join("prefs.json");
        write_prefs_with_mtime(&path, "light", 1_700_000_000);

        let mut watcher = ThemeWatcher::from_path(path);
        assert_eq!(watcher.mode(), "light");
        assert_eq!(watcher.current().bg, LIGHT.bg);
        assert!(!watcher.poll());
        assert!(!watcher.poll());
    }

    #[test]
    fn updating_theme_mode_from_light_to_dark_updates_state_and_returns_true() {
        let dir = TestDir::new("light-to-dark");
        let path = dir.path().join("prefs.json");
        write_prefs_with_mtime(&path, "light", 1_700_000_000);

        let mut watcher = ThemeWatcher::from_path(path.clone());
        assert_eq!(watcher.mode(), "light");
        assert_eq!(watcher.current().bg, LIGHT.bg);

        // Update mode with a distinct mtime -> poll detects change.
        write_prefs_with_mtime(&path, "dark", 1_700_000_010);
        assert!(watcher.poll());
        assert_eq!(watcher.mode(), "dark");
        assert_eq!(watcher.current().bg, DARK.bg);

        // Subsequent poll with no further change returns false.
        assert!(!watcher.poll());

        // Update mode with the same mtime -> force_reload still detects change.
        write_prefs_with_mtime(&path, "light", 1_700_000_010);
        assert!(!watcher.poll());
        assert!(watcher.force_reload());
        assert_eq!(watcher.mode(), "light");
        assert_eq!(watcher.current().bg, LIGHT.bg);
    }

    #[test]
    fn touching_file_without_changing_theme_mode_returns_false() {
        let dir = TestDir::new("touch-same-mode");
        let path = dir.path().join("prefs.json");
        write_prefs_with_mtime(&path, "dark", 1_700_000_000);

        let mut watcher = ThemeWatcher::from_path(path.clone());
        assert_eq!(watcher.mode(), "dark");
        assert_eq!(watcher.current().bg, DARK.bg);

        // Rewrite with a newer mtime and a different unrelated field, same theme_mode.
        let prefs = Prefs {
            theme_mode: "dark".to_string(),
            do_not_disturb: true,
            ..Prefs::default()
        };
        let mut file = File::create(&path).expect("rewrite prefs file");
        file.write_all(prefs.to_json().as_bytes())
            .expect("write prefs json");
        file.set_modified(UNIX_EPOCH + Duration::from_secs(1_700_000_020))
            .expect("bump mtime");

        assert!(!watcher.poll());
        assert_eq!(watcher.mode(), "dark");
        assert_eq!(watcher.current().bg, DARK.bg);
    }

    #[test]
    fn accent_change_repaints_without_mode_change() {
        let dir = TestDir::new("accent-only");
        let path = dir.path().join("prefs.json");
        write_prefs_with_mtime(&path, "light", 1_700_000_000);

        let mut watcher = ThemeWatcher::from_path(path.clone());
        assert_eq!(watcher.accent(), "copper");
        assert_eq!(watcher.current().accent, LIGHT.accent);

        // Same mode, new accent -> poll still reports a change.
        let prefs = Prefs {
            accent: "blue".into(),
            ..Prefs::default()
        };
        let mut file = File::create(&path).expect("rewrite prefs file");
        file.write_all(prefs.to_json().as_bytes())
            .expect("write prefs json");
        file.set_modified(UNIX_EPOCH + Duration::from_secs(1_700_000_030))
            .expect("bump mtime");

        assert!(watcher.poll());
        assert_eq!(watcher.mode(), "light");
        assert_eq!(watcher.accent(), "blue");
        assert_eq!(
            watcher.current().accent,
            de_design::theme::accent_colors(de_design::theme::Mode::Light, "blue")
                .unwrap()
                .accent
        );
        assert!(!watcher.poll());
    }

    #[test]
    fn file_appearing_and_disappearing_updates_theme() {
        let dir = TestDir::new("appear-disappear");
        let path = dir.path().join("prefs.json");

        let mut watcher = ThemeWatcher::from_path(path.clone());
        assert_eq!(watcher.mode(), "light");

        // File appears with dark mode.
        write_prefs_with_mtime(&path, "dark", 1_700_000_000);
        assert!(watcher.poll());
        assert_eq!(watcher.mode(), "dark");
        assert_eq!(watcher.current().bg, DARK.bg);

        // File disappears -> falls back to default light mode.
        fs::remove_file(&path).expect("remove prefs file");
        assert!(watcher.poll());
        assert_eq!(watcher.mode(), "light");
        assert_eq!(watcher.current().bg, LIGHT.bg);
        assert!(!watcher.poll());
    }
}
