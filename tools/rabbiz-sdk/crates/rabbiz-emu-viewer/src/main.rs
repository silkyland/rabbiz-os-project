#![forbid(unsafe_code)]

use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use rabbiz_emu_viewer::ViewerState;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut emu_addr = "127.0.0.1:5910".to_string();
    let mut ctl_addr = "127.0.0.1:5911".to_string();
    let mut self_test = false;
    let mut demo_only = false;
    let mut out_png: Option<PathBuf> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--self-test" => self_test = true,
            "--demo" => demo_only = true,
            "--connect" | "--emu" => {
                if i + 1 < args.len() {
                    emu_addr.clone_from(&args[i + 1]);
                    i += 1;
                }
            }
            "--ctl" => {
                if i + 1 < args.len() {
                    ctl_addr.clone_from(&args[i + 1]);
                    i += 1;
                }
            }
            "--out" => {
                if i + 1 < args.len() {
                    out_png = Some(PathBuf::from(&args[i + 1]));
                    i += 1;
                }
            }
            "--help" | "-h" => {
                println!(
                    "Usage: rabbiz-emu-viewer [--connect 127.0.0.1:5910] [--ctl 127.0.0.1:5911] [--demo] [--self-test] [--out file.png]"
                );
                return;
            }
            _ => {}
        }
        i += 1;
    }

    let mut spawned_backend = None;
    if !self_test && !demo_only {
        spawned_backend = ensure_linux_backend_running(&emu_addr, &ctl_addr);
    }

    let mut state = ViewerState::new(emu_addr, ctl_addr);
    let _ = state.poll_remote_frame();

    if self_test {
        let out = out_png
            .unwrap_or_else(|| std::env::temp_dir().join("rabbiz-emu-viewer-selftest.png"));
        match state.save_viewer_png(&out) {
            Ok(()) => {
                let layout = state.layout();
                println!(
                    "rabbiz-emu-viewer self-test OK: {}x{} (panel={}px, buttons={}) -> {}",
                    layout.total_size.w,
                    layout.total_size.h,
                    layout.side_panel_rect.w,
                    layout.buttons.len(),
                    out.display()
                );
                return;
            }
            Err(err) => {
                eprintln!("rabbiz-emu-viewer self-test failed: {err}");
                std::process::exit(1);
            }
        }
    }

    run_viewer_window(state);

    if let Some(mut child) = spawned_backend {
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// Find the workspace root containing `tools/rabbiz-de` and `rootfs-overlay`.
fn find_repo_root() -> Option<PathBuf> {
    let mut cur = std::env::current_dir().ok()?;
    for _ in 0..5 {
        if cur.join("tools/rabbiz-de").is_dir() && cur.join("rootfs-overlay").is_dir() {
            return Some(cur);
        }
        if !cur.pop() {
            break;
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        let mut dir = exe.parent()?.to_path_buf();
        for _ in 0..6 {
            if dir.join("tools/rabbiz-de").is_dir() && dir.join("rootfs-overlay").is_dir() {
                return Some(dir);
            }
            if !dir.pop() {
                break;
            }
        }
    }
    None
}

/// Automatically ensure the background Linux compositor (`rabbiz-de --backend=virtual`)
/// is running with `rabbiz-apps`, `rabbiz-osk`, `.desktop` files, and SVG icons staged
/// when `emu_addr` (`127.0.0.1:5910`) is not yet listening.
///
/// On Linux hosts, prefers running the native host `rabbiz-de` binary directly with an
/// isolated runtime/data directory (`/tmp/rabbiz-emu-live`), falling back to the
/// `rabbiz-de-build:arm64` Docker container if only ARM64 container artifacts exist.
fn ensure_linux_backend_running(emu_addr: &str, ctl_addr: &str) -> Option<Child> {
    if let Ok(sock) = emu_addr.parse::<SocketAddr>() {
        if TcpStream::connect_timeout(&sock, Duration::from_millis(80)).is_ok() {
            return None;
        }
    } else {
        return None;
    }

    let repo = find_repo_root();

    #[cfg(target_os = "linux")]
    if let Some(child) = start_native_linux_backend(repo.as_deref(), emu_addr, ctl_addr) {
        return Some(child);
    }

    let Some(repo) = repo else {
        return None;
    };
    let rel_bin = repo.join(
        "tools/rabbiz-de/target-linux-arm64/aarch64-unknown-linux-gnu/release/rabbiz-de",
    );
    let dbg_bin = repo.join(
        "tools/rabbiz-de/target-linux-arm64/aarch64-unknown-linux-gnu/debug/rabbiz-de",
    );
    let de_container_bin = if Path::new(&rel_bin).is_file() {
        "/src/tools/rabbiz-de/target-linux-arm64/aarch64-unknown-linux-gnu/release/rabbiz-de"
    } else if Path::new(&dbg_bin).is_file() {
        "/src/tools/rabbiz-de/target-linux-arm64/aarch64-unknown-linux-gnu/debug/rabbiz-de"
    } else {
        return None;
    };

    let mount_arg = format!("{}:/src", repo.display());
    let boot_script = format!(
        concat!(
            "mkdir -p /tmp/rabbiz-rt /tmp/rabbiz-home /usr/share/applications /usr/share/icons/hicolor/scalable/apps && ",
            "chmod 700 /tmp/rabbiz-rt && ",
            "ln -sf /src/tools/rabbiz-apps/target-linux-arm64/release/rabbiz-* /usr/local/bin/ 2>/dev/null || true && ",
            "ln -sf /src/tools/rabbiz-apps/target-linux-arm64/aarch64-unknown-linux-gnu/release/rabbiz-* /usr/local/bin/ 2>/dev/null || true && ",
            "ln -sf /src/tools/rabbiz-osk/target-linux-arm64/release/rabbiz-osk /usr/local/bin/rabbiz-osk 2>/dev/null || true && ",
            "cp -f /src/rootfs-overlay/usr/share/applications/*.desktop /usr/share/applications/ 2>/dev/null || true && ",
            "cp -f /src/rootfs-overlay/usr/share/icons/hicolor/scalable/apps/*.svg /usr/share/icons/hicolor/scalable/apps/ 2>/dev/null || true && ",
            "exec {} run --backend=virtual --size=720x1600 --emu-listen=0.0.0.0:5910 --ctl-listen=0.0.0.0:5911"
        ),
        de_container_bin
    );

    let _ = Command::new("docker").args(["rm", "-f", "rabbiz-emu-live"]).output();
    let status = Command::new("docker")
        .args([
            "run",
            "-d",
            "--rm",
            "--name",
            "rabbiz-emu-live",
            "-p",
            "5910:5910",
            "-p",
            "5911:5911",
            "-v",
            &mount_arg,
            "-e",
            "XDG_RUNTIME_DIR=/tmp/rabbiz-rt",
            "-e",
            "HOME=/tmp/rabbiz-home",
            "-e",
            "XDG_DATA_DIRS=/usr/local/share:/usr/share:/var/lib/flatpak/exports/share",
            "rabbiz-de-build:arm64",
            "bash",
            "-c",
            &boot_script,
        ])
        .output();

    if status.is_ok() {
        wait_for_tcp(emu_addr);
    }
    None
}

fn wait_for_tcp(emu_addr: &str) {
    if let Ok(sock) = emu_addr.parse::<SocketAddr>() {
        for _ in 0..40 {
            if TcpStream::connect_timeout(&sock, Duration::from_millis(80)).is_ok() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

#[cfg(target_os = "linux")]
fn start_native_linux_backend(
    repo: Option<&Path>,
    emu_addr: &str,
    ctl_addr: &str,
) -> Option<Child> {
    use std::os::unix::fs::PermissionsExt;

    let de_bin = repo
        .and_then(|r| {
            [
                r.join("tools/rabbiz-de/target/release/rabbiz-de"),
                r.join("tools/rabbiz-de/target/debug/rabbiz-de"),
            ]
            .into_iter()
            .find(|p| p.is_file())
        })
        .or_else(|| {
            std::env::var_os("PATH").and_then(|paths| {
                std::env::split_paths(&paths)
                    .map(|dir| dir.join("rabbiz-de"))
                    .find(|p| p.is_file())
            })
        })?;

    let live_root = std::env::temp_dir().join("rabbiz-emu-live");
    let rt_dir = live_root.join("rt");
    let home_dir = live_root.join("home");
    let bin_dir = live_root.join("bin");
    let apps_dir = live_root.join("share/applications");
    let icons_dir = live_root.join("share/icons/hicolor/scalable/apps");
    let home_icons_dir = home_dir.join(".local/share/icons/hicolor/scalable/apps");

    for dir in [&rt_dir, &home_dir, &bin_dir, &apps_dir, &icons_dir, &home_icons_dir] {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::set_permissions(&rt_dir, std::fs::Permissions::from_mode(0o700));

    // The emulated runtime dir has no sound server; link the host PipeWire /
    // PulseAudio sockets so guest apps can reach the host speakers.
    if let Some(host_rt) = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from) {
        let pulse_dir = rt_dir.join("pulse");
        if pulse_dir.symlink_metadata().is_ok_and(|m| m.is_symlink()) {
            let _ = std::fs::remove_file(&pulse_dir);
        }
        let _ = std::fs::create_dir_all(&pulse_dir);
        for (src, dst) in [
            (host_rt.join("pipewire-0"), rt_dir.join("pipewire-0")),
            (
                host_rt.join("pipewire-0-manager"),
                rt_dir.join("pipewire-0-manager"),
            ),
            (host_rt.join("pulse/native"), pulse_dir.join("native")),
        ] {
            if src.exists() {
                let _ = std::fs::remove_file(&dst);
                let _ = std::os::unix::fs::symlink(&src, &dst);
            }
        }
    }

    if let Some(repo) = repo {
        // Stage built rabbiz-apps and rabbiz-osk binaries into bin_dir
        let bin_sources = [
            repo.join("tools/rabbiz-apps/target/release"),
            repo.join("tools/rabbiz-apps/target/debug"),
            repo.join("tools/rabbiz-osk/target/release"),
            repo.join("tools/rabbiz-osk/target/debug"),
        ];
        for src_dir in bin_sources {
            if let Ok(entries) = std::fs::read_dir(&src_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        if name.starts_with("rabbiz-")
                            && !name.contains('.')
                            && path.is_file()
                        {
                            let dst = bin_dir.join(name);
                            if !dst.exists() {
                                let _ = std::os::unix::fs::symlink(&path, &dst);
                            }
                        }
                    }
                }
            }
        }

        // Stage .desktop files and SVG icons
        let src_apps = repo.join("rootfs-overlay/usr/share/applications");
        if let Ok(entries) = std::fs::read_dir(&src_apps) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("desktop") {
                    if let Some(name) = path.file_name() {
                        let _ = std::fs::copy(&path, apps_dir.join(name));
                    }
                }
            }
        }
        let src_icons = repo.join("rootfs-overlay/usr/share/icons/hicolor/scalable/apps");
        if let Ok(entries) = std::fs::read_dir(&src_icons) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("svg") {
                    if let Some(name) = path.file_name() {
                        let _ = std::fs::copy(&path, icons_dir.join(name));
                        let _ = std::fs::copy(&path, home_icons_dir.join(name));
                    }
                }
            }
        }
    }

    let orig_path = std::env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{orig_path}", bin_dir.display());
    let data_dirs = format!(
        "{}:/usr/local/share:/usr/share:/var/lib/flatpak/exports/share",
        live_root.join("share").display()
    );

    let de_cfg_dir = home_dir.join(".config/rabbiz-de");
    let _ = std::fs::create_dir_all(&de_cfg_dir);
    let prefs_path = de_cfg_dir.join("prefs.json");
    let needs_seed = if let Ok(raw) = std::fs::read_to_string(&prefs_path) {
        serde_json::from_str::<serde_json::Value>(&raw)
            .map(|v| {
                let empty_dock = v.get("dock").and_then(|d| d.as_array()).is_none_or(Vec::is_empty);
                let empty_home = v
                    .get("homeOrder")
                    .and_then(|h| h.as_array())
                    .is_none_or(Vec::is_empty);
                empty_dock && empty_home
            })
            .unwrap_or(true)
    } else {
        true
    };
    if needs_seed {
        let default_prefs = serde_json::json!({
            "deScreenTimeoutSec": 0,
            "dock": [
                "rabbiz-dialer",
                "rabbiz-sms",
                "rabbiz-files",
                "rabbiz-settings"
            ],
            "homeOrder": [
                "rabbiz-store",
                "rabbiz-calculator",
                "rabbiz-clock",
                "rabbiz-notes",
                "rabbiz-photos",
                "rabbiz-music",
                "rabbiz-contacts",
                "rabbiz-pdf",
                "rabbiz-terminal",
                "rabbiz-gallery"
            ]
        });
        let _ = std::fs::write(&prefs_path, default_prefs.to_string());
    }

    let log_file = std::fs::File::create(live_root.join("rabbiz-de.log")).ok();
    let stdout = log_file
        .as_ref()
        .and_then(|f| f.try_clone().ok())
        .map_or_else(Stdio::null, Stdio::from);
    let stderr = log_file.map_or_else(Stdio::null, Stdio::from);

    let child = Command::new(&de_bin)
        .args([
            "run",
            "--backend=virtual",
            "--size=720x1600",
            &format!("--emu-listen={emu_addr}"),
            &format!("--ctl-listen={ctl_addr}"),
        ])
        .env("XDG_RUNTIME_DIR", &rt_dir)
        .env("HOME", &home_dir)
        .env("XDG_CONFIG_HOME", home_dir.join(".config"))
        .env("PATH", new_path)
        .env("XDG_DATA_DIRS", data_dirs)
        .env("LIBGL_ALWAYS_SOFTWARE", "1")
        .env("RABBIZ_FORCE_SOFTWARE", "1")
        .env("QT_QUICK_BACKEND", "software")
        .stdout(stdout)
        .stderr(stderr)
        .spawn()
        .ok()?;

    let _ = std::fs::write(live_root.join("rabbiz-de.pid"), child.id().to_string());
    wait_for_tcp(emu_addr);
    Some(child)
}

