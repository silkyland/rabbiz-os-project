//! Public SemVer Rust SDK facade for RabbizOS application developers.
//!
//! Provides a stable, safe, ergonomic API over `rabbiz-app-kit`, `de-api`,
//! `de-design`, and `de-paint` with app-scoped XDG storage, intent dispatch,
//! theme watching, MCP server hooks, and deterministic headless testing.

#![forbid(unsafe_code)]

/// Application lifecycle, input events, and Wayland/Host window runner.
pub mod app {
    pub use rabbiz_app_kit::{
        run_app, AppEvent, AppFrame, AppState, ContentPurpose, TextInputCtl, BTN_LEFT, BTN_MIDDLE,
        BTN_RIGHT,
    };
}

/// Material Design 3 drawing primitives, typography, widgets, icons, and tokens.
pub mod ui {
    pub use rabbiz_app_kit::icons::*;
    pub use rabbiz_app_kit::ui::*;
    pub use rabbiz_app_kit::widgets::*;
    pub use rabbiz_app_kit::{load_theme, text, tokens, Canvas, ScrollView, TextBuffer, Theme};
}

/// Physical pixel geometry and safe-area window insets.
pub mod insets {
    pub use de_api::{EdgeInsets, Point, Rect, Size, WindowInsets};
}

/// Cross-app URI and action intent routing.
pub mod intent {
    pub use rabbiz_app_kit::intent::{parse_uri, resolve_mime_app};
    pub use rabbiz_app_kit::Intent;

    /// Dispatch a cross-app [`Intent`] in the active session or dry-run environment.
    ///
    /// If `RABBIZ_SDK_DRY_RUN=1` is set (or in headless tests when configured),
    /// validates the command without spawning a background process.
    pub fn dispatch(intent: &Intent) -> Result<(), String> {
        let (bin, argv) = intent.to_command();
        if bin.is_empty() {
            return Err("empty intent command binary".to_string());
        }
        if std::env::var("RABBIZ_SDK_DRY_RUN").as_deref() == Ok("1") {
            return Ok(());
        }
        // Attempt non-blocking spawn if binary exists on PATH; otherwise return
        // a descriptive error that callers can log or ignore in offline sim runs.
        std::process::Command::new(bin)
            .args(&argv)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("dispatch {bin} {argv:?}: {e}"))
    }
}

/// App-scoped XDG directories (`data`, `config`, `cache`) and atomic JSON persistence.
pub mod storage {
    use std::path::{Path, PathBuf};

    use serde::de::DeserializeOwned;
    use serde::Serialize;

    fn sanitize_segment(raw: &str) -> String {
        let s: String = raw
            .trim()
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        if s.is_empty() || s == "." || s == ".." {
            "rabbiz-app".to_string()
        } else {
            s
        }
    }

