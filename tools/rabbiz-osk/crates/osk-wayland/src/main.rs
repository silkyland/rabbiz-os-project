//! `rabbiz-osk` — the real keyboard: a bottom layer-shell surface that draws `osk-core` layouts
//! with `osk-render` and delivers text through `zwp_input_method_v2`.
//!
//! It never takes keyboard focus; text goes only through the input-method protocol, so no
//! keymap tricks are needed for Thai or emoji. squeekboard must not run at the same time.
//!
//! Compatibility: it also owns the `sm.puri.OSK0` D-Bus name (SetVisible/Visible) so the shell's
//! Quick Settings tiles and `rabbiz-osk-ctl` keep working, plus `org.rabbiz.OSK1.ReloadTheme`.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Command as ProcessCommand, Stdio};
use std::sync::Mutex;
use std::os::fd::AsRawFd;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use osk_core::gesture::{GestureDetector, GestureEvent, TouchEvent};
use osk_core::layout::{KeyAction, KeyPos, Layout};
use osk_core::state::{Command, InputPurpose, Keyboard};
use osk_render::{sheet_row_rect, suggestion_rect, toolbar_button_rect, AltBar, Renderer, Theme, TOOLBAR_ICONS};

use smithay_client_toolkit::reexports::client::{
    globals::registry_queue_init,
    protocol::{wl_output, wl_seat, wl_shm, wl_surface, wl_touch},
    Connection, Dispatch, Proxy, QueueHandle,
};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_layer, delegate_output, delegate_registry, delegate_seat,
    delegate_shm, delegate_touch,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{touch::TouchHandler, Capability, SeatHandler, SeatState},
    shell::{
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
        WaylandSurface,
    },
    shm::{
        slot::{Buffer, SlotPool},
        Shm, ShmHandler,
    },
};

use wayland_protocols_misc::zwp_input_method_v2::client::{
    zwp_input_method_manager_v2::ZwpInputMethodManagerV2,
    zwp_input_method_v2::{Event as ImEvent, ZwpInputMethodV2},
};
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::{
    zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1,
    zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1,
};


struct AltState {
    key: KeyPos,
    items: Vec<String>,
    selected: usize,
}

struct App {
    reg: RegistryState,
    seats: SeatState,
    out: OutputState,
    shm: Shm,
    pool: SlotPool,
    layer: LayerSurface,
    im: ZwpInputMethodV2,
    /// `zwp_virtual_keyboard_v1` carries the keys an input method cannot express as text:
    /// Enter, and later arrows.
    virtual_keyboard: ZwpVirtualKeyboardV1,
    /// Number of `done` events so far; `commit(serial)` takes exactly this number.
    done_count: u32,
    keyboard: Keyboard,
    gesture: GestureDetector,
    renderer: Renderer,
    theme: Theme,
    active: bool,
    user_visible: bool,
    configured: bool,
    visible: bool,
    /// Set when we asked the compositor for a fresh configure after being unmapped.
    draw_pending: bool,
    w: u32,
    h: u32,
    held: Option<Buffer>,
    pressed: Vec<KeyPos>,
    alt: Option<AltState>,
    touch_pos: HashMap<i32, (f32, f32)>,
    conn: Connection,
    start: Instant,
    predictor_saved: Instant,
    /// K5: last selections seen by `wl-paste --watch`; memory only, never written to disk.
    clipboard: Arc<Mutex<Vec<String>>>,
    clipboard_changed: Arc<AtomicBool>,
    sheet: Option<Vec<String>>,
}

impl App {
    fn now_ms(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }

    fn layer_name(&self) -> String {
        self.keyboard.layer_name().to_string()
    }

    /// Surface height follows the active layout's base layer; the emoji/numbers rows stretch.
    fn sync_size(&mut self) {
        let h = self.keyboard.layout().height_px().round().max(1.0) as u32;
        if h != self.h {
            self.h = h;
            self.layer.set_size(0, h);
            self.layer.set_exclusive_zone(h as i32);
            self.layer.commit();
            self.pool.resize((self.w * self.h * 4 * 2) as usize).ok();
            self.configured = false;
        }
    }

