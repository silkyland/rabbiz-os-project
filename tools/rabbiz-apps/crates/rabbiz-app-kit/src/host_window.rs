//! Tier 1 Native Host App Simulator (`winit` + `softbuffer`) for non-Linux
//! hosts (macOS / Windows / BSD).
//!
//! Replaces the inert stub so every `rabbiz-*` app opens interactively at
//! native speed with touch/drag simulation, mouse scroll, macOS/host IME
//! (including Thai), safe-area chrome, mock OSK overlay, and PNG capture.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use de_api::Pixels;
use de_paint::Canvas;
use softbuffer::{Context, Surface};
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::ContentPurpose;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, Ime, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{Window, WindowAttributes, WindowId};

pub use crate::app::{AppEvent, AppFrame, AppState, BTN_LEFT};
use crate::app::{BTN_MIDDLE, BTN_RIGHT};
use crate::sim_config::{
    char_to_xkb, draw_sim_chrome, filter_key_utf8, SimOptions, ViewportFit, BEZEL_XRGB,
    XKB_BACKSPACE, XKB_DELETE, XKB_DOWN, XKB_END, XKB_ESCAPE, XKB_HOME, XKB_LEFT, XKB_PAGE_DOWN,
    XKB_PAGE_UP, XKB_RETURN, XKB_RIGHT, XKB_TAB, XKB_UP,
};
use crate::theme_watch::ThemeWatcher;

/// Client text-input handle; tracks IME active state and purpose for the
/// simulator window and `TestHarness`.
#[derive(Debug)]
pub struct TextInputCtl {
    active: Cell<bool>,
    purpose: Cell<Option<ContentPurpose>>,
    changed: Cell<bool>,
}

impl Default for TextInputCtl {
    fn default() -> Self {
        Self::headless()
    }
}

impl TextInputCtl {
    /// Create an inert [`TextInputCtl`] for headless tests and host runs.
    #[must_use]
    pub const fn headless() -> Self {
        Self {
            active: Cell::new(false),
            purpose: Cell::new(None),
            changed: Cell::new(false),
        }
    }

    pub fn activate(&self, purpose: ContentPurpose) {
        if !self.active.get() || self.purpose.get() != Some(purpose) {
            self.changed.set(true);
        }
        self.active.set(true);
        self.purpose.set(Some(purpose));
    }

    pub fn set_surrounding(&self, _text: &str, _cursor_bytes: usize) {}

    pub fn deactivate(&self) {
        if self.active.get() {
            self.changed.set(true);
        }
        self.active.set(false);
        self.purpose.set(None);
    }

    /// Whether the app currently expects text-input / OSK focus.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.active.get()
    }

    /// Active [`ContentPurpose`] passed to [`Self::activate`], if active.
    #[must_use]
    pub fn purpose(&self) -> Option<ContentPurpose> {
        self.purpose.get()
    }

    fn take_changed(&self) -> bool {
        self.changed.replace(false)
    }
}

/// Launch `state` inside a native Tier 1 host simulator window.
pub fn run_app<S: AppState + 'static>(state: S) {
    let opts = SimOptions::from_env();
    eprintln!(
        "RabbizOS Sim ({}, {}) | Cmd+L theme | Cmd+K OSK | Cmd+R preset | Cmd+B chrome | Cmd+S shot | Cmd+Q quit",
        opts.preset.short_label(),
        opts.preset.form_factor()
    );

    let event_loop = EventLoop::new().expect("create winit event loop");
    let mut sim = HostSimApp::new(state, opts);
    if let Err(err) = event_loop.run_app(&mut sim) {
        eprintln!("rabbiz-app-kit simulator error: {err}");
    }
}

struct HostSimApp<S: AppState> {
    state: S,
    opts: SimOptions,
    ti: TextInputCtl,
    theme_watcher: ThemeWatcher,
    window: Option<Arc<Window>>,
    surface: Option<Surface<Arc<Window>, Arc<Window>>>,
    win_phys_w: u32,
    win_phys_h: u32,
    cursor_phys: PhysicalPosition<f64>,
    left_touch_active: bool,
    modifiers: ModifiersState,
    dirty: bool,
    animating: bool,
    quit: bool,
    last_tick: Instant,
    last_theme_poll: Instant,
}

