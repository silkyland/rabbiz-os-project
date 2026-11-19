//! Native Wayland (`xdg_toplevel` + `wl_shm` + `wl_touch` + `wl_pointer` +
//! `wl_keyboard` + `zwp_text_input_v3`) window runner for `rabbiz-*` apps.

use std::collections::HashMap;
use std::os::fd::AsRawFd;
use std::time::{Duration, Instant};

use de_api::{Point, Size};
use de_design::tokens::Theme;
use de_paint::Canvas;
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_keyboard, delegate_output, delegate_pointer, delegate_registry,
    delegate_seat, delegate_shm, delegate_touch, delegate_xdg_shell, delegate_xdg_window,
    output::{OutputHandler, OutputState},
    reexports::client::{
        globals::registry_queue_init,
        protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface, wl_touch},
        Connection, Dispatch, QueueHandle,
    },
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers},
        pointer::{PointerEvent, PointerEventKind, PointerHandler},
        touch::TouchHandler,
        Capability, SeatHandler, SeatState,
    },
    shell::{
        xdg::{
            window::{Window, WindowConfigure, WindowDecorations, WindowHandler},
            XdgShell,
        },
        WaylandSurface,
    },
    shm::{slot::SlotPool, Shm, ShmHandler},
};
use wayland_protocols::wp::text_input::zv3::client::{
    zwp_text_input_manager_v3::ZwpTextInputManagerV3,
    zwp_text_input_v3::{ContentHint, ContentPurpose, Event as TiEvent, ZwpTextInputV3},
};

/// evdev `BTN_LEFT` — the only pointer button routed through Touch* events;
/// every other button surfaces as `AppEvent::PointerButton`.
const BTN_LEFT: u32 = 0x110;

#[derive(Debug, Clone)]
pub enum AppEvent {
    TouchDown {
        id: i32,
        pos: Point,
    },
    TouchMove {
        id: i32,
        pos: Point,
    },
    TouchUp {
        id: i32,
        pos: Point,
    },
    /// Mouse hover motion with no button held (touch never produces this).
    PointerMove {
        pos: Point,
    },
    /// Mouse wheel / touchpad scroll, in surface pixels (`wl_pointer` axis
    /// `absolute` values; positive dy scrolls content down).
    Scroll {
        dx: f32,
        dy: f32,
    },
    /// Non-primary mouse button edge. `button` is the evdev code
    /// (0x110 left is routed to Touch* instead; right = 0x111, middle =
    /// 0x112, others pass through).
    PointerButton {
        button: u32,
        pressed: bool,
        pos: Point,
    },
    Key {
        keysym: u32,
        utf8: Option<String>,
    },
    TextCommit(String),
    TextBackspace(usize),
    Tick(Duration),
}

#[derive(Default)]
pub struct AppFrame {
    pub dirty: bool,
    pub animating: bool,
    pub quit: bool,
}

pub struct TextInputCtl {
    ti: Option<ZwpTextInputV3>,
    pending_commit: Option<String>,
    pending_delete_before: u32,
}

impl TextInputCtl {
    fn new(ti: Option<ZwpTextInputV3>) -> Self {
        Self {
            ti,
            pending_commit: None,
            pending_delete_before: 0,
        }
    }

    pub fn activate(&self, purpose: ContentPurpose) {
        if let Some(ti) = &self.ti {
            ti.enable();
            ti.set_content_type(ContentHint::None, purpose);
            ti.commit();
        }
    }

    pub fn set_surrounding(&self, text: &str, cursor_bytes: usize) {
        if let Some(ti) = &self.ti {
            let mut end = text.len().min(3000);
            while end > 0 && !text.is_char_boundary(end) {
                end -= 1;
            }
            let slice = &text[..end];
            let cur = cursor_bytes.min(slice.len()) as i32;
            ti.set_surrounding_text(slice.to_string(), cur, cur);
            ti.commit();
        }
    }

    pub fn deactivate(&self) {
        if let Some(ti) = &self.ti {
            ti.disable();
            ti.commit();
        }
    }
}