    fn draw(&mut self) {
        if !self.configured || !self.active || !self.user_visible {
            return;
        }
        let pressed = self
            .alt
            .as_ref()
            .map(|alt| alt.key)
            .or_else(|| self.pressed.first().copied());
        let alts = self.alt.as_ref().map(|alt| AltBar {
            items: &alt.items,
            selected: alt.selected,
        });
        if let Some(items) = self.sheet.clone() {
            let pixmap = self
                .renderer
                .render_sheet(self.keyboard.layout(), &self.theme, 1.0, "Clipboard", &items);
            self.commit_pixmap(pixmap);
            return;
        }
        let suggestions = if self.keyboard.is_panel() {
            self.keyboard.strip_text().into_iter().collect::<Vec<_>>()
        } else {
            self.keyboard.suggestions()
        };
        let pixmap = self.renderer.render_full(
            self.keyboard.layout(),
            &self.layer_name(),
            self.keyboard.shift(),
            &self.theme,
            1.0,
            pressed,
            alts,
            &suggestions,
        );
        self.commit_pixmap(pixmap);
    }

    fn commit_pixmap(&mut self, pixmap: osk_render::Pixmap) {
        let w = pixmap.width() as i32;
        let h = pixmap.height() as i32;
        if w != self.w as i32 || h != self.h as i32 {
            self.w = w as u32;
            self.h = h as u32;
            self.layer.set_size(0, self.h);
            self.layer.set_exclusive_zone(self.h as i32);
            self.layer.commit();
            self.pool.resize((self.w * self.h * 4 * 2) as usize).ok();
            return;
        }
        let stride = w * 4;
        let (buffer, canvas) = match self.pool.create_buffer(w, h, stride, wl_shm::Format::Argb8888)
        {
            Ok(pair) => pair,
            Err(err) => {
                eprintln!("rabbiz-osk: buffer error {err:?}");
                return;
            }
        };
        for (dst, src) in canvas.chunks_exact_mut(4).zip(pixmap.data().chunks_exact(4)) {
            dst[0] = src[2];
            dst[1] = src[1];
            dst[2] = src[0];
            dst[3] = src[3];
        }
        let surface = self.layer.wl_surface().clone();
        surface.damage_buffer(0, 0, w, h);
        if let Err(err) = buffer.attach_to(&surface) {
            eprintln!("rabbiz-osk: attach failed {err:?}");
            return;
        }
        surface.commit();
        self.held = Some(buffer);
    }

    fn show(&mut self) {
        if !self.visible {
            self.visible = true;
            // Keep the layer surface mapped: unmapping and re-mapping needs a fresh configure
            // and attaching a buffer first is a protocol error. Sliding it back in with the
            // margin is both simpler and flicker-free.
            self.layer.set_margin(0, 0, 0, 0);
            self.layer.set_exclusive_zone(self.h as i32);
            self.layer.commit();
        }
        self.sync_size();
        self.draw();
    }

    fn hide(&mut self) {
        if !self.visible {
            return;
        }
        self.visible = false;
        self.alt = None;
        self.pressed.clear();
        // Slide the surface below the screen edge and stop reserving space for it.
        self.layer.set_margin(0, 0, -(self.h as i32), 0);
        self.layer.set_exclusive_zone(0);
        self.layer.commit();
    }

    fn send_commands(&mut self, commands: Vec<Command>) {
        for command in commands {
            match command {
                Command::Commit(text) => {
                    if self.done_count > 0 {
                        self.im.commit_string(text);
                        self.im.commit(self.done_count);
                    }
                }
                Command::Backspace { bytes } => {
                    if self.done_count > 0 {
                        self.im.delete_surrounding_text(bytes as u32, 0);
                        self.im.commit(self.done_count);
                    }
                }
                Command::Enter => {
                    // A text commit with "\n" is ignored by single-line fields (Chrome's
                    // omnibox, search boxes). Send a real Return key press instead.
                    self.send_key(KEY_ENTER);
                }
                // Cursor moves and layout switches need no protocol request.
                Command::MoveCursor(_) | Command::Layer(_) | Command::Language(_) => {}
            }
        }
        let _ = self.conn.flush();
    }