impl<S: AppState> HostSimApp<S> {
    fn new(state: S, opts: SimOptions) -> Self {
        let now = Instant::now();
        Self {
            state,
            opts,
            ti: TextInputCtl::headless(),
            theme_watcher: ThemeWatcher::new(),
            window: None,
            surface: None,
            win_phys_w: 1,
            win_phys_h: 1,
            cursor_phys: PhysicalPosition::new(0.0, 0.0),
            left_touch_active: false,
            modifiers: ModifiersState::empty(),
            dirty: true,
            animating: false,
            quit: false,
            last_tick: now,
            last_theme_poll: now,
        }
    }

    fn active_theme_and_mode(&self) -> (&de_design::tokens::Theme, &'static str) {
        if let Some(mode) = &self.opts.theme_override {
            if mode == "dark" {
                return (&de_design::tokens::DARK, "dark");
            }
            return (&de_design::tokens::LIGHT, "light");
        }
        let mode = if self.theme_watcher.mode() == "dark" {
            "dark"
        } else {
            "light"
        };
        (self.theme_watcher.current(), mode)
    }

    fn window_title(&self) -> String {
        format!(
            "{} — RabbizOS Sim ({})",
            self.state.title(),
            self.opts.preset.short_label()
        )
    }

    fn viewport_fit(&self) -> ViewportFit {
        ViewportFit::compute_fit(self.win_phys_w, self.win_phys_h, self.opts.preset.size())
    }

    fn dispatch(&mut self, event: AppEvent) {
        let frame = self.state.handle_event(event, &self.ti);
        if frame.dirty {
            self.dirty = true;
        }
        self.animating = frame.animating;
        if frame.quit {
            self.quit = true;
        }
    }

    fn render_app_pixels(&mut self, include_chrome: bool) -> Pixels {
        let size = self.opts.preset.size();
        let (theme, mode) = {
            let (t, m) = self.active_theme_and_mode();
            (*t, m)
        };
        let mut canvas = Canvas::filled(size, theme.bg);
        self.state.render(&mut canvas, &theme);
        if include_chrome {
            draw_sim_chrome(
                &mut canvas,
                &theme,
                self.opts.preset,
                mode,
                self.ti.is_active(),
                self.opts.mock_osk,
            );
        }
        canvas.into_pixels()
    }

    fn present_frame(&mut self) {
        let px = self.render_app_pixels(self.opts.show_chrome);
        let fit = self.viewport_fit();
        let (w, h) = (self.win_phys_w.max(1), self.win_phys_h.max(1));

        let Some(surface) = self.surface.as_mut() else {
            return;
        };
        let (Some(nw), Some(nh)) = (std::num::NonZeroU32::new(w), std::num::NonZeroU32::new(h))
        else {
            return;
        };
        if let Err(err) = surface.resize(nw, nh) {
            eprintln!("rabbiz-app-kit sim surface resize error: {err}");
            return;
        }
        match surface.buffer_mut() {
            Ok(mut buf) => {
                fit.blit_to_xrgb(&px, &mut buf, w, h, BEZEL_XRGB);
                if let Err(err) = buf.present() {
                    eprintln!("rabbiz-app-kit sim present error: {err}");
                }
            }
            Err(err) => eprintln!("rabbiz-app-kit sim buffer error: {err}"),
        }
        self.dirty = false;
    }

    fn toggle_theme(&mut self) {
        let (_, cur_mode) = self.active_theme_and_mode();
        let next = if cur_mode == "dark" { "light" } else { "dark" };
        self.opts.theme_override = Some(next.to_string());
        self.dirty = true;
        eprintln!("RabbizOS Sim: theme -> {next}");
    }

    fn cycle_preset(&mut self) {
        self.opts.preset = self.opts.preset.cycle();
        let (lw, lh) = self.opts.preset.initial_logical_window_size();
        let title = self.window_title();
        if let Some(win) = &self.window {
            let _ = win.request_inner_size(LogicalSize::new(lw, lh));
            win.set_title(&title);
        }
        self.dirty = true;
        eprintln!("RabbizOS Sim: preset -> {}", self.opts.preset.short_label());
    }