    fn base_dir(env_key: &str, home_fallback: &str) -> PathBuf {
        if let Ok(val) = std::env::var(env_key) {
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                return PathBuf::from(trimmed);
            }
        }
        if let Ok(home) = std::env::var("HOME") {
            let trimmed = home.trim();
            if !trimmed.is_empty() {
                return PathBuf::from(trimmed).join(home_fallback);
            }
        }
        std::env::temp_dir().join("rabbiz-xdg")
    }

    /// Resolve `$XDG_DATA_HOME/rabbiz/apps/<app_id>` (defaulting to `~/.local/share/rabbiz/apps/<app_id>`).
    #[must_use]
    pub fn data_dir(app_id: &str) -> PathBuf {
        base_dir("XDG_DATA_HOME", ".local/share")
            .join("rabbiz")
            .join("apps")
            .join(sanitize_segment(app_id))
    }

    /// Resolve `$XDG_CONFIG_HOME/rabbiz/apps/<app_id>` (defaulting to `~/.config/rabbiz/apps/<app_id>`).
    #[must_use]
    pub fn config_dir(app_id: &str) -> PathBuf {
        base_dir("XDG_CONFIG_HOME", ".config")
            .join("rabbiz")
            .join("apps")
            .join(sanitize_segment(app_id))
    }

    /// Resolve `$XDG_CACHE_HOME/rabbiz/apps/<app_id>` (defaulting to `~/.cache/rabbiz/apps/<app_id>`).
    #[must_use]
    pub fn cache_dir(app_id: &str) -> PathBuf {
        base_dir("XDG_CACHE_HOME", ".cache")
            .join("rabbiz")
            .join("apps")
            .join(sanitize_segment(app_id))
    }

    fn json_file_path(dir: &Path, name: &str) -> PathBuf {
        let clean = sanitize_segment(name);
        if Path::new(&clean)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
        {
            dir.join(clean)
        } else {
            dir.join(format!("{clean}.json"))
        }
    }

    /// Read and deserialize `<name>.json` from `data_dir(app_id)`.
    #[must_use]
    pub fn read_json<T: DeserializeOwned>(app_id: &str, name: &str) -> Option<T> {
        read_json_in(&data_dir(app_id), name)
    }

    /// Atomically serialize `val` to `<name>.json` inside `data_dir(app_id)` via temporary file + rename.
    pub fn write_json<T: Serialize>(app_id: &str, name: &str, val: &T) -> Result<(), String> {
        write_json_in(&data_dir(app_id), name, val)
    }

    /// Read and deserialize `<name>.json` from an explicit directory `dir`.
    #[must_use]
    pub fn read_json_in<T: DeserializeOwned>(dir: &Path, name: &str) -> Option<T> {
        let path = json_file_path(dir, name);
        let text = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&text).ok()
    }

    /// Atomically write `<name>.json` inside an explicit directory `dir` (tmp write + rename).
    pub fn write_json_in<T: Serialize>(dir: &Path, name: &str, val: &T) -> Result<(), String> {
        std::fs::create_dir_all(dir).map_err(|e| format!("mkdir {}: {e}", dir.display()))?;
        let final_path = json_file_path(dir, name);
        let tmp_name = format!(
            ".{}.tmp-{}",
            final_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("state.json"),
            std::process::id()
        );
        let tmp_path = dir.join(tmp_name);
        let bytes =
            serde_json::to_vec_pretty(val).map_err(|e| format!("serialize json: {e}"))?;
        std::fs::write(&tmp_path, bytes)
            .map_err(|e| format!("write {}: {e}", tmp_path.display()))?;
        std::fs::rename(&tmp_path, &final_path).map_err(|e| {
            let _ = std::fs::remove_file(&tmp_path);
            format!(
                "rename {} -> {}: {e}",
                tmp_path.display(),
                final_path.display()
            )
        })
    }
}

/// System services: live theme reactivity and desktop notifications.
pub mod services {
    pub use rabbiz_app_kit::ThemeWatcher;

    /// Send a user notification from `app_id` with `title` and `body`.
    ///
    /// Records the notification to `$XDG_RUNTIME_DIR/rabbiz-notifications.jsonl`
    /// (or `/tmp/rabbiz-notifications.jsonl`) so the compositor / test harness can
    /// inspect or surface it as a heads-up notification banner.
    pub fn send_notification(app_id: &str, title: &str, body: &str) -> Result<(), String> {
        use std::io::Write;

        if app_id.trim().is_empty() {
            return Err("app_id must not be empty".to_string());
        }
        if title.trim().is_empty() {
            return Err("notification title must not be empty".to_string());
        }
        let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .map_or_else(std::env::temp_dir, std::path::PathBuf::from);
        let log_path = runtime_dir.join("rabbiz-notifications.jsonl");
        let entry = serde_json::json!({
            "app_id": app_id,
            "title": title,
            "body": body,
        });
        let mut line = serde_json::to_string(&entry).map_err(|e| e.to_string())?;
        line.push('\n');
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .map_err(|e| format!("open {}: {e}", log_path.display()))?;
        file.write_all(line.as_bytes())
            .map_err(|e| format!("write notification: {e}"))
    }
}

