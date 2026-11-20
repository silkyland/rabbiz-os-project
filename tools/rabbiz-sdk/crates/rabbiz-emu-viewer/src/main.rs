#![forbid(unsafe_code)]

use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
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

    if !self_test && !demo_only {
        ensure_linux_backend_running(&emu_addr);
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

/// Automatically ensure the background ARM64 Linux container (`rabbiz-emu-live`)
/// is running with all 12 real `rabbiz-apps`, `rabbiz-osk`, `.desktop` files, and
/// SVG icons staged when `127.0.0.1:5910` is not yet listening.
fn ensure_linux_backend_running(emu_addr: &str) {
    if let Ok(sock) = emu_addr.parse::<SocketAddr>() {
        if TcpStream::connect_timeout(&sock, Duration::from_millis(80)).is_ok() {
            return;
        }
    } else {
        return;
    }

    let Some(repo) = find_repo_root() else {
        return;
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
        return;
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
        if let Ok(sock) = emu_addr.parse::<SocketAddr>() {
            for _ in 0..30 {
                if TcpStream::connect_timeout(&sock, Duration::from_millis(80)).is_ok() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn run_viewer_window(state: ViewerState) {
    let out = std::env::temp_dir().join("rabbiz-emu-viewer-linux.png");
    if let Err(err) = state.save_viewer_png(&out) {
        eprintln!("rabbiz-emu-viewer error: {err}");
        std::process::exit(1);
    }
    println!("rabbiz-emu-viewer rendered frame to {}", out.display());
}

#[cfg(not(target_os = "linux"))]
fn run_viewer_window(state: ViewerState) {
    use std::num::NonZeroU32;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use de_api::Point;
    use rabbiz_app_kit::sim_config::ViewportFit;
    use softbuffer::{Context, Surface};
    use winit::application::ApplicationHandler;
    use winit::dpi::{LogicalSize, PhysicalPosition};
    use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
    use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
    use winit::keyboard::{Key, NamedKey};
    use winit::window::{Window, WindowAttributes, WindowId};

    struct ViewerApp {
        state: ViewerState,
        window: Option<Arc<Window>>,
        surface: Option<Surface<Arc<Window>, Arc<Window>>>,
        win_phys_w: u32,
        win_phys_h: u32,
        cursor: PhysicalPosition<f64>,
        touch_dragging: bool,
        pending_drag_move: Option<Point>,
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
            // Request logical points equal to half the 1088x1648 physical canvas
            // (544x824 pt) so macOS 2x Retina displays render 1:1 physical pixels!
            let logical_w = (layout.total_size.w / 2).max(400) as u32;
            let logical_h = (layout.total_size.h / 2).max(600) as u32;
            let attrs = WindowAttributes::default()
                .with_title("RabbizOS Emulator — Live ARM64 Linux + Host Drivers")
                .with_inner_size(LogicalSize::new(logical_w, logical_h));
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
                WindowEvent::KeyboardInput { event, .. } => {
                    if event.state == ElementState::Pressed {
                        match &event.logical_key {
                            Key::Named(NamedKey::Escape) => {
                                self.state.activate_button(
                                    rabbiz_emu_viewer::ControlButton::SwipeBack,
                                );
                                self.dirty = true;
                            }
                            Key::Character(ch) => {
                                let s = ch.as_str();
                                if !s.is_empty() && !s.chars().any(char::is_control) {
                                    let _ = self.state.send_ctl_json(&serde_json::json!({
                                        "id": 1,
                                        "cmd": "inject_key",
                                        "keysym": 0,
                                        "text": s,
                                    }));
                                    self.dirty = true;
                                }
                            }
                            _ => {}
                        }
                    }
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
        dirty: true,
        last_poll: Instant::now(),
    };
    let _ = event_loop.run_app(&mut app);
}