    /// Toolbar buttons live in the strip, above the keys.
    fn on_toolbar_tap(&mut self, x: f32) {
        let mut hit = None;
        for index in 0..TOOLBAR_ICONS.len() {
            let rect = toolbar_button_rect(index, self.keyboard.layout(), 1.0);
            if x >= rect.x && x < rect.x + rect.w {
                hit = Some(index);
                break;
            }
        }
        match hit {
            Some(0) => {
                let items = self.clipboard.lock().map(|g| g.clone()).unwrap_or_default();
                self.sheet = Some(items);
                self.draw();
                return;
            }
            Some(1) => {
                eprintln!("rabbiz-osk: voice typing is not available on this device");
                return;
            }
            _ => {}
        }
        if self.keyboard.is_panel() {
            return;
        }
        // Suggestion slots: replace the partial word and continue typing.
        let picks = self.keyboard.suggestions();
        for (index, pick) in picks.iter().enumerate() {
            let rect = suggestion_rect(index, self.keyboard.layout(), 1.0);
            if x >= rect.x && x < rect.x + rect.w {
                let commands = self.keyboard.apply_suggestion(pick);
                self.send_commands(commands);
                self.draw();
                return;
            }
        }
    }

    /// Clipboard stub until K5 owns the history: paste the current selection via wl-paste.
    fn paste_clipboard(&mut self) {
        let output = std::process::Command::new("wl-paste").arg("--no-newline").output();
        match output {
            Ok(out) if out.status.success() => {
                let text = String::from_utf8_lossy(&out.stdout).to_string();
                if !text.is_empty() {
                    let commands = self.keyboard.insert_text(&text);
                    self.send_commands(commands);
                    self.draw();
                }
            }
            other => eprintln!("rabbiz-osk: clipboard unavailable ({other:?})"),
        }
    }

    /// Press and release one evdev keycode through the virtual keyboard.
    fn send_key(&mut self, keycode: u32) {
        let time = self.start.elapsed().as_millis() as u32;
        self.virtual_keyboard.key(time, keycode, 1); // wl_keyboard.key_state.pressed
        self.virtual_keyboard.key(time, keycode, 0); // wl_keyboard.key_state.released
        let _ = self.conn.flush();
    }

    fn apply_tap(&mut self, pos: KeyPos) {
        let commands = self.keyboard.press(pos, self.now_ms());
        self.send_commands(commands);
        self.sync_size();
        self.draw();
    }

    /// Index of the alternates slot under `x`, in the strip.
    fn alt_index_at(&self, x: f32) -> usize {
        let Some(alt) = &self.alt else { return 0 };
        let width = self.keyboard.layout().geometry.width_px;
        let margin = self.keyboard.layout().geometry.margin_px;
        let n = alt.items.len().max(1) as f32;
        let slot = (width - 2.0 * margin) / n;
        (((x - margin) / slot).floor() as i32).clamp(0, alt.items.len() as i32 - 1) as usize
    }

    fn commit_alternate(&mut self) {
        if let Some(alt) = self.alt.take() {
            if let Some(item) = alt.items.get(alt.selected).cloned() {
                let commands = self.keyboard.insert_text(&item);
                self.send_commands(commands);
            }
        }
        self.draw();
    }