pub trait AppState {
    fn app_id(&self) -> &'static str;
    fn title(&self) -> &'static str;
    fn handle_event(&mut self, event: AppEvent, ti: &TextInputCtl) -> AppFrame;
    fn render(&mut self, canvas: &mut Canvas, theme: &Theme);
}

struct Runner<S: AppState> {
    reg: RegistryState,
    seats: SeatState,
    out: OutputState,
    shm: Shm,
    pool: SlotPool,
    window: Window,
    ti_mgr: Option<ZwpTextInputManagerV3>,
    ti: TextInputCtl,
    ti_focused: bool,
    w: u32,
    h: u32,
    configured: bool,
    dirty: bool,
    animating: bool,
    quit: bool,
    touches: HashMap<i32, Point>,
    pointer_pos: Point,
    pointer_down: bool,
    theme: Theme,
    state: S,
    last_tick: Instant,
}

impl<S: AppState> Runner<S> {
    fn draw(&mut self) {
        if !self.configured {
            return;
        }
        let w = self.w.max(1);
        let h = self.h.max(1);
        let mut canvas = Canvas::filled(Size::new(w as i32, h as i32), self.theme.bg);
        self.state.render(&mut canvas, &self.theme);
        let px = canvas.into_pixels();

        let stride = (w * 4) as i32;
        if let Ok((buf, dst)) =
            self.pool
                .create_buffer(w as i32, h as i32, stride, wl_shm::Format::Argb8888)
        {
            for (d, s) in dst
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .zip(px.data.as_chunks::<4>().0.iter())
            {
                d[0] = s[2];
                d[1] = s[1];
                d[2] = s[0];
                d[3] = s[3];
            }
            let surf = self.window.wl_surface();
            surf.damage_buffer(0, 0, w as i32, h as i32);
            if buf.attach_to(surf).is_ok() {
                surf.commit();
            }
        }
        self.dirty = false;
    }

    fn dispatch_app(&mut self, ev: AppEvent) {
        let frame = self.state.handle_event(ev, &self.ti);
        if frame.dirty {
            self.dirty = true;
        }
        self.animating = frame.animating;
        if frame.quit {
            self.quit = true;
        }
    }
}

pub fn run_app<S: AppState + 'static>(state: S) {
    let conn = Connection::connect_to_env().expect("Wayland session required");
    let (globals, mut queue) = registry_queue_init(&conn).expect("registry");
    let qh = queue.handle();

    let compositor = CompositorState::bind(&globals, &qh).expect("wl_compositor");
    let xdg_shell = XdgShell::bind(&globals, &qh).expect("xdg_wm_base");
    let shm = Shm::bind(&globals, &qh).expect("wl_shm");
    let ti_mgr = globals
        .bind::<ZwpTextInputManagerV3, Runner<S>, ()>(&qh, 1..=1, ())
        .ok();

    let surface = compositor.create_surface(&qh);
    let window = xdg_shell.create_window(surface, WindowDecorations::ServerDefault, &qh);
    window.set_title(state.title());
    window.set_app_id(state.app_id());
    window.commit();

    let w = 720u32;
    let h = 1600u32;
    let pool = SlotPool::new((w * h * 4 * 2) as usize, &shm).expect("shm pool");

    let mut runner = Runner {
        reg: RegistryState::new(&globals),
        seats: SeatState::new(&globals, &qh),
        out: OutputState::new(&globals, &qh),
        shm,
        pool,
        window,
        ti_mgr,
        ti: TextInputCtl::new(None),
        ti_focused: false,
        w,
        h,
        configured: false,
        dirty: true,
        animating: false,
        quit: false,
        touches: HashMap::new(),
        pointer_pos: Point::ZERO,
        pointer_down: false,
        theme: crate::load_theme(),
        state,
        last_tick: Instant::now(),
    };

    // While `animating` is set, `blocking_dispatch` would sleep until the next
    // Wayland event and Tick animations would freeze between touches. Poll the
    // connection fd with a bounded timeout instead so Tick keeps a steady
    // cadence; idle apps still block as before.
    let backend = conn.backend();
    while !runner.quit {
        if runner.animating && runner.configured {
            let mut pfd = libc::pollfd {
                fd: backend.poll_fd().as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            let _ = unsafe { libc::poll(&mut pfd, 1, 8) };
            if pfd.revents & libc::POLLIN != 0 {
                let _ = queue.blocking_dispatch(&mut runner);
            } else {
                let _ = queue.dispatch_pending(&mut runner);
            }
        } else {
            let _ = queue.blocking_dispatch(&mut runner);
        }
        let now = Instant::now();
        let dt = now.saturating_duration_since(runner.last_tick);
        runner.last_tick = now;
        if runner.animating {
            runner.dispatch_app(AppEvent::Tick(dt));
        }
        if runner.dirty && runner.configured {
            runner.draw();
        }
        let _ = conn.flush();
    }
}