    fn save_screenshot(&mut self) {
        let px = self.render_app_pixels(false);
        let path = screenshot_output_path(&self.opts.shot_dir, self.state.app_id());
        if let Some(parent) = path.parent() {
            if let Err(err) = std::fs::create_dir_all(parent) {
                eprintln!("RabbizOS Sim: mkdir {} failed: {err}", parent.display());
                return;
            }
        }
        match de_paint::encode_png(&px).map_err(|e| e.to_string()) {
            Ok(bytes) => {
                if let Err(err) = std::fs::write(&path, bytes) {
                    eprintln!("RabbizOS Sim: write {} failed: {err}", path.display());
                } else {
                    eprintln!("RabbizOS Sim: saved screenshot {}", path.display());
                }
            }
            Err(err) => eprintln!("RabbizOS Sim: encode PNG failed: {err}"),
        }
    }

    fn handle_shortcut(&mut self, event: &winit::event::KeyEvent) -> bool {
        if !shortcut_modifier_held(self.modifiers) {
            return false;
        }
        let Key::Character(ch) = &event.logical_key else {
            return false;
        };
        match ch.to_ascii_lowercase().as_str() {
            "l" => self.toggle_theme(),
            "k" => {
                self.opts.mock_osk = !self.opts.mock_osk;
                self.dirty = true;
                eprintln!("RabbizOS Sim: mock OSK -> {}", self.opts.mock_osk);
            }
            "r" => self.cycle_preset(),
            "b" => {
                self.opts.show_chrome = !self.opts.show_chrome;
                self.dirty = true;
            }
            "s" => self.save_screenshot(),
            "q" | "w" => self.quit = true,
            _ => return false,
        }
        true
    }
}

impl<S: AppState + 'static> ApplicationHandler for HostSimApp<S> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let (lw, lh) = self.opts.preset.initial_logical_window_size();
        let attrs = WindowAttributes::default()
            .with_title(self.window_title())
            .with_inner_size(LogicalSize::new(lw, lh));
        let win = Arc::new(
            event_loop
                .create_window(attrs)
                .expect("create simulator window"),
        );
        let phys = win.inner_size();
        self.win_phys_w = phys.width.max(1);
        self.win_phys_h = phys.height.max(1);
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
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                self.quit = true;
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                self.win_phys_w = size.width.max(1);
                self.win_phys_h = size.height.max(1);
                self.dirty = true;
                if let Some(win) = &self.window {
                    win.request_redraw();
                }
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(win) = &self.window {
                    let size = win.inner_size();
                    self.win_phys_w = size.width.max(1);
                    self.win_phys_h = size.height.max(1);
                    win.request_redraw();
                }
                self.dirty = true;
            }
            WindowEvent::ModifiersChanged(mods) => {
                self.modifiers = mods.state();
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor_phys = position;
                let fit = self.viewport_fit();
                if self.left_touch_active {
                    let pos = fit.window_to_app_clamped(position.x, position.y);
                    self.dispatch(AppEvent::TouchMove { id: -1, pos });
                } else if let Some(pos) = fit.window_to_app(position.x, position.y) {
                    self.dispatch(AppEvent::PointerMove { pos });
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let fit = self.viewport_fit();
                let pressed = state == ElementState::Pressed;
                match button {
                    MouseButton::Left => {
                        if pressed {
                            if let Some(pos) =
                                fit.window_to_app(self.cursor_phys.x, self.cursor_phys.y)
                            {
                                self.left_touch_active = true;
                                self.dispatch(AppEvent::TouchDown { id: -1, pos });
                            }
                        } else if self.left_touch_active {
                            self.left_touch_active = false;
                            let pos = fit
                                .window_to_app_clamped(self.cursor_phys.x, self.cursor_phys.y);
                            self.dispatch(AppEvent::TouchUp { id: -1, pos });
                        }
                    }
                    MouseButton::Right | MouseButton::Middle => {
                        if let Some(pos) =
                            fit.window_to_app(self.cursor_phys.x, self.cursor_phys.y)
                        {
                            let evdev = if button == MouseButton::Right {
                                BTN_RIGHT
                            } else {
                                BTN_MIDDLE
                            };
                            self.dispatch(AppEvent::PointerButton {
                                button: evdev,
                                pressed,
                                pos,
                            });
                        }
                    }
                    _ => {}
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let (dx, dy) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (-x * 48.0, -y * 48.0),
                    MouseScrollDelta::PixelDelta(p) => (-p.x as f32, -p.y as f32),
                };
                self.dispatch(AppEvent::Scroll { dx, dy });
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed {
                    if self.handle_shortcut(&event) {
                        if let Some(win) = &self.window {
                            win.request_redraw();
                        }
                        if self.quit {
                            event_loop.exit();
                        }
                        return;
                    }
                    if let Some((keysym, utf8)) = winit_key_to_app(&event) {
                        self.dispatch(AppEvent::Key { keysym, utf8 });
                    }
                }
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                if !text.is_empty() {
                    self.dispatch(AppEvent::TextCommit(text));
                }
            }
            WindowEvent::RedrawRequested => {
                self.present_frame();
            }
            _ => {}
        }

        if self.quit {
            event_loop.exit();
        } else if self.dirty {
            if let Some(win) = &self.window {
                win.request_redraw();
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.quit {
            event_loop.exit();
            return;
        }

        if self.ti.take_changed() {
            if let Some(win) = &self.window {
                win.set_ime_allowed(self.ti.is_active());
            }
            if self.opts.show_chrome {
                self.dirty = true;
            }
        }

        let now = Instant::now();
        let dt = now.saturating_duration_since(self.last_tick);
        self.last_tick = now;

        if self.animating || self.app.needs_tick() {
            self.dispatch(AppEvent::Tick(dt));
        }

        if self.opts.theme_override.is_none()
            && now.saturating_duration_since(self.last_theme_poll) >= Duration::from_millis(500)
        {
            self.last_theme_poll = now;
            if self.theme_watcher.poll() {
                self.dirty = true;
            }
        }

        if self.quit {
            event_loop.exit();
            return;
        }

        if self.dirty {
            if let Some(win) = &self.window {
                win.request_redraw();
            }
        }

        let wait_dur = if self.animating || self.app.needs_tick() {
            Duration::from_millis(16)
        } else {
            Duration::from_millis(250)
        };
        event_loop.set_control_flow(ControlFlow::WaitUntil(now + wait_dur));
    }
}