    fn handle_gestures(&mut self, events: Vec<GestureEvent>) {
        for event in events {
            match event {
                GestureEvent::Press(pos) => {
                    self.pressed = vec![pos];
                    self.draw();
                }
                GestureEvent::Cancel(_) => {
                    // A long-press chooser stays open until the finger lifts.
                    if self.alt.is_none() {
                        self.pressed.clear();
                        self.draw();
                    }
                }
                GestureEvent::Tap(pos) => {
                    self.pressed.clear();
                    self.alt = None;
                    self.apply_tap(pos);
                }
                GestureEvent::LongPress(pos) => {
                    let items = self.keyboard.alternates(pos);
                    if items.is_empty() {
                        continue;
                    }
                    self.alt = Some(AltState { key: pos, items, selected: 0 });
                    self.draw();
                }
                GestureEvent::Repeat(pos) => {
                    // Stop at the start of the text: a held backspace must not spam the app
                    // with deletes it cannot apply (GTK asserts on negative positions).
                    let backspace =
                        matches!(self.keyboard.key(pos).map(|k| &k.action), Some(KeyAction::Backspace));
                    if backspace && !self.keyboard.text().is_empty() {
                        let commands = self.keyboard.backspace();
                        self.send_commands(commands);
                    }
                }
                GestureEvent::Swipe { key, dx, .. } => {
                    self.pressed.clear();
                    if self.keyboard.is_space(key) {
                        let step = self.keyboard.layout().geometry.width_px / 10.0;
                        let steps = (dx / step).round() as i32;
                        let commands = self.keyboard.space_swipe(steps);
                        self.send_commands(commands);
                    }
                    self.draw();
                }
            }
        }
    }
}

impl CompositorHandler for App {
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: i32) {}
    fn transform_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: wl_output::Transform) {}
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}
    fn surface_enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
    fn surface_leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
}

impl OutputHandler for App {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.out
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl LayerShellHandler for App {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        std::process::exit(0);
    }
    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _: u32,
    ) {
        if configure.new_size.0 > 0 {
            self.w = configure.new_size.0;
        }
        if configure.new_size.1 > 0 {
            self.h = configure.new_size.1;
        }
        self.configured = true;
        self.draw_pending = false;
        self.pool.resize((self.w * self.h * 4 * 2) as usize).ok();
        if self.visible {
            self.draw();
        }
    }
}

impl SeatHandler for App {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seats
    }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
    fn new_capability(&mut self, _: &Connection, qh: &QueueHandle<Self>, seat: wl_seat::WlSeat, cap: Capability) {
        if cap == Capability::Touch {
            if let Err(err) = self.seats.get_touch(qh, &seat) {
                eprintln!("rabbiz-osk: no touch: {err:?}");
            }
        }
    }
    fn remove_capability(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat, _: Capability) {}
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl TouchHandler for App {
    fn down(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_touch::WlTouch,
        _serial: u32,
        time: u32,
        _: wl_surface::WlSurface,
        id: i32,
        pos: (f64, f64),
    ) {
        self.touch_pos.insert(id, (pos.0 as f32, pos.1 as f32));
        if self.alt.is_some() {
            return;
        }
        if let Some(items) = self.sheet.clone() {
            for (index, item) in items.iter().enumerate() {
                let rect = sheet_row_rect(index, self.keyboard.layout(), 1.0);
                if pos.1 as f32 >= rect.y && pos.1 as f32 <= rect.y + rect.h {
                    let commands = self.keyboard.insert_text(item);
                    self.send_commands(commands);
                    self.sheet = None;
                    self.draw();
                    return;
                }
            }
            self.sheet = None;
            self.draw();
            return;
        }
        let strip_h = self.keyboard.layout().geometry.strip_height_px;
        if (pos.1 as f32) <= strip_h {
            self.on_toolbar_tap(pos.0 as f32);
            return;
        }
        let hit = self
            .keyboard
            .layout()
            .hit_test(&self.layer_name(), pos.0 as f32, pos.1 as f32);
        // Use our own monotonic clock: wl_touch timestamps come from the compositor and the
        // long-press/repeat tick in `GestureDetector` compares against `now_ms()`.
        let _ = time;
        let events = self.gesture.handle(
            TouchEvent::down(id as u64, pos.0 as f32, pos.1 as f32, self.now_ms()),
            hit,
        );
        self.handle_gestures(events);
    }

    fn motion(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_touch::WlTouch,
        _time: u32,
        id: i32,
        pos: (f64, f64),
    ) {
        self.touch_pos.insert(id, (pos.0 as f32, pos.1 as f32));
        if self.alt.is_some() {
            let index = self.alt_index_at(pos.0 as f32);
            if let Some(alt) = &mut self.alt {
                if alt.selected != index {
                    alt.selected = index;
                    self.draw();
                }
            }
            return;
        }
        let hit = self
            .keyboard
            .layout()
            .hit_test(&self.layer_name(), pos.0 as f32, pos.1 as f32);
        let events = self.gesture.handle(
            TouchEvent::mv(id as u64, pos.0 as f32, pos.1 as f32, self.now_ms()),
            hit,
        );
        self.handle_gestures(events);
    }

    fn up(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_touch::WlTouch,
        _serial: u32,
        time: u32,
        id: i32,
    ) {
        let pos = self.touch_pos.remove(&id).unwrap_or((0.0, 0.0));
        if self.alt.is_some() {
            self.commit_alternate();
            self.gesture.cancel_all(self.now_ms());
            return;
        }
        let hit = self
            .keyboard
            .layout()
            .hit_test(&self.layer_name(), pos.0, pos.1);
        let _ = time;
        let events = self
            .gesture
            .handle(TouchEvent::up(id as u64, pos.0, pos.1, self.now_ms()), hit);
        self.handle_gestures(events);
    }

    fn shape(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_touch::WlTouch, _: i32, _: f64, _: f64) {}
    fn orientation(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_touch::WlTouch, _: i32, _: f64) {}
    fn cancel(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_touch::WlTouch) {
        self.touch_pos.clear();
        if self.alt.is_some() {
            self.alt = None;
            self.draw();
            return;
        }
        let events = self.gesture.cancel_all(self.now_ms());
        self.handle_gestures(events);
    }
}

