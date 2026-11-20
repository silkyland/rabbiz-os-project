//! Wayland Flutter Embedder host runner and bundle validator for RabbizOS.
//!
//! Validates a Flutter bundle directory (`flutter_assets/AssetManifest.json` or
//! `kernel_blob.bin`, optional AOT `lib/libapp.so`, `rabbiz.toml`), enforces the
//! Panfrost / software rendering environment contract (`RABBIZ_FORCE_SOFTWARE=1`),
//! probes `libflutter_engine.so` on Linux via `dlopen`, and provides an interactive
//! `FlutterHostApp` preview window when running on macOS or without the proprietary
//! engine binary installed.

use std::path::{Path, PathBuf};

use de_api::{Point, Rect, Size};
use de_design::tokens::{self, Theme};
use de_paint::Canvas;
use rabbiz_app_kit::{AppEvent, AppFrame, AppState, TextInputCtl};
use serde::{Deserialize, Serialize};

/// Execution mode selected for a validated [`FlutterBundle`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FlutterEngineMode {
    /// JIT/debug bundle (`flutter_assets/kernel_blob.bin`).
    JitKernel,
    /// Release AOT bundle (`lib/libapp.so`).
    AotElf,
    /// Asset-only or host preview bundle (`flutter_assets/AssetManifest.json`).
    AssetPreview,
}

/// Validated Flutter application bundle metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlutterBundle {
    pub root_dir: PathBuf,
    pub assets_dir: PathBuf,
    pub app_id: String,
    pub title: String,
    pub mode: FlutterEngineMode,
    pub aot_elf_path: Option<PathBuf>,
    pub force_software: bool,
}

#[derive(Debug, Deserialize)]
struct MinimalManifest {
    app: Option<MinimalAppSection>,
}

#[derive(Debug, Deserialize)]
struct MinimalAppSection {
    id: Option<String>,
    name: Option<String>,
}

impl FlutterBundle {
    /// Validate and load a Flutter bundle directory (`bundle_dir` or `bundle_dir/build/flutter_assets`).
    pub fn load(bundle_dir: &Path) -> Result<Self, String> {
        if !bundle_dir.exists() {
            return Err(format!("bundle directory does not exist: {}", bundle_dir.display()));
        }
        let candidates = [
            bundle_dir.join("flutter_assets"),
            bundle_dir.join("data/flutter_assets"),
            bundle_dir.join("build/flutter_assets"),
            bundle_dir.to_path_buf(),
        ];

        let assets_dir = candidates
            .into_iter()
            .find(|d| {
                d.join("AssetManifest.json").is_file()
                    || d.join("AssetManifest.bin").is_file()
                    || d.join("kernel_blob.bin").is_file()
                    || d.join("FontManifest.json").is_file()
            })
            .ok_or_else(|| {
                format!(
                    "no valid flutter_assets (AssetManifest.json or kernel_blob.bin) found in {}",
                    bundle_dir.display()
                )
            })?;

        let aot_candidates = [
            bundle_dir.join("lib/libapp.so"),
            bundle_dir.join("data/lib/libapp.so"),
            bundle_dir.join("libapp.so"),
        ];
        let aot_elf_path = aot_candidates.into_iter().find(|p| p.is_file());

        let mode = if aot_elf_path.is_some() {
            FlutterEngineMode::AotElf
        } else if assets_dir.join("kernel_blob.bin").is_file() {
            FlutterEngineMode::JitKernel
        } else {
            FlutterEngineMode::AssetPreview
        };

        let (app_id, title) = read_bundle_identity(bundle_dir);
        let force_software = std::env::var("RABBIZ_FORCE_SOFTWARE")
            .ok()
            .is_none_or(|v| v != "0" && !v.eq_ignore_ascii_case("false"));

        Ok(Self {
            root_dir: bundle_dir.to_path_buf(),
            assets_dir,
            app_id,
            title,
            mode,
            aot_elf_path,
            force_software,
        })
    }

    /// Return the environment variable contract required by the RabbizOS Flutter embedder.
    #[must_use]
    pub fn env_contract(&self) -> Vec<(String, String)> {
        let wayland_display =
            std::env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| "wayland-0".to_string());
        vec![
            (
                "RABBIZ_FORCE_SOFTWARE".to_string(),
                if self.force_software { "1" } else { "0" }.to_string(),
            ),
            ("WAYLAND_DISPLAY".to_string(), wayland_display),
            ("FLUTTER_ASSETS_DIR".to_string(), self.assets_dir.display().to_string()),
            ("RABBIZ_APP_ID".to_string(), self.app_id.clone()),
        ]
    }
}

fn read_bundle_identity(bundle_dir: &Path) -> (String, String) {
    let manifest_path = bundle_dir.join("rabbiz.toml");
    if let Ok(raw) = std::fs::read_to_string(&manifest_path) {
        if let Ok(parsed) = toml::from_str::<MinimalManifest>(&raw) {
            if let Some(app) = parsed.app {
                let id = app.id.unwrap_or_else(|| "rabbiz-flutter-app".to_string());
                let name = app.name.unwrap_or_else(|| "Flutter App".to_string());
                return (id, name);
            }
        }
    }
    let dir_name = bundle_dir
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("rabbiz-flutter-app");
    (dir_name.to_string(), format!("Flutter ({dir_name})"))
}