impl<S: AppState> CompositorHandler for Runner<S> {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: i32,
    ) {
    }
    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}
    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl<S: AppState> OutputHandler for Runner<S> {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.out
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl<S: AppState> WindowHandler for Runner<S> {
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Window) {
        self.quit = true;
    }
    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &Window,
        cfg: WindowConfigure,
        _: u32,
    ) {
        if let Some(w) = cfg.new_size.0 {
            self.w = w.get();
        }
        if let Some(h) = cfg.new_size.1 {
            self.h = h.get();
        }
        let _ = self.pool.resize((self.w * self.h * 4 * 2) as usize);
        self.configured = true;
        self.dirty = true;
        self.draw();
    }
}

impl<S: AppState> ShmHandler for Runner<S> {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl<S: AppState + 'static> ProvidesRegistryState for Runner<S> {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.reg
    }
    registry_handlers![OutputState, SeatState];
}

impl<S: AppState + 'static> SeatHandler for Runner<S> {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seats
    }
    fn new_seat(&mut self, _: &Connection, qh: &QueueHandle<Self>, seat: wl_seat::WlSeat) {
        if self.ti.ti.is_none() {
            if let Some(mgr) = &self.ti_mgr {
                let ti = mgr.get_text_input(&seat, qh, ());
                self.ti = TextInputCtl::new(Some(ti));
            }
        }
    }
    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        cap: Capability,
    ) {
        match cap {
            Capability::Touch => {
                let _ = self.seats.get_touch(qh, &seat);
            }
            Capability::Pointer => {
                let _ = self.seats.get_pointer(qh, &seat);
            }
            Capability::Keyboard => {
                let _ = self.seats.get_keyboard(qh, &seat, None);
            }
            _ => {}
        }
    }
    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        _: Capability,
    ) {
    }
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl<S: AppState> TouchHandler for Runner<S> {
    fn down(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_touch::WlTouch,
        _: u32,
        _: u32,
        _: wl_surface::WlSurface,
        id: i32,
        pos: (f64, f64),
    ) {
        let p = Point::new(pos.0 as i32, pos.1 as i32);
        self.touches.insert(id, p);
        self.dispatch_app(AppEvent::TouchDown { id, pos: p });
    }
    fn motion(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_touch::WlTouch,
        _: u32,
        id: i32,
        pos: (f64, f64),
    ) {
        let p = Point::new(pos.0 as i32, pos.1 as i32);
        self.touches.insert(id, p);
        self.dispatch_app(AppEvent::TouchMove { id, pos: p });
    }
    fn up(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_touch::WlTouch,
        _: u32,
        _: u32,
        id: i32,
    ) {
        let p = self.touches.remove(&id).unwrap_or(Point::ZERO);
        self.dispatch_app(AppEvent::TouchUp { id, pos: p });
    }
    fn shape(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_touch::WlTouch,
        _: i32,
        _: f64,
        _: f64,
    ) {
    }
    fn orientation(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_touch::WlTouch,
        _: i32,
        _: f64,
    ) {
    }
    fn cancel(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_touch::WlTouch) {
        let ids: Vec<i32> = self.touches.keys().copied().collect();
        for id in ids {
            if let Some(p) = self.touches.remove(&id) {
                self.dispatch_app(AppEvent::TouchUp { id, pos: p });
            }
        }
    }
}