/// Model Context Protocol (MCP) stdio server helpers for AI-controllable apps.
pub mod mcp {
    pub use rabbiz_app_kit::mcp::{serve, Tool};
}

/// Adaptive layout helpers modeled on Material 3 / AOSP WindowSizeClass.
pub mod adaptive {
    use crate::insets::Size;

    /// Responsive width class for adaptive phone / tablet / desktop UIs.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum ViewportClass {
        /// Handset portrait (`w < 840px`, e.g., 720×1600).
        Compact,
        /// Tablet portrait / small landscape (`840px <= w < 1200px`).
        Medium,
        /// Landscape desktop / wide tablet (`w >= 1200px`, e.g., 1600×720 or 1280×800+).
        Expanded,
    }

    impl ViewportClass {
        /// Classify an output or window size into a responsive [`ViewportClass`].
        #[must_use]
        pub const fn from_size(size: Size) -> Self {
            if size.w >= 1200 {
                Self::Expanded
            } else if size.w >= 840 {
                Self::Medium
            } else {
                Self::Compact
            }
        }

        /// Recommended grid column count for card/list layouts.
        #[must_use]
        pub const fn grid_columns(self) -> usize {
            match self {
                Self::Compact => 1,
                Self::Medium => 2,
                Self::Expanded => 3,
            }
        }

        /// True when a two-pane / desktop sidebar layout is appropriate.
        #[must_use]
        pub const fn supports_split_pane(self) -> bool {
            matches!(self, Self::Medium | Self::Expanded)
        }
    }
}

/// MPRIS media-player metadata contract for `rabbiz-de` media pill & shade card.
pub mod media {
    use serde::{Deserialize, Serialize};

    /// Standard `org.mpris.MediaPlayer2.Player` playback status.
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
    pub enum PlaybackStatus {
        Playing,
        Paused,
        #[default]
        Stopped,
    }

    impl PlaybackStatus {
        #[must_use]
        pub const fn as_mpris_str(self) -> &'static str {
            match self {
                Self::Playing => "Playing",
                Self::Paused => "Paused",
                Self::Stopped => "Stopped",
            }
        }
    }

    /// Track metadata published to the compositor's MPRIS watcher.
    #[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
    pub struct MediaMetadata {
        pub title: String,
        pub artist: String,
        pub album: String,
        pub status: PlaybackStatus,
        pub duration_ms: u64,
        pub position_ms: u64,
        /// Local `file://` path under `$XDG_CACHE_HOME` or `$XDG_RUNTIME_DIR` (never remote HTTP).
        pub art_url: Option<String>,
    }

    impl MediaMetadata {
        /// Validate that `art_url`, if present, is a local `file://` URI without traversal.
        #[must_use]
        pub fn has_safe_local_art(&self) -> bool {
            match &self.art_url {
                None => true,
                Some(u) => u.starts_with("file:///") && !u.contains(".."),
            }
        }
    }
}

/// Headless app testing harness, golden image comparison, and simulator presets.
pub mod testing {
    pub use rabbiz_app_kit::{DevicePreset, GoldenCompare, SimOptions, TestHarness};
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{AppEvent, AppFrame, AppState, TextInputCtl};
    use crate::insets::{EdgeInsets, Point, Rect, Size, WindowInsets};
    use crate::testing::TestHarness;
    use crate::ui::{Canvas, Theme};
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct CounterData {
        count: i32,
        label: String,
    }

    struct CounterApp {
        count: i32,
    }

    impl AppState for CounterApp {
        fn app_id(&self) -> &'static str {
            "com.rabbiz.sdktest"
        }