impl ShmHandler for App {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.reg
    }
    registry_handlers![OutputState, SeatState];
}

impl Dispatch<wl_seat::WlSeat, ()> for App {
    fn event(
        _: &mut App,
        _: &wl_seat::WlSeat,
        _: <wl_seat::WlSeat as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<App>,
    ) {
    }
}

impl Dispatch<ZwpVirtualKeyboardManagerV1, ()> for App {
    fn event(
        _: &mut App,
        _: &ZwpVirtualKeyboardManagerV1,
        _: <ZwpVirtualKeyboardManagerV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<App>,
    ) {
    }
}

impl Dispatch<ZwpVirtualKeyboardV1, ()> for App {
    fn event(
        _: &mut App,
        _: &ZwpVirtualKeyboardV1,
        _: <ZwpVirtualKeyboardV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<App>,
    ) {
    }
}

impl Dispatch<ZwpInputMethodManagerV2, ()> for App {
    fn event(
        _: &mut App,
        _: &ZwpInputMethodManagerV2,
        _: <ZwpInputMethodManagerV2 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<App>,
    ) {
    }
}

impl Dispatch<ZwpInputMethodV2, ()> for App {
    fn event(
        state: &mut App,
        _: &ZwpInputMethodV2,
        event: ImEvent,
        _: &(),
        _: &Connection,
        _: &QueueHandle<App>,
    ) {
        match event {
            ImEvent::Activate => {
                state.active = true;
                if state.user_visible {
                    state.show();
                }
            }
            ImEvent::Deactivate => {
                state.active = false;
                state.hide();
            }
            ImEvent::SurroundingText { text, cursor, anchor } => {
                let _ = (cursor, anchor);
                state.keyboard.set_surrounding_text(text);
            }
            ImEvent::ContentType { purpose, .. } => {
                // Gboard-style adaptation: email/url bottom rows, number pad for numeric fields.
                state.keyboard.set_purpose(InputPurpose::from_protocol(purpose.into()));
                state.sync_size();
                state.draw();
            }
            ImEvent::Done => state.done_count += 1,
            ImEvent::Unavailable => {
                eprintln!("rabbiz-osk: another input method owns the seat (squeekboard?)");
                std::process::exit(2);
            }
            _ => {}
        }
    }
}

delegate_compositor!(App);
delegate_output!(App);
delegate_shm!(App);
delegate_seat!(App);
delegate_touch!(App);
delegate_layer!(App);
delegate_registry!(App);

// ---- D-Bus compatibility ---------------------------------------------------