/// Probe whether `libflutter_engine.so` can be dynamically opened on the current host.
#[must_use]
pub fn probe_flutter_engine_library() -> bool {
    #[cfg(target_os = "linux")]
    {
        use std::ffi::CString;
        let Ok(name) = CString::new("libflutter_engine.so") else {
            return false;
        };
        // SAFETY: `dlopen` with `RTLD_LAZY | RTLD_LOCAL` safely probes dynamic linker paths.
        #[allow(unsafe_code)]
        unsafe {
            let handle = libc::dlopen(name.as_ptr(), libc::RTLD_LAZY | libc::RTLD_LOCAL);
            if !handle.is_null() {
                let _ = libc::dlclose(handle);
                return true;
            }
        }
        false
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

/// Interactive fallback/preview `AppState` window rendering the Flutter bundle's
/// status, mode, software rendering contract, and simulated counter interaction.
pub struct FlutterHostApp {
    app_id: &'static str,
    title: &'static str,
    pub bundle: FlutterBundle,
    pub engine_present: bool,
    pub tap_count: u32,
}

impl FlutterHostApp {
    #[must_use]
    pub fn new(bundle: FlutterBundle) -> Self {
        let leaked_id: &'static str = Box::leak(bundle.app_id.clone().into_boxed_str());
        let leaked_title: &'static str = Box::leak(bundle.title.clone().into_boxed_str());
        Self {
            app_id: leaked_id,
            title: leaked_title,
            bundle,
            engine_present: probe_flutter_engine_library(),
            tap_count: 0,
        }
    }

    #[must_use]
    pub fn fab_rect(size: Size) -> Rect {
        Rect::new(
            (size.w - 96).max(16),
            (size.h - 140).max(80),
            64,
            64,
        )
    }
}

impl AppState for FlutterHostApp {
    fn app_id(&self) -> &'static str {
        self.app_id
    }

    fn title(&self) -> &'static str {
        self.title
    }

    fn handle_event(&mut self, event: AppEvent, _ti: &TextInputCtl) -> AppFrame {
        if let AppEvent::TouchDown { .. } = event {
            self.tap_count += 1;
            return AppFrame {
                dirty: true,
                animating: false,
                quit: false,
            };
        }
        AppFrame::default()
    }

    fn render(&mut self, canvas: &mut Canvas, theme: &Theme) {
        let size = canvas.size();
        let top_y = rabbiz_app_kit::ui::draw_header(
            canvas,
            theme,
            self.title,
            &format!("Flutter Embedder · {:?}", self.bundle.mode),
        );

        let card = Rect::new(24, top_y + 16, (size.w - 48).max(120), 220);
        rabbiz_app_kit::ui::draw_card(canvas, theme, card, false);

        let sw_label = if self.bundle.force_software {
            "Renderer: Software (RABBIZ_FORCE_SOFTWARE=1)"
        } else {
            "Renderer: Hardware GLES2"
        };
        rabbiz_app_kit::ui::draw_text(
            canvas,
            sw_label,
            &rabbiz_app_kit::ui::body(theme.text),
            card.x + 20,
            card.y + 24,
            Some((card.w - 40).max(80)),
        );

        let engine_label = if self.engine_present {
            "Engine: libflutter_engine.so loaded"
        } else {
            "Engine: Host Preview (RabbizOS Embedder Stub)"
        };
        rabbiz_app_kit::ui::draw_text(
            canvas,
            engine_label,
            &rabbiz_app_kit::ui::caption(theme.text2),
            card.x + 20,
            card.y + 68,
            Some((card.w - 40).max(80)),
        );

        rabbiz_app_kit::ui::draw_pill_button(
            canvas,
            theme,
            Rect::new(card.x + 20, card.y + 128, (card.w - 40).min(260), 56),
            &format!("Flutter Taps: {}", self.tap_count),
            true,
            false,
        );

        let fab = Self::fab_rect(size);
        canvas.fill_rounded_rect(fab, tokens::RADIUS_PILL, theme.accent);
        rabbiz_app_kit::ui::draw_centered(
            canvas,
            "+",
            &rabbiz_app_kit::ui::title(theme.text_on_accent),
            fab,
        );

        let _ = Point::ZERO;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rabbiz_app_kit::TestHarness;

    #[test]
    fn flutter_bundle_validation_and_test_harness_preview() {
        let tmp = std::env::temp_dir().join(format!(
            "rabbiz-flutter-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&tmp);
        let assets = tmp.join("flutter_assets");
        std::fs::create_dir_all(&assets).unwrap();
        std::fs::write(assets.join("AssetManifest.json"), "{}").unwrap();
        std::fs::write(assets.join("kernel_blob.bin"), b"dart-kernel").unwrap();
        std::fs::write(
            tmp.join("rabbiz.toml"),
            "[app]\nid = \"com.example.flut\"\nname = \"Flutter Counter\"\n",
        )
        .unwrap();

        let bundle = FlutterBundle::load(&tmp).expect("load flutter bundle");
        assert_eq!(bundle.app_id, "com.example.flut");
        assert_eq!(bundle.title, "Flutter Counter");
        assert_eq!(bundle.mode, FlutterEngineMode::JitKernel);
        assert!(bundle.force_software);

        let env_vars = bundle.env_contract();
        assert!(env_vars
            .iter()
            .any(|(k, v)| k == "RABBIZ_FORCE_SOFTWARE" && v == "1"));

        let host_app = FlutterHostApp::new(bundle);
        let mut harness = TestHarness::new(host_app).with_size(Size::new(360, 640));
        harness.tap(Point::new(100, 200));
        assert_eq!(harness.state().tap_count, 1);
        let px = harness.render();
        assert_eq!(px.size, Size::new(360, 640));

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