impl<S: AppState> PointerHandler for Runner<S> {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for ev in events {
            let p = Point::new(ev.position.0 as i32, ev.position.1 as i32);
            match ev.kind {
                PointerEventKind::Motion { .. } => {
                    self.pointer_pos = p;
                    if self.pointer_down {
                        self.dispatch_app(AppEvent::TouchMove { id: -1, pos: p });
                    } else {
                        self.dispatch_app(AppEvent::PointerMove { pos: p });
                    }
                }
                PointerEventKind::Press { button, .. } => {
                    self.pointer_pos = p;
                    if button == BTN_LEFT {
                        self.pointer_down = true;
                        self.dispatch_app(AppEvent::TouchDown { id: -1, pos: p });
                    } else {
                        self.dispatch_app(AppEvent::PointerButton {
                            button,
                            pressed: true,
                            pos: p,
                        });
                    }
                }
                PointerEventKind::Release { button, .. } => {
                    self.pointer_pos = p;
                    if button == BTN_LEFT {
                        self.pointer_down = false;
                        self.dispatch_app(AppEvent::TouchUp { id: -1, pos: p });
                    } else {
                        self.dispatch_app(AppEvent::PointerButton {
                            button,
                            pressed: false,
                            pos: p,
                        });
                    }
                }
                PointerEventKind::Axis {
                    horizontal,
                    vertical,
                    ..
                } => {
                    self.dispatch_app(AppEvent::Scroll {
                        dx: horizontal.absolute as f32,
                        dy: vertical.absolute as f32,
                    });
                }
                _ => {}
            }
        }
    }
}

impl<S: AppState> KeyboardHandler for Runner<S> {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
    }
    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
    ) {
    }
    fn press_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        self.dispatch_app(AppEvent::Key {
            keysym: event.keysym.raw(),
            utf8: event.utf8,
        });
    }
    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: KeyEvent,
    ) {
    }
    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: Modifiers,
        _: u32,
    ) {
    }
}

impl<S: AppState> Dispatch<ZwpTextInputManagerV3, ()> for Runner<S> {
    fn event(
        _: &mut Self,
        _: &ZwpTextInputManagerV3,
        _: wayland_protocols::wp::text_input::zv3::client::zwp_text_input_manager_v3::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl<S: AppState> Dispatch<ZwpTextInputV3, ()> for Runner<S> {
    fn event(
        state: &mut Self,
        _: &ZwpTextInputV3,
        event: TiEvent,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            TiEvent::Enter { .. } => {
                state.ti_focused = true;
            }
            TiEvent::Leave { .. } => {
                state.ti_focused = false;
            }
            TiEvent::CommitString { text } => {
                state.ti.pending_commit = text;
            }
            TiEvent::DeleteSurroundingText { before_length, .. } => {
                state.ti.pending_delete_before = before_length;
            }
            TiEvent::Done { .. } => {
                let del = std::mem::take(&mut state.ti.pending_delete_before);
                let commit = state.ti.pending_commit.take();
                if del > 0 {
                    state.dispatch_app(AppEvent::TextBackspace(del as usize));
                }
                if let Some(txt) = commit {
                    if !txt.is_empty() {
                        state.dispatch_app(AppEvent::TextCommit(txt));
                    }
                }
            }
            _ => {}
        }
    }
}

delegate_compositor!(@<S: AppState + 'static> Runner<S>);
delegate_output!(@<S: AppState + 'static> Runner<S>);
delegate_shm!(@<S: AppState + 'static> Runner<S>);
delegate_seat!(@<S: AppState + 'static> Runner<S>);
delegate_touch!(@<S: AppState + 'static> Runner<S>);
delegate_pointer!(@<S: AppState + 'static> Runner<S>);
delegate_keyboard!(@<S: AppState + 'static> Runner<S>);
delegate_xdg_shell!(@<S: AppState + 'static> Runner<S>);
delegate_xdg_window!(@<S: AppState + 'static> Runner<S>);
delegate_registry!(@<S: AppState + 'static> Runner<S>);