struct Osk0Service {
    visible: Arc<AtomicBool>,
}

#[zbus::interface(name = "sm.puri.OSK0")]
impl Osk0Service {
    fn set_visible(&self, visible: bool) {
        self.visible.store(visible, Ordering::SeqCst);
    }

    #[zbus(property)]
    fn visible(&self) -> bool {
        self.visible.load(Ordering::SeqCst)
    }
}

struct Osk1Service {
    reload_theme: Arc<AtomicBool>,
    next_language: Arc<AtomicBool>,
}

#[zbus::interface(name = "org.rabbiz.OSK1")]
impl Osk1Service {
    /// Re-read ~/.config/rabbiz-osk/theme.toml and repaint without restarting.
    fn reload_theme(&self) {
        self.reload_theme.store(true, Ordering::SeqCst);
    }

    /// Quick Settings "language" tile: cycle EN → TH without gsettings.
    fn next_language(&self) {
        self.next_language.store(true, Ordering::SeqCst);
    }
}

/// K5: watch the selection with `wl-paste --watch` (wl-clipboard uses data-control, so this
/// works without keyboard focus) and keep the last 20 text snippets in memory.
/// evdev key code for Enter (KEY_ENTER).
const KEY_ENTER: u32 = 28;

/// Minimal xkb keymap: the compositor needs one before it accepts key events.
const KEYMAP: &str = r#"xkb_keymap {
    xkb_keycodes  { include "evdev+aliases(qwerty)" };
    xkb_types     { include "complete" };
    xkb_compat    { include "complete" };
    xkb_symbols   { include "pc+us" };
};"#;

fn upload_keymap(keyboard: &ZwpVirtualKeyboardV1) {
    use std::io::Write;
    let Ok(mut file) = tempfile_memfd() else {
        eprintln!("rabbiz-osk: cannot create keymap fd");
        return;
    };
    if file.write_all(KEYMAP.as_bytes()).is_err() {
        eprintln!("rabbiz-osk: cannot write keymap");
        return;
    }
    let fd = std::os::fd::AsFd::as_fd(&file);
    keyboard.keymap(1 /* wl_keyboard.keymap_format.xkb_v1 */, fd, KEYMAP.len() as u32);
}

fn tempfile_memfd() -> std::io::Result<std::fs::File> {
    use std::os::fd::FromRawFd;
    let name = c"rabbiz-osk-keymap";
    let fd = unsafe { libc::memfd_create(name.as_ptr(), libc::MFD_CLOEXEC) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(unsafe { std::fs::File::from_raw_fd(fd) })
}

fn emoji_state_path() -> std::path::PathBuf {
    osk_core::predict::state_path().with_file_name("emoji-recent.txt")
}

fn spawn_clipboard_watch(items: Arc<Mutex<Vec<String>>>, changed: Arc<AtomicBool>) {
    std::thread::Builder::new()
        .name("osk-clipboard".into())
        .spawn(move || {
            let child = ProcessCommand::new("wl-paste")
                .args(["--watch", "--no-newline"])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn();
            let Ok(mut child) = child else {
                eprintln!("rabbiz-osk: wl-paste not available; clipboard history disabled");
                return;
            };
            let Some(stdout) = child.stdout.take() else { return };
            for line in BufReader::new(stdout).lines() {
                let Ok(text) = line else { break };
                if text.trim().is_empty() {
                    continue;
                }
                if let Ok(mut items) = items.lock() {
                    items.retain(|old| old != &text);
                    items.insert(0, text);
                    items.truncate(20);
                }
                changed.store(true, Ordering::SeqCst);
            }
        })
        .ok();
}

fn spawn_dbus(
    visible: Arc<AtomicBool>,
    reload_theme: Arc<AtomicBool>,
    next_language: Arc<AtomicBool>,
) {
    std::thread::Builder::new()
        .name("osk-dbus".into())
        .spawn(move || {
            let built = zbus::blocking::connection::Builder::session()
                .and_then(|b| b.name("sm.puri.OSK0"))
                .and_then(|b| {
                    b.serve_at(
                        "/sm/puri/OSK0",
                        Osk0Service { visible: visible.clone() },
                    )
                })
                .and_then(|b| {
                    b.serve_at(
                        "/org/rabbiz/OSK1",
                        Osk1Service {
                            reload_theme: reload_theme.clone(),
                            next_language: next_language.clone(),
                        },
                    )
                });
            match built.and_then(|b| b.build()) {
                Ok(_conn) => loop {
                    std::thread::park();
                },
                Err(err) => eprintln!("rabbiz-osk: D-Bus unavailable ({err}); compat methods disabled"),
            }
        })
        .ok();
}

fn load_theme(path: &std::path::Path) -> Option<Theme> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut dark = true;
    let mut accent: Option<(u8, u8, u8)> = None;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else { continue };
        let key = key.trim();
        let value = value.trim().trim_matches('"');
        match key {
            "mode" => dark = value != "light",
            "accent" => accent = parse_hex(value),
            _ => {}
        }
    }
    let mut theme = if dark { Theme::dark() } else { Theme::light() };
    if let Some(accent) = accent {
        theme.accent = accent;
    }
    Some(theme)
}

