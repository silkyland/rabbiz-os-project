#![forbid(unsafe_code)]

use std::path::PathBuf;

use rabbiz_emu_viewer::ViewerState;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut emu_addr = "127.0.0.1:5910".to_string();
    let mut ctl_addr = "127.0.0.1:5911".to_string();
    let mut self_test = false;
    let mut out_png: Option<PathBuf> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--self-test" => self_test = true,
            "--demo" => {}
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

    let mut state = ViewerState::new(emu_addr, ctl_addr);
    let _ = state.poll_remote_frame();

    if self_test {
        let out = out_png.unwrap_or_else(|| std::env::temp_dir().join("rabbiz-emu-viewer-selftest.png"));
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
    use rabbiz_emu_viewer::pixels_to_xrgb;
    use softbuffer::{Context, Surface};
    use winit::application::ApplicationHandler;
    use winit::dpi::{LogicalSize, PhysicalPosition};
    use winit::event::{ElementState, MouseButton, WindowEvent};
    use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
    use winit::window::{Window, WindowAttributes, WindowId};

    struct ViewerApp {
        state: ViewerState,
        window: Option<Arc<Window>>,
        surface: Option<Surface<Arc<Window>, Arc<Window>>>,
        cursor: PhysicalPosition<f64>,
        touch_dragging: bool,
        dirty: bool,
        last_poll: Instant,
    }

    impl ViewerApp {
        fn present(&mut self) {
            let px = self.state.render_viewer_frame();
            let xrgb = pixels_to_xrgb(&px);
            let Some(surface) = self.surface.as_mut() else {
                return;
            };
            let (Some(nw), Some(nh)) = (
                NonZeroU32::new(px.size.w.max(1) as u32),
                NonZeroU32::new(px.size.h.max(1) as u32),
            ) else {
                return;
            };
            if surface.resize(nw, nh).is_ok() {
                if let Ok(mut buf) = surface.buffer_mut() {
                    let count = buf.len().min(xrgb.len());
                    buf[..count].copy_from_slice(&xrgb[..count]);
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
            let attrs = WindowAttributes::default()
                .with_title("RabbizOS Emulator Viewer")
                .with_inner_size(LogicalSize::new(
                    layout.total_size.w as u32,
                    layout.total_size.h as u32,
                ));
            let win = Arc::new(event_loop.create_window(attrs).expect("create viewer window"));
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
                WindowEvent::CursorMoved { position, .. } => {
                    self.cursor = position;
                    if self.touch_dragging {
                        let layout = self.state.layout();
                        let pt = Point::new(position.x as i32, position.y as i32);
                        if let Some(dev_pt) =
                            layout.viewer_to_device(pt, self.state.preset.size())
                        {
                            self.state.inject_touch("move", dev_pt);
                            self.dirty = true;
                        }
                    }
                }
                WindowEvent::MouseInput {
                    state: btn_state,
                    button: MouseButton::Left,
                    ..
                } => {
                    let layout = self.state.layout();
                    let pt = Point::new(self.cursor.x as i32, self.cursor.y as i32);
                    if btn_state == ElementState::Pressed {
                        if let Some(ctrl_btn) = layout.hit_button(pt) {
                            self.state.activate_button(ctrl_btn);
                            self.dirty = true;
                        } else if let Some(dev_pt) =
                            layout.viewer_to_device(pt, self.state.preset.size())
                        {
                            self.touch_dragging = true;
                            self.state.inject_touch("down", dev_pt);
                            self.dirty = true;
                        }
                    } else {
                        self.state.pressed_button = None;
                        if self.touch_dragging {
                            self.touch_dragging = false;
                            if let Some(dev_pt) =
                                layout.viewer_to_device(pt, self.state.preset.size())
                            {
                                self.state.inject_touch("up", dev_pt);
                            }
                            self.dirty = true;
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
            if now.saturating_duration_since(self.last_poll) >= Duration::from_millis(33) {
                self.last_poll = now;
                if self.state.poll_remote_frame() {
                    self.dirty = true;
                }
            }
            if self.dirty {
                if let Some(win) = &self.window {
                    win.request_redraw();
                }
            }
            event_loop.set_control_flow(ControlFlow::WaitUntil(now + Duration::from_millis(33)));
        }
    }

    let event_loop = EventLoop::new().expect("viewer event loop");
    let mut app = ViewerApp {
        state,
        window: None,
        surface: None,
        cursor: PhysicalPosition::new(0.0, 0.0),
        touch_dragging: false,
        dirty: true,
        last_poll: Instant::now(),
    };
    let _ = event_loop.run_app(&mut app);
}