fn run_viewer_window(state: ViewerState) {
    use std::num::NonZeroU32;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use de_api::Point;
    use rabbiz_app_kit::sim_config::ViewportFit;
    use softbuffer::{Context, Surface};
    use winit::application::ApplicationHandler;
    use winit::dpi::{PhysicalPosition, PhysicalSize};
    use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
    use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
    use winit::keyboard::{KeyCode, ModifiersState, PhysicalKey};
    use winit::window::{Window, WindowAttributes, WindowId};

    fn winit_keycode_to_xkb(code: KeyCode) -> Option<u32> {
        let evdev = match code {
            KeyCode::Escape => 1,
            KeyCode::Digit1 => 2,
            KeyCode::Digit2 => 3,
            KeyCode::Digit3 => 4,
            KeyCode::Digit4 => 5,
            KeyCode::Digit5 => 6,
            KeyCode::Digit6 => 7,
            KeyCode::Digit7 => 8,
            KeyCode::Digit8 => 9,
            KeyCode::Digit9 => 10,
            KeyCode::Digit0 => 11,
            KeyCode::Minus => 12,
            KeyCode::Equal => 13,
            KeyCode::Backspace => 14,
            KeyCode::Tab => 15,
            KeyCode::KeyQ => 16,
            KeyCode::KeyW => 17,
            KeyCode::KeyE => 18,
            KeyCode::KeyR => 19,
            KeyCode::KeyT => 20,
            KeyCode::KeyY => 21,
            KeyCode::KeyU => 22,
            KeyCode::KeyI => 23,
            KeyCode::KeyO => 24,
            KeyCode::KeyP => 25,
            KeyCode::BracketLeft => 26,
            KeyCode::BracketRight => 27,
            KeyCode::Enter => 28,
            KeyCode::ControlLeft => 29,
            KeyCode::KeyA => 30,
            KeyCode::KeyS => 31,
            KeyCode::KeyD => 32,
            KeyCode::KeyF => 33,
            KeyCode::KeyG => 34,
            KeyCode::KeyH => 35,
            KeyCode::KeyJ => 36,
            KeyCode::KeyK => 37,
            KeyCode::KeyL => 38,
            KeyCode::Semicolon => 39,
            KeyCode::Quote => 40,
            KeyCode::Backquote => 41,
            KeyCode::ShiftLeft => 42,
            KeyCode::Backslash => 43,
            KeyCode::KeyZ => 44,
            KeyCode::KeyX => 45,
            KeyCode::KeyC => 46,
            KeyCode::KeyV => 47,
            KeyCode::KeyB => 48,
            KeyCode::KeyN => 49,
            KeyCode::KeyM => 50,
            KeyCode::Comma => 51,
            KeyCode::Period => 52,
            KeyCode::Slash => 53,
            KeyCode::ShiftRight => 54,
            KeyCode::AltLeft => 56,
            KeyCode::Space => 57,
            KeyCode::CapsLock => 58,
            KeyCode::F1 => 59,
            KeyCode::F2 => 60,
            KeyCode::F3 => 61,
            KeyCode::F4 => 62,
            KeyCode::F5 => 63,
            KeyCode::F6 => 64,
            KeyCode::F7 => 65,
            KeyCode::F8 => 66,
            KeyCode::F9 => 67,
            KeyCode::F10 => 68,
            KeyCode::F11 => 87,
            KeyCode::F12 => 88,
            KeyCode::NumpadEnter => 96,
            KeyCode::ControlRight => 97,
            KeyCode::AltRight => 100,
            KeyCode::Home => 102,
            KeyCode::ArrowUp => 103,
            KeyCode::PageUp => 104,
            KeyCode::ArrowLeft => 105,
            KeyCode::ArrowRight => 106,
            KeyCode::End => 107,
            KeyCode::ArrowDown => 108,
            KeyCode::PageDown => 109,
            KeyCode::Insert => 110,
            KeyCode::Delete => 111,
            KeyCode::SuperLeft => 125,
            KeyCode::SuperRight => 126,
            _ => return None,
        };
        Some(evdev + 8)
    }

    struct ViewerApp {
        state: ViewerState,
        window: Option<Arc<Window>>,
        surface: Option<Surface<Arc<Window>, Arc<Window>>>,
        win_phys_w: u32,
        win_phys_h: u32,
        cursor: PhysicalPosition<f64>,
        touch_dragging: bool,
        pending_drag_move: Option<Point>,
        modifiers: ModifiersState,
        dirty: bool,
        last_poll: Instant,
    }

    impl ViewerApp {
        fn current_fit(&self) -> ViewportFit {
            let layout = self.state.layout();
            ViewportFit::compute_fit(self.win_phys_w, self.win_phys_h, layout.total_size)
        }

        fn cursor_canvas_point(&self, clamped: bool) -> Option<Point> {
            let fit = self.current_fit();
            if clamped {
                Some(fit.window_to_app_clamped(self.cursor.x, self.cursor.y))
            } else {
                fit.window_to_app(self.cursor.x, self.cursor.y)
            }
        }

        fn present(&mut self) {
            let px = self.state.render_viewer_frame();
            let Some(surface) = self.surface.as_mut() else {
                return;
            };
            let w = self.win_phys_w.max(1);
            let h = self.win_phys_h.max(1);
            let (Some(nw), Some(nh)) = (NonZeroU32::new(w), NonZeroU32::new(h)) else {
                return;
            };
            let fit = ViewportFit::compute_fit(w, h, px.size);
            if surface.resize(nw, nh).is_ok() {
                if let Ok(mut buf) = surface.buffer_mut() {
                    fit.blit_to_xrgb(&px, &mut buf, w, h, 0x000E_0B11);
                    let _ = buf.present();
                }
            }
            self.dirty = false;
        }
    }

    impl ApplicationHandler for ViewerApp {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            if self.window.is_some() {
                return;
            }
            let layout = self.state.layout();
            // Prefer 1:1 physical pixels (1088x1648) so text renders pixel-sharp on both
            // 1x Linux/Windows displays and 2x macOS Retina displays, clamping to 88% of
            // the active monitor so it still fits comfortably on 1080p screens.
            let target_w = layout.total_size.w.max(400) as f64;
            let target_h = layout.total_size.h.max(600) as f64;
            let monitor = event_loop
                .primary_monitor()
                .or_else(|| event_loop.available_monitors().next());
            let (phys_w, phys_h) = if let Some(mon) = monitor {
                let ms = mon.size();
                let max_w = (ms.width as f64 * 0.90).max(400.0);
                let max_h = (ms.height as f64 * 0.88).max(600.0);
                let scale = (max_w / target_w).min(max_h / target_h).min(1.0);
                (
                    (target_w * scale).round() as u32,
                    (target_h * scale).round() as u32,
                )
            } else {
                (target_w as u32, target_h as u32)
            };
            let attrs = WindowAttributes::default()
                .with_title("RabbizOS Emulator — Live ARM64 Linux + Host Drivers")
                .with_inner_size(PhysicalSize::new(phys_w, phys_h));
            let win = Arc::new(event_loop.create_window(attrs).expect("create viewer window"));
            let inner = win.inner_size();
            self.win_phys_w = inner.width.max(1);
            self.win_phys_h = inner.height.max(1);
            let ctx = Context::new(win.clone()).expect("softbuffer context");
            let surface = Surface::new(&ctx, win.clone()).expect("softbuffer surface");
            win.request_redraw();
            self.window = Some(win);
            self.surface = Some(surface);
            self.dirty = true;
        }

        fn window_event(
            &mut self,
            event_loop: &ActiveEventLoop,
            _id: WindowId,
            event: WindowEvent,
        ) {
            match event {
                WindowEvent::CloseRequested => event_loop.exit(),
                WindowEvent::Resized(new_size) => {
                    self.win_phys_w = new_size.width.max(1);
                    self.win_phys_h = new_size.height.max(1);
                    self.dirty = true;
                }
                WindowEvent::ScaleFactorChanged { .. } => {
                    if let Some(win) = &self.window {
                        let inner = win.inner_size();
                        self.win_phys_w = inner.width.max(1);
                        self.win_phys_h = inner.height.max(1);
                        self.dirty = true;
                    }
                }
                WindowEvent::CursorMoved { position, .. } => {
                    self.cursor = position;
                    if self.touch_dragging {
                        let layout = self.state.layout();
                        if let Some(pt) = self.cursor_canvas_point(true) {
                            let clamped_pt = Point::new(
                                pt.x.clamp(
                                    layout.phone_screen_rect.x,
                                    layout.phone_screen_rect.x + layout.phone_screen_rect.w - 1,
                                ),
                                pt.y.clamp(
                                    layout.phone_screen_rect.y,
                                    layout.phone_screen_rect.y + layout.phone_screen_rect.h - 1,
                                ),
                            );
                            if let Some(dev_pt) =
                                layout.viewer_to_device(clamped_pt, self.state.preset.size())
                            {
                                self.pending_drag_move = Some(dev_pt);
                            }
                        }
                    }
                }
                WindowEvent::MouseInput {
                    state: btn_state,
                    button: MouseButton::Left,
                    ..
                } => {
                    let layout = self.state.layout();
                    if btn_state == ElementState::Pressed {
                        if let Some(pt) = self.cursor_canvas_point(false) {
                            if let Some(ctrl_btn) = layout.hit_button(pt) {
                                self.state.activate_button(ctrl_btn);
                                let _ = self.state.poll_remote_frame();
                                self.dirty = true;
                            } else if let Some(dev_pt) =
                                layout.viewer_to_device(pt, self.state.preset.size())
                            {
                                self.touch_dragging = true;
                                self.pending_drag_move = None;
                                self.state.inject_touch("down", dev_pt);
                            }
                        }
                    } else {
                        if self.state.pressed_button.take().is_some() {
                            self.dirty = true;
                        }
                        if self.touch_dragging {
                            self.touch_dragging = false;
                            if let Some(move_pt) = self.pending_drag_move.take() {
                                self.state.inject_touch("move", move_pt);
                            }
                            if let Some(pt) = self.cursor_canvas_point(true) {
                                let clamped_pt = Point::new(
                                    pt.x.clamp(
                                        layout.phone_screen_rect.x,
                                        layout.phone_screen_rect.x + layout.phone_screen_rect.w - 1,
                                    ),
                                    pt.y.clamp(
                                        layout.phone_screen_rect.y,
                                        layout.phone_screen_rect.y + layout.phone_screen_rect.h - 1,
                                    ),
                                );
                                if let Some(dev_pt) =
                                    layout.viewer_to_device(clamped_pt, self.state.preset.size())
                                {
                                    self.state.inject_touch("up", dev_pt);
                                }
                            }
                            let _ = self.state.poll_remote_frame();
                            self.dirty = true;
                        }
                    }
                }
                WindowEvent::MouseWheel { delta, .. } => {
                    let dy = match delta {
                        MouseScrollDelta::LineDelta(_, y) => (y * 120.0) as i32,
                        MouseScrollDelta::PixelDelta(pos) => pos.y as i32,
                    };
                    if dy.abs() >= 18 {
                        let layout = self.state.layout();
                        if let Some(pt) = self.cursor_canvas_point(false) {
                            if let Some(dev_pt) =
                                layout.viewer_to_device(pt, self.state.preset.size())
                            {
                                let end_y = (dev_pt.y + dy.clamp(-420, 420)).clamp(80, 1520);
                                self.state.inject_smooth_swipe(
                                    dev_pt,
                                    Point::new(dev_pt.x, end_y),
                                    6,
                                );
                                self.dirty = true;
                            }
                        }
                    }
                }
                WindowEvent::ModifiersChanged(mods) => {
                    self.modifiers = mods.state();
                }
                WindowEvent::KeyboardInput { event, .. } => {
                    if !event.repeat {
                        if let PhysicalKey::Code(code) = event.physical_key {
                            let shortcut_mod = self.modifiers.control_key()
                                || (cfg!(target_os = "macos") && self.modifiers.super_key());
                            if event.state == ElementState::Pressed
                                && shortcut_mod
                                && code == KeyCode::KeyS
                            {
                                if let Ok(path) = self.state.save_phone_screenshot() {
                                    println!("Saved phone screenshot: {}", path.display());
                                }
                                self.dirty = true;
                                return;
                            }
                            if let Some(xkb_keycode) = winit_keycode_to_xkb(code) {
                                let pressed = event.state == ElementState::Pressed;
                                let _ = self.state.send_ctl_json(&serde_json::json!({
                                    "id": 1,
                                    "cmd": "inject_key",
                                    "keycode": xkb_keycode,
                                    "pressed": pressed,
                                }));
                                self.dirty = true;
                            }
                        }
                    }
                }
                WindowEvent::DroppedFile(path) => {
                    match self.state.transfer_dropped_file(&path) {
                        Ok(dest) => println!("Transferred {} -> {dest}", path.display()),
                        Err(err) => {
                            eprintln!("Transfer failed: {err}");
                            self.state.banner = Some(format!("Transfer failed: {err}"));
                        }
                    }
                    self.dirty = true;
                }
                WindowEvent::RedrawRequested => self.present(),
                _ => {}
            }
            if self.dirty {
                if let Some(win) = &self.window {
                    win.request_redraw();
                }
            }
        }

        fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
            let now = Instant::now();
            if now.saturating_duration_since(self.last_poll) >= Duration::from_millis(16) {
                self.last_poll = now;
                if let Some(move_pt) = self.pending_drag_move.take() {
                    self.state.inject_touch("move", move_pt);
                }
                if self.state.poll_remote_frame() {
                    self.dirty = true;
                }
            }
            if self.dirty {
                if let Some(win) = &self.window {
                    win.request_redraw();
                }
            }
            event_loop.set_control_flow(ControlFlow::WaitUntil(now + Duration::from_millis(16)));
        }
    }

    let event_loop = EventLoop::new().expect("viewer event loop");
    let mut app = ViewerApp {
        state,
        window: None,
        surface: None,
        win_phys_w: 1088,
        win_phys_h: 1648,
        cursor: PhysicalPosition::new(0.0, 0.0),
        touch_dragging: false,
        pending_drag_move: None,
        modifiers: ModifiersState::default(),
        dirty: true,
        last_poll: Instant::now(),
    };
    let _ = event_loop.run_app(&mut app);
}