fn parse_hex(value: &str) -> Option<(u8, u8, u8)> {
    let hex = value.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some((r, g, b))
}

fn theme_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/gamer".into());
    std::path::Path::new(&home).join(".config/rabbiz-osk/theme.toml")
}

fn poll_fd(fd: std::os::fd::BorrowedFd<'_>, timeout_ms: i32) -> bool {
    let mut pollfd = libc::pollfd {
        fd: fd.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    unsafe { libc::poll(&mut pollfd, 1, timeout_ms) > 0 }
}

fn main() {
    let layouts = [
        ("en", include_str!("../../../data/layouts/en-us.toml")),
        ("th", include_str!("../../../data/layouts/th-kedmanee.toml")),
        ("emoji", include_str!("../../../data/layouts/emoji.toml")),
    ]
    .iter()
    .map(|(_, src)| Layout::parse(src).unwrap_or_else(|err| {
        eprintln!("rabbiz-osk: bad layout: {err}");
        std::process::exit(1);
    }))
    .collect::<Vec<_>>();

    let mut keyboard = match Keyboard::new(layouts) {
        Ok(keyboard) => keyboard,
        Err(err) => {
            eprintln!("rabbiz-osk: {err}");
            std::process::exit(1);
        }
    };
    // K6: predictions learn from the user's own typing; nothing is shipped but the seeds.
    keyboard.predictor = osk_core::predict::Predictor::load(&osk_core::predict::state_path());
    keyboard.load_recents(&emoji_state_path());

    let connection = Connection::connect_to_env().expect("rabbiz-osk needs a Wayland session");
    let (globals, mut queue) = registry_queue_init(&connection).expect("registry");
    let qh = queue.handle();
    let compositor = CompositorState::bind(&globals, &qh).expect("wl_compositor");
    let layer_shell = LayerShell::bind(&globals, &qh).expect("zwlr_layer_shell_v1");
    let shm = Shm::bind(&globals, &qh).expect("wl_shm");
    let im_manager = globals
        .bind::<ZwpInputMethodManagerV2, App, ()>(&qh, 1..=1, ())
        .expect("zwp_input_method_manager_v2 (labwc 0.20 advertises it)");
    // A seat proxy of our own: the input method is created per seat, and the surface only
    // needs text delivery, not keyboard focus.
    let seat = globals
        .bind::<wl_seat::WlSeat, App, ()>(&qh, 1..=7, ())
        .expect("wl_seat");

    let surface = compositor.create_surface(&qh);
    let layer = layer_shell.create_layer_surface(&qh, surface, Layer::Top, Some("rabbiz-osk"), None);
    layer.set_anchor(Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
    let h = keyboard.layout().height_px().round() as u32;
    layer.set_size(0, h);
    layer.set_exclusive_zone(h as i32);
    layer.set_keyboard_interactivity(KeyboardInteractivity::None);
    layer.commit();

    let visible = Arc::new(AtomicBool::new(true));
    let reload_theme = Arc::new(AtomicBool::new(false));
    let next_language = Arc::new(AtomicBool::new(false));
    spawn_dbus(visible.clone(), reload_theme.clone(), next_language.clone());

    let clipboard: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let clipboard_changed = Arc::new(AtomicBool::new(false));
    spawn_clipboard_watch(clipboard.clone(), clipboard_changed.clone());

    let pool = SlotPool::new(720 * h as usize * 4 * 2, &shm).expect("shm pool");
    let mut app = App {
        reg: RegistryState::new(&globals),
        seats: SeatState::new(&globals, &qh),
        out: OutputState::new(&globals, &qh),
        shm,
        pool,
        layer: layer.clone(),
        im: im_manager.get_input_method(&seat, &qh, ()),
        virtual_keyboard: {
            let manager = globals
                .bind::<ZwpVirtualKeyboardManagerV1, App, ()>(&qh, 1..=1, ())
                .expect("zwp_virtual_keyboard_manager_v1");
            let keyboard = manager.create_virtual_keyboard(&seat, &qh, ());
            upload_keymap(&keyboard);
            keyboard
        },
        done_count: 0,
        keyboard,
        gesture: GestureDetector::new(),
        renderer: Renderer::new(),
        theme: load_theme(&theme_path()).unwrap_or_else(Theme::dark),
        active: false,
        user_visible: true,
        draw_pending: false,
        configured: false,
        visible: false,
        w: 720,
        h,
        held: None,
        pressed: Vec::new(),
        alt: None,
        touch_pos: HashMap::new(),
        conn: connection.clone(),
        start: Instant::now(),
        predictor_saved: Instant::now(),
        clipboard: clipboard.clone(),
        clipboard_changed: clipboard_changed.clone(),
        sheet: None,
    };
    eprintln!(
        "rabbiz-osk: up, layout {} ({}x{}), waiting for text-input activation",
        app.keyboard.layout().id,
        app.w,
        app.h
    );

    loop {
        if let Err(err) = queue.dispatch_pending(&mut app) {
            eprintln!("rabbiz-osk: dispatch: {err}");
            break;
        }
        let _ = app.conn.flush();

        // D-Bus compat + theme reload are polled (cheap: one atomic load per turn).
        if app.clipboard_changed.swap(false, Ordering::SeqCst) && app.sheet.is_some() {
            app.sheet = Some(app.clipboard.lock().map(|g| g.clone()).unwrap_or_default());
            app.draw();
        }
        let want = visible.load(Ordering::SeqCst);
        if want != app.user_visible {
            app.user_visible = want;
            if app.active {
                if want {
                    app.show();
                } else {
                    app.hide();
                }
            }
        }
        if app.keyboard.predictor.is_dirty()
            && app.predictor_saved.elapsed().as_secs() >= 2
        {
            let path = osk_core::predict::state_path();
            if let Err(err) = app.keyboard.predictor.save(&path) {
                eprintln!("rabbiz-osk: predictor save failed: {err}");
            }
            app.keyboard.predictor.mark_clean();
            app.predictor_saved = Instant::now();
        }
        if app.keyboard.recents_dirty() {
            if let Err(err) = app.keyboard.save_recents(&emoji_state_path()) {
                eprintln!("rabbiz-osk: emoji recents save failed: {err}");
            }
            app.keyboard.mark_recents_clean();
        }
        if next_language.swap(false, Ordering::SeqCst) {
            let commands = app.keyboard.cycle_language();
            app.send_commands(commands);
            app.sync_size();
            app.draw();
        }
        if reload_theme.swap(false, Ordering::SeqCst) {
            if let Some(theme) = load_theme(&theme_path()) {
                app.theme = theme;
            }
            app.draw();
        }

        let touching = app.gesture.active_touches() > 0;
        if let Some(guard) = queue.prepare_read() {
            let ready = poll_fd(guard.connection_fd(), if touching { 16 } else { 100 });
            if ready {
                let _ = guard.read();
            }
        }
        if touching {
            let events = app.gesture.tick(app.now_ms());
            app.handle_gestures(events);
        }
    }
}