fn shortcut_modifier_held(mods: ModifiersState) -> bool {
    if cfg!(target_os = "macos") {
        mods.super_key() && !mods.control_key() && !mods.alt_key()
    } else {
        mods.control_key() && mods.shift_key() && !mods.alt_key()
    }
}

fn winit_key_to_app(event: &winit::event::KeyEvent) -> Option<(u32, Option<String>)> {
    let raw_text = event.text.as_ref().map(|s| s.as_str());
    let keysym = match &event.logical_key {
        Key::Named(named) => match named {
            NamedKey::Backspace => XKB_BACKSPACE,
            NamedKey::Tab => XKB_TAB,
            NamedKey::Enter => XKB_RETURN,
            NamedKey::Escape => XKB_ESCAPE,
            NamedKey::Delete => XKB_DELETE,
            NamedKey::Home => XKB_HOME,
            NamedKey::ArrowLeft => XKB_LEFT,
            NamedKey::ArrowUp => XKB_UP,
            NamedKey::ArrowRight => XKB_RIGHT,
            NamedKey::ArrowDown => XKB_DOWN,
            NamedKey::PageUp => XKB_PAGE_UP,
            NamedKey::PageDown => XKB_PAGE_DOWN,
            NamedKey::End => XKB_END,
            NamedKey::Space => char_to_xkb(' '),
            _ => return None,
        },
        Key::Character(s) => {
            let ch = s.chars().next()?;
            char_to_xkb(ch)
        }
        _ => return None,
    };

    let utf8 = match &event.logical_key {
        Key::Named(NamedKey::Space) => Some(" ".to_string()),
        Key::Character(s) => filter_key_utf8(keysym, raw_text.or(Some(s.as_str()))),
        Key::Named(_) => filter_key_utf8(keysym, raw_text),
        _ => None,
    };
    Some((keysym, utf8))
}

fn screenshot_output_path(dir: &Path, app_id: &str) -> PathBuf {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let safe_id: String = app_id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    dir.join(format!("{safe_id}-{ms}.png"))
}