        fn title(&self) -> &'static str {
            "SDK Counter"
        }

        fn handle_event(&mut self, event: AppEvent, _ti: &TextInputCtl) -> AppFrame {
            if let AppEvent::TouchDown { .. } = event {
                self.count += 1;
                return AppFrame {
                    dirty: true,
                    animating: false,
                    quit: false,
                };
            }
            AppFrame::default()
        }

        fn render(&mut self, canvas: &mut Canvas, theme: &Theme) {
            let y = crate::ui::draw_header(canvas, theme, "SDK Test", "Tap anywhere");
            crate::ui::draw_pill_button(
                canvas,
                theme,
                Rect::new(24, y + 16, 200, 56),
                &format!("Count: {}", self.count),
                true,
                false,
            );
        }
    }

    #[test]
    fn storage_atomic_json_roundtrip() {
        let dir = std::env::temp_dir().join(format!(
            "rabbiz-sdk-storage-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);

        let payload = CounterData {
            count: 42,
            label: "สวัสดี RabbizOS".to_string(),
        };
        storage::write_json_in(&dir, "counter_state", &payload).expect("write_json_in");
        let loaded: CounterData =
            storage::read_json_in(&dir, "counter_state").expect("read_json_in");
        assert_eq!(loaded, payload);

        // Verify XDG directory helpers produce app-scoped paths.
        assert!(storage::data_dir("com.example.demo")
            .to_string_lossy()
            .contains("com.example.demo"));
        assert!(storage::config_dir("com.example.demo")
            .to_string_lossy()
            .contains("com.example.demo"));
        assert!(storage::cache_dir("com.example.demo")
            .to_string_lossy()
            .contains("com.example.demo"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn insets_and_intent_facades_work() {
        let win_insets = WindowInsets {
            system_bars: EdgeInsets::new(56, 0, 32, 0),
            cutout: EdgeInsets::new(56, 0, 0, 0),
            ime: EdgeInsets::ZERO,
        };
        let frame = win_insets.app_frame(Size::new(720, 1600));
        assert_eq!(frame, Rect::new(0, 56, 720, 1512));

        let parsed = intent::parse_uri("tel:+66812345678").expect("parse tel uri");
        assert_eq!(
            parsed.to_command(),
            (
                "rabbiz-dialer",
                vec!["--dial".to_string(), "+66812345678".to_string()]
            )
        );
    }

    #[test]
    fn test_harness_drives_rabbiz_sdk_app_and_services() {
        let mut harness =
            TestHarness::new(CounterApp { count: 0 }).with_size(Size::new(360, 640));
        harness.tap(Point::new(100, 180));
        harness.tap(Point::new(100, 180));
        assert_eq!(harness.state().count, 2);

        let px = harness.render();
        assert_eq!(px.size, Size::new(360, 640));
        assert!(px.data.iter().any(|&b| b != 0));

        services::send_notification("com.rabbiz.sdktest", "Count Updated", "Count is now 2")
            .expect("send_notification");
    }

    #[test]
    fn adaptive_viewport_class_and_media_metadata_work() {
        use adaptive::ViewportClass;
        assert_eq!(
            ViewportClass::from_size(Size::new(720, 1600)),
            ViewportClass::Compact
        );
        assert_eq!(
            ViewportClass::from_size(Size::new(1600, 720)),
            ViewportClass::Expanded
        );
        assert_eq!(ViewportClass::Compact.grid_columns(), 1);
        assert_eq!(ViewportClass::Expanded.grid_columns(), 3);
        assert!(ViewportClass::Expanded.supports_split_pane());

        let meta = media::MediaMetadata {
            title: "Track 1".to_string(),
            artist: "Rabbiz".to_string(),
            status: media::PlaybackStatus::Playing,
            art_url: Some("file:///tmp/cover.png".to_string()),
            ..Default::default()
        };
        assert_eq!(meta.status.as_mpris_str(), "Playing");
        assert!(meta.has_safe_local_art());
    }
}
