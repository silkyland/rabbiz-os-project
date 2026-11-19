//! libinput event → seat dispatch (M1-T1/M1-T8) + shell gestures (M3-T6).
//!
//! Every touch event is also fed to the shell's `GestureTracker`: contacts
//! starting in a system zone (nav bar, top edge, side edges) are claimed by
//! the shell and never reach the client, so a home swipe doesn't scroll the
//! app underneath. Content-zone touches reach the client as usual; the
//! tracker sees them too so taps still resolve when the shell is in the
//! foreground.
//!
//! Touch carries libinput timestamps through to the seat so gesture code
//! can compute release velocity; the power key spawns `a21s-power-key
//! toggle` detached on release.

use smithay::reexports::wayland_server::{protocol::wl_surface::WlSurface, Resource};
use std::process::Command;
use std::time::Instant;

use de_ui::gestures::{Gesture, GestureEvent, Zone};

use smithay::backend::input::{
    AbsolutePositionEvent, Axis, AxisSource, Device as _, DeviceCapability, Event, InputBackend,
    InputEvent, KeyState, KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent,
    PointerMotionEvent, TouchEvent as _,
};
use smithay::desktop::WindowSurfaceType;
use smithay::input::keyboard::{FilterResult, Keysym, ModifiersState};
use smithay::input::pointer::{AxisFrame, ButtonEvent, MotionEvent, RelativeMotionEvent};
use smithay::input::touch::{DownEvent, MotionEvent as TouchMotion, UpEvent};
use smithay::utils::{Logical, Point, Size, SERIAL_COUNTER as SCOUNTER};

use crate::state::De;

pub(crate) struct SurfaceHit {
    pub surface: WlSurface,
    pub location: Point<f64, Logical>,
    pub keyboard_target: Option<WlSurface>,
    pub above_windows: bool,
}

impl SurfaceHit {
    fn into_focus(self) -> (WlSurface, Point<f64, Logical>) {
        (self.surface, self.location)
    }
}

impl De {
    /// Dispatch one libinput event into the seat.
    pub fn process_input_event<B: InputBackend>(&mut self, event: InputEvent<B>) {
        // M1 bring-up aid: DE_INPUT_DEBUG=1 logs every backend event.
        if std::env::var_os("DE_INPUT_DEBUG").is_some() {
            eprintln!("input: {}", variant_name::<B>(&event));
        }
        // Any hardware input is activity — undims and restarts the clock.
        self.power_note_activity();
        match event {
            InputEvent::DeviceAdded { device } => self.input_device::<B>(&device, true),
            InputEvent::DeviceRemoved { device } => self.input_device::<B>(&device, false),
            InputEvent::Keyboard { event } => self.on_keyboard_key::<B>(&event),
            InputEvent::TouchDown { event } => self.on_touch_down::<B>(&event),
            InputEvent::TouchMotion { event } => self.on_touch_motion::<B>(&event),
            InputEvent::TouchUp { event } => self.on_touch_up::<B>(&event),
            InputEvent::TouchCancel { event } => {
                if self.sleeping() {
                    self.sleep_touch_cancel();
                    self.touch_claimed = true;
                    return;
                }
                self.power_menu_release();
                self.feed_gesture(
                    i32::from(event.slot()),
                    de_api::input::TouchPhase::Cancel,
                    self.last_touch_pos,
                    event.time_msec(),
                );
                if !std::mem::take(&mut self.touch_claimed) {
                    self.touch.clone().cancel(self);
                }
            }
            InputEvent::PointerMotion { event } => {
                let pos = self.pointer.current_location() + event.delta();
                self.pointer_motion(event.time_msec(), pos, event.delta());
            }
            InputEvent::PointerMotionAbsolute { event } => {
                let pos = event.position_transformed(Size::from((
                    self.output.width as i32,
                    self.output.height as i32,
                )));
                let delta = pos - self.pointer.current_location();
                self.pointer_motion(event.time_msec(), pos, delta);
            }
            InputEvent::PointerButton { event } => {
                self.pointer_button(event.time_msec(), event.button_code(), event.state())
            }
            InputEvent::PointerAxis { event } => {
                let mut frame = AxisFrame::new(event.time_msec()).source(event.source());
                let v = event.amount(Axis::Vertical).unwrap_or(0.0);
                let h = event.amount(Axis::Horizontal).unwrap_or(0.0);
                frame = frame
                    .relative_direction(Axis::Vertical, event.relative_direction(Axis::Vertical))
                    .relative_direction(
                        Axis::Horizontal,
                        event.relative_direction(Axis::Horizontal),
                    );
                if event.source() == AxisSource::Finger {
                    frame = frame.value(Axis::Vertical, v).value(Axis::Horizontal, h);
                } else {
                    frame = frame
                        .v120(
                            Axis::Vertical,
                            event.amount_v120(Axis::Vertical).unwrap_or(0.0) as i32,
                        )
                        .v120(
                            Axis::Horizontal,
                            event.amount_v120(Axis::Horizontal).unwrap_or(0.0) as i32,
                        );
                }
                if matches!(event.source(), AxisSource::Finger | AxisSource::Continuous)
                    && v == 0.0
                    && h == 0.0
                {
                    frame = frame.stop(Axis::Vertical).stop(Axis::Horizontal);
                }
                self.pointer_axis_frame(frame);
            }
            _ => {}
        }
    }

    /// One pointer button event — shared by libinput and the virtual
    /// pointer protocol (a21s-wayvnc / Rabbiz Mirror).
    ///
    /// Click-to-focus: a press that lands on a client surface moves
    /// keyboard/pointer focus to it (the touch model has no equivalent —
    /// touch stays "last touched surface" via the down handler). A press on
    /// a shell-owned surface is consumed by the shell: a nearby release of
    /// the same button synthesises the tap the touch path would have made,
    /// and no button event leaks to a client underneath.
    pub(crate) fn pointer_button(
        &mut self,
        time: u32,
        button: u32,
        state: smithay::backend::input::ButtonState,
    ) {
        self.pointer_active = true;
        // A click between Super-down and Super-up means Super is a real
        // modifier, not the launcher key.
        self.launcher_pending = false;
        let pressed = state == smithay::backend::input::ButtonState::Pressed;
        if self.menu_pointer_button(button, pressed, self.pointer.current_location()) {
            return;
        }
        if self.sleeping() {
            // The lock/power screens are touch-only; swallow pointer input
            // so nothing reaches a client under the lock.
            return;
        }
        let pos = self.pointer.current_location();
        let serial = SCOUNTER.next_serial();
        let ptr = self.pointer.clone();
        if pressed {
            if self.pointer_shell_owned(pos) {
                self.pointer_press = Some((pos, button));
                return;
            }
            if let Some(target) = self.surface_under(pos).and_then(|hit| hit.keyboard_target) {
                self.focus_surface(Some(target), serial);
            }
        } else if let Some((down_pos, down_button)) = self.pointer_press.take() {
            let moved = ((pos.x - down_pos.x).powi(2) + (pos.y - down_pos.y).powi(2)).sqrt();
            if down_button == button && moved <= 12.0 {
                self.pointer_tap(down_pos, time);
            }
            return;
        }
        ptr.button(
            self,
            &ButtonEvent {
                button,
                state,
                serial,
                time,
            },
        );
    }

    /// One axis frame — shared by libinput and the virtual pointer protocol.
    pub(crate) fn pointer_axis_frame(&mut self, frame: AxisFrame) {
        self.pointer_active = true;
        let pos = self.pointer.current_location();
        if self.shell_axis_scroll(&frame, pos) {
            // The shell consumed the scroll (drawer/recents/home pager or a
            // consume-only surface) — the client must not see it too.
            return;
        }
        let ptr = self.pointer.clone();
        ptr.axis(self, frame);
    }

    /// Track hot-plugged input devices and maintain `hw_keyboard` /
    /// `hw_pointer` (OSK suppression, cursor policy). libinput reports the
    /// platform button devices (power/volume keys) with keyboard
    /// capability too, so "hardware" means a real bus identity: a non-zero
    /// USB VID:PID, or a `uhid` path (Bluetooth HID — the kernel has no
    /// native BT HID, everything arrives via uhid). Virtual/test devices
    /// (`/devices/virtual/` without uhid, e.g. the a21s touch injector or a
    /// wayvnc uinput node) never count — same rule as
    /// `de_system::input_devices`, which udev classifies the same way.
    fn input_device<B: InputBackend>(&mut self, device: &B::Device, added: bool) {
        let real_bus = device
            .usb_id()
            .is_some_and(|(vid, pid)| vid != 0 && pid != 0)
            || device
                .syspath()
                .is_some_and(|p| p.to_string_lossy().contains("/uhid/"));
        if !real_bus {
            return;
        }
        let id = device.id();
        if device.has_capability(DeviceCapability::Keyboard) {
            if added {
                self.kbd_devices.insert(id.clone());
            } else {
                self.kbd_devices.remove(&id);
            }
        }
        if device.has_capability(DeviceCapability::Pointer) {
            if added {
                self.ptr_devices.insert(id.clone());
            } else {
                self.ptr_devices.remove(&id);
            }
        }
        let was = (self.hw_keyboard, self.hw_pointer);
        self.hw_keyboard = !self.kbd_devices.is_empty();
        self.hw_pointer = !self.ptr_devices.is_empty();
        // A keyboard appearing mid-session re-spawns the OSK with
        // RABBIZ_HW_KEYBOARD=1 so it can hide itself; one leaving brings
        // the normal spawn back on the next check_osk tick.
        if self.hw_keyboard != was.0 {
            self.osk = None;
        }
        if std::env::var_os("DE_INPUT_DEBUG").is_some() {
            eprintln!(
                "input: device {} '{}' kbd={} ptr={} (hw_kbd={} hw_ptr={})",
                if added { "added" } else { "removed" },
                device.name(),
                device.has_capability(DeviceCapability::Keyboard),
                device.has_capability(DeviceCapability::Pointer),
                self.hw_keyboard,
                self.hw_pointer,
            );
        }
    }

    /// Is the point owned by the shell rather than a Wayland client?
    ///
    /// Mirrors `claim_touch`: while the shell is foreground every point is
    /// shell-owned except interactive layer surfaces (the OSK); while an
    /// app is foreground only the system zones (nav bar, top edge, side
    /// edges) are shell-owned — a click in the content area is the app's.
    fn pointer_shell_owned(&self, pos: Point<f64, Logical>) -> bool {
        if self.panels.shown() || self.locked() || self.sleeping() || self.menu_is_open() {
            return true;
        }
        match self.surface_under(pos) {
            // Interactive layer (OSK) sits above everything, shell included.
            Some(hit) if hit.above_windows => false,
            Some(_) => {
                // A window surface: in an app only system zones belong to
                // the shell; on shell screens surface_under already
                // returned None except for layers.
                self.gestures.zone(api_point(pos)) != Zone::Content
            }
            None => true,
        }
    }

    /// Synthesize the tap a touch would have produced (`Gesture::Tap` in
    /// `handle_gesture`): panels and the lock screen first, then the
    /// shell-screen dispatcher.
    fn pointer_tap(&mut self, pos: Point<f64, Logical>, _time: u32) {
        use crate::state::Foreground;
        let api = api_point(pos);
        if self.panels.shown() {
            self.panel_tap(api);
            return;
        }
        if self.locked() {
            if self.power.menu_over_lock {
                self.power_menu_tap(api, true);
            } else {
                self.lock_tap(api);
            }
            return;
        }
        if self.menu_is_open() || self.foreground != Foreground::Shell {
            return;
        }
        let size = de_api::Size::new(self.output.width as i32, self.output.height as i32);
        if self.shot_fx.tap(api, size, std::time::Instant::now()) {
            self.needs_present = true;
        } else if !self.release_home_drag(false) {
            self.on_shell_tap(api);
        }
    }

    /// Route a scroll frame to the shell when the pointer is over a
    /// shell-owned surface. Returns `true` when consumed.
    ///
    /// Mapping (the pointer twin of the touch drags in `handle_gesture`):
    /// - Drawer: vertical scroll moves `drawer_scroll` (same clamp the
    ///   renderer applies — stored unclamped like the touch path).
    /// - Recents: horizontal (or vertical) scroll pages the card pager.
    /// - Home: horizontal scroll pages the icon grid; vertical scroll is
    ///   consumed (no vertical list on Home).
    /// - Panels/lock/search/etc.: consumed so nothing leaks to a client
    ///   underneath.
    fn shell_axis_scroll(&mut self, frame: &AxisFrame, pos: Point<f64, Logical>) -> bool {
        if !self.pointer_shell_owned(pos) {
            return false;
        }
        let (dx, dy) = wheel_delta(frame);
        if dx == 0.0 && dy == 0.0 {
            // axis-stop / zero frames still count as "consumed" on shell
            // surfaces so clients never see phantom scroll.
            return true;
        }
        use crate::state::Foreground;
        use de_ui::nav::Screen;
        match self.nav.current {
            Screen::Drawer if self.foreground == Foreground::Shell => {
                let step = if dy.abs() >= dx.abs() { dy } else { dx };
                let next = (self.navigation.drawer_scroll + step.round() as i32).max(0);
                if next != self.navigation.drawer_scroll {
                    self.navigation.drawer_scroll = next;
                    self.shell_dirty = true;
                }
                self.needs_present = true;
            }
            Screen::Recents if self.foreground == Foreground::Shell => {
                self.recents_wheel(dx + dy);
            }
            Screen::Home if self.foreground == Foreground::Shell => {
                if dx.abs() >= dy.abs() {
                    self.home_wheel(dx);
                }
                // Vertical wheel on Home: nothing scrolls — consumed.
            }
            _ => {}
        }
        self.needs_present = true;
        true
    }

    /// One wheel notch over Recents pages the card carousel (drag + release
    /// through the same `Pager` the touch path uses, so the settle spring
    /// animates exactly like a flick).
    fn recents_wheel(&mut self, delta: f32) {
        if delta == 0.0 || self.morph.is_some() {
            return;
        }
        let size = de_api::Size::new(self.output.width as i32, self.output.height as i32);
        let stride = de_ui::recents::card_stride(size) as f32;
        if stride <= 0.0 {
            return;
        }
        // One notch = one card: drag a full stride opposite the scroll
        // direction, then release with a matching velocity.
        let dir = if delta > 0.0 { -1.0 } else { 1.0 };
        self.navigation.recents_pager.begin();
        self.navigation.recents_pager.drag(dir * stride, stride);
        self.navigation.recents_pager.release(dir * 900.0, stride);
        self.navigation.recents_last_tick = Some(Instant::now());
        self.navigation.recents_axis = None;
        self.shell_dirty = true;
        self.needs_present = true;
    }

    /// Pointer motion + hit-test — shared by libinput and the virtual
    /// pointer protocol.
    pub(crate) fn pointer_motion(
        &mut self,
        time: u32,
        pos: Point<f64, Logical>,
        delta: Point<f64, Logical>,
    ) {
        self.pointer_active = true;
        // The cursor sprite is a composited element: motion must schedule
        // a frame or it would only move on unrelated damage.
        self.needs_present = true;
        self.menu_pointer_moved(pos);
        let serial = SCOUNTER.next_serial();
        let under = self.surface_under(pos).map(SurfaceHit::into_focus);
        let ptr = self.pointer.clone();
        ptr.motion(
            self,
            under.clone(),
            &MotionEvent {
                location: pos,
                serial,
                time,
            },
        );
        ptr.relative_motion(
            self,
            under,
            &RelativeMotionEvent {
                delta,
                delta_unaccel: delta,
                utime: u64::from(time) * 1000,
            },
        );
    }

    fn on_keyboard_key<B: InputBackend>(&mut self, evt: &B::KeyboardKeyEvent) {
        let serial = SCOUNTER.next_serial();
        let keycode = evt.key_code();
        let state = evt.state();
        let time = evt.time_msec();
        if state == KeyState::Pressed && self.menu_key(keycode.raw()) {
            return;
        }
        let Some(keyboard) = self.seat.get_keyboard() else {
            return;
        };
        keyboard.input(
            self,
            keycode,
            state,
            serial,
            time,
            |de, mods: &ModifiersState, keysym| {
                let raw = keysym.raw_syms().first().copied();
                // Power button: act on release only, detached so the 300 ms
                // combo guard in a21s-power-key never stalls the loop. The
                // release also wakes a sleeping panel via `toggle`.
                if raw == Some(Keysym::XF86_PowerOff) && state == KeyState::Released {
                    let _ = Command::new("setsid")
                        .args(["a21s-power-key", "toggle"])
                        .spawn();
                    return FilterResult::Intercept(());
                }
                if de.sleeping() {
                    // Any other key wakes the panel; swallow it so the
                    // unlock press doesn't type into an app.
                    if state == KeyState::Pressed {
                        de.power_wake();
                    }
                    return FilterResult::Intercept(());
                }
                if de.locked() {
                    return FilterResult::Intercept(());
                }
                // Shell keybindings (shortcuts.rs): chords and the bare
                // Super launcher toggle. Runs before the Search/Drawer
                // typing block so bindings work on every screen; consumed
                // events never reach a focused client.
                if let Some(sym) = raw {
                    if de.keyboard_shortcut(mods, sym, state, u64::from(time)) {
                        return FilterResult::Intercept(());
                    }
                }
                if de.foreground == crate::state::Foreground::Shell
                    && matches!(
                        de.nav.current,
                        de_ui::nav::Screen::Search | de_ui::nav::Screen::Drawer
                    )
                {
                    if state == KeyState::Pressed {
                        match raw {
                            Some(Keysym::BackSpace) => {
                                de.navigation.search_query.pop();
                                de.navigation.drawer_scroll = 0;
                                de.shell_dirty = true;
                                de.needs_present = true;
                            }
                            Some(Keysym::Escape) => {
                                de.navigation.search_query.clear();
                                de.show_shell(de_ui::nav::Screen::Home);
                            }
                            // Enter opens the top search hit — the
                            // keyboard twin of tapping the first row (and
                            // the same path the shell IM's Enter takes).
                            Some(Keysym::Return) | Some(Keysym::KP_Enter)
                                if de.nav.current == de_ui::nav::Screen::Search =>
                            {
                                let rows = de.search_rows();
                                if let Some(first) = rows.first() {
                                    let id = first.id.clone();
                                    let size = de_api::Size::new(
                                        de.output.width as i32,
                                        de.output.height as i32,
                                    );
                                    let rect = de_ui::search::icon_rect(size, &rows, 0);
                                    de.try_launch(&id, rect);
                                }
                            }
                            _ => {
                                if let Some(ch) = keysym.modified_sym().key_char() {
                                    if !ch.is_control() {
                                        de.navigation.search_query.push(ch);
                                        de.navigation.drawer_scroll = 0;
                                        de.shell_dirty = true;
                                        de.needs_present = true;
                                    }
                                }
                            }
                        }
                    }
                    return FilterResult::Intercept(());
                }
                FilterResult::Forward
            },
        );
    }

    fn on_touch_down<B: InputBackend>(&mut self, evt: &B::TouchDownEvent) {
        let pos = evt.position_transformed(Size::from((
            self.output.width as i32,
            self.output.height as i32,
        )));
        self.last_touch_pos = pos;
        // A touch between Super-down and Super-up means Super is a real
        // modifier, not the launcher key.
        self.launcher_pending = false;
        // Sleeping screen: the contact may be a hold-to-peek or the first
        // half of a wake double-tap — never delivered to clients.
        if self.sleeping() {
            self.sleep_touch_down();
            self.touch_claimed = true;
            return;
        }
        // Shell gestures first: a contact starting in a system zone — or any
        // contact while a shell screen is foreground — is claimed by the
        // shell and never reaches the client.
        let hit = self.surface_under(pos);
        // An open context menu is dismissed by a touch outside it, and that touch is consumed.
        if self.dismiss_popup_for_touch(hit.as_ref()) || self.menu_touch_down(pos) {
            self.touch_claimed = true;
            return;
        }
        let zone = self.gestures.zone(api_point(pos));
        // Top layers (including the OSK) own their interactive region.
        // Retain the nav strip as a way to leave the foreground client.
        let layer_contact = zone != Zone::NavBar
            && !self.panel_wants_touch()
            && hit.as_ref().is_some_and(|h| h.above_windows);
        if !layer_contact {
            self.feed_gesture(
                i32::from(evt.slot()),
                de_api::input::TouchPhase::Down,
                pos,
                evt.time_msec(),
            );
        }
        // Power-screen contact down: arm a destructive row (800 ms hold).
        if !self.locked()
            && self.foreground == crate::state::Foreground::Shell
            && self.nav.current == de_ui::nav::Screen::Power
        {
            self.power_menu_press(api_point(pos));
        }
        self.touch_claimed = claim_touch(
            zone,
            self.foreground == crate::state::Foreground::Shell,
            layer_contact,
        ) || self.panel_wants_touch()
            || self.locked();
        if self.touch_claimed {
            return;
        }
        let serial = SCOUNTER.next_serial();
        if let Some(target) = hit.as_ref().and_then(|hit| hit.keyboard_target.clone()) {
            self.focus_surface(Some(target), serial);
        }
        let under = hit.map(SurfaceHit::into_focus);
        if std::env::var_os("DE_INPUT_DEBUG").is_some() {
            let info = under.as_ref().map(|(s, l)| {
                let alive = self
                    .display_handle
                    .get_client(s.id())
                    .map_or("DEAD", |_| "alive");
                format!("{:?}@{l:?} client={alive}", s.id())
            });
            eprintln!(
                "touch_down pos={pos:?} under={info:?} windows={} kbd={}",
                self.space.elements().count(),
                self.seat.get_keyboard().is_some()
            );
        }
        let touch = self.touch.clone();
        touch.down(
            self,
            under,
            &DownEvent {
                slot: evt.slot(),
                location: pos,
                serial,
                time: evt.time_msec(),
            },
        );
    }

    fn on_touch_motion<B: InputBackend>(&mut self, evt: &B::TouchMotionEvent) {
        let pos = evt.position_transformed(Size::from((
            self.output.width as i32,
            self.output.height as i32,
        )));
        self.last_touch_pos = pos;
        if self.sleeping() {
            return;
        }
        self.feed_gesture(
            i32::from(evt.slot()),
            de_api::input::TouchPhase::Motion,
            pos,
            evt.time_msec(),
        );
        if self.touch_claimed {
            return;
        }
        let under = self.surface_under(pos).map(SurfaceHit::into_focus);
        let touch = self.touch.clone();
        touch.motion(
            self,
            under,
            &TouchMotion {
                slot: evt.slot(),
                location: pos,
                time: evt.time_msec(),
            },
        );
    }

    fn on_touch_up<B: InputBackend>(&mut self, evt: &B::TouchUpEvent) {
        let pos = self.last_touch_pos;
        if self.sleeping() {
            self.sleep_touch_up();
            self.touch_claimed = true;
            self.needs_present = true;
            return;
        }
        self.power_menu_release();
        self.feed_gesture(
            i32::from(evt.slot()),
            de_api::input::TouchPhase::Up,
            pos,
            evt.time_msec(),
        );
        if std::mem::take(&mut self.touch_claimed) {
            self.needs_present = true;
            return;
        }
        let serial = SCOUNTER.next_serial();
        let touch = self.touch.clone();
        touch.up(
            self,
            &UpEvent {
                slot: evt.slot(),
                serial,
                time: evt.time_msec(),
            },
        );
    }

    /// Feed the gesture tracker and act on what it emits.
    fn feed_gesture(
        &mut self,
        slot: i32,
        phase: de_api::input::TouchPhase,
        pos: Point<f64, Logical>,
        time_ms: u32,
    ) {
        let ev = de_api::input::TouchEvent {
            slot,
            phase,
            pos: api_point(pos),
            time_ms: u64::from(time_ms),
        };
        if let Some(g) = self.gestures.feed(&ev) {
            self.on_gesture_event(g, ev.time_ms);
        }
    }

    pub(crate) fn tick_gestures(&mut self) {
        let now_ms = std::time::Duration::from(self.clock.now()).as_millis() as u32;
        if let Some(event) = self.gestures.tick(u64::from(now_ms)) {
            self.on_gesture_event(event, u64::from(now_ms));
        }
    }

    fn on_gesture_event(&mut self, ev: GestureEvent, time_ms: u64) {
        use crate::navigation::RecentsAxis;
        use crate::state::Foreground;
        use de_ui::nav::Screen;
        if !matches!(
            ev,
            GestureEvent::Progress { .. } | GestureEvent::HoldDrag { .. }
        ) && std::env::var_os("DE_GESTURE_DEBUG").is_some()
        {
            eprintln!("gesture: {ev:?}");
        }
        // The lock overlay owns every gesture while it covers the screen
        // (until an unlock has been committed).
        if self.lock.shown() && !self.lock.is_unlocking() {
            self.lock_gesture(ev);
            return;
        }
        let g = match ev {
            GestureEvent::Progress {
                zone: Zone::NavBar,
                dx,
                dy,
            } => {
                // Horizontal scrub along the bottom bar is QuickSwitch; don't
                // hijack it as a vertical home-drag.
                if dx.abs() > dy.abs() * 1.5 && dy > -24.0 {
                    return;
                }
                self.follow_home_drag(dx, dy, time_ms);
                return;
            }
            GestureEvent::Progress { zone, dx, dy } if zone != Zone::NavBar => {
                if matches!(zone, Zone::EdgeLeft | Zone::EdgeRight) {
                    self.back_progress(zone, dx, dy);
                    if self.navigation.back.is_some() {
                        return;
                    }
                }
                if self.panel_progress(zone == Zone::TopEdge, dx, dy) {
                    return;
                }
                if zone == Zone::Content {
                    if self.foreground == Foreground::Shell
                        && self.nav.current == Screen::Drawer
                        && !self.navigation.drawer_closing
                    {
                        if self.navigation.drawer_dragging
                            && self.navigation.drawer_pull_y == 0.0
                            && dy < 0.0
                        {
                            self.drag_drawer(dy);
                        } else if self.navigation.drawer_pull_y > 0.0
                            || (self.navigation.drawer_drag_origin == 0 && dy > 0.0)
                        {
                            let pull = dy.max(0.0);
                            self.navigation.drawer_dragging = true;
                            self.navigation.drawer_pull_y = pull;
                            self.navigation.drawer_slide.value = pull;
                            self.navigation.drawer_slide.target = pull;
                            self.needs_present = true;
                        } else {
                            self.navigation.drawer_dragging = false;
                            self.navigation.drawer_pull_y = 0.0;
                            let next =
                                (self.navigation.drawer_drag_origin - dy.round() as i32).max(0);
                            if next != self.navigation.drawer_scroll {
                                self.navigation.drawer_scroll = next;
                                self.shell_dirty = true;
                                self.needs_present = true;
                            }
                        }
                    } else if self.foreground == Foreground::Shell
                        && self.nav.current == Screen::Recents
                    {
                        let axis = *self.navigation.recents_axis.get_or_insert_with(|| {
                            if dx.abs() > dy.abs() {
                                RecentsAxis::Horizontal
                            } else {
                                RecentsAxis::Vertical
                            }
                        });
                        if axis == RecentsAxis::Vertical && self.morph.is_none() {
                            self.card_dismiss_drag(dy);
                        } else if axis == RecentsAxis::Horizontal {
                            let size = de_api::Size::new(
                                self.output.width as i32,
                                self.output.height as i32,
                            );
                            self.navigation
                                .recents_pager
                                .drag(dx, de_ui::recents::card_stride(size) as f32);
                            self.shell_dirty = true;
                            self.needs_present = true;
                        }
                    } else if self.foreground == Foreground::Shell
                        && self.nav.current == Screen::Home
                        && !self.home_ui.paging()
                        && !self.home_ui.dragging_icon()
                        && dy < -16.0
                        && -dy > dx.abs() * 1.25
                    {
                        self.home_drag_cancelled();
                        self.begin_drawer_drag();
                        self.drag_drawer(dy);
                        return;
                    } else if self.foreground == Foreground::Shell
                        && self.nav.current == Screen::Home
                        && !self.home_ui.paging()
                        && dy > 24.0
                        && dy > dx.abs() * 1.4
                        && self.panel_progress(true, dx, dy)
                    {
                        return;
                    } else {
                        self.home_drag_progress(dx, dy);
                    }
                }
                return;
            }
            GestureEvent::Started { pos, zone } => {
                self.clear_back_feedback();
                self.navigation.zone = zone;
                self.navigation.start = pos;
                self.navigation.drawer_drag_origin = self.navigation.drawer_scroll;
                if self.foreground == Foreground::Shell
                    && self.nav.current == Screen::Recents
                    && zone == Zone::Content
                    && self.morph.is_none()
                {
                    self.navigation.recents_axis = None;
                    self.navigation.recents_pager.begin();
                }
                self.panel_touch_started(pos);
                self.home_touch_started();
                return;
            }
            GestureEvent::NavPaused => {
                crate::haptics::pulse(crate::haptics::HapticKind::RecentsHold);
                self.nav_preview(true);
                return;
            }
            GestureEvent::NavResumed => {
                self.nav_preview(false);
                return;
            }
            GestureEvent::NavReleased { dy, vy, paused } => {
                use de_ui::gestures::{decide_navigation, NavDestination};
                match decide_navigation(dy, vy, self.output.height as f32, paused) {
                    NavDestination::Home if self.panels.shown() => self.panel_close(),
                    NavDestination::Home => {
                        crate::haptics::pulse(crate::haptics::HapticKind::NavCommit);
                        if self.foreground == Foreground::Shell && self.nav.current == Screen::Home
                        {
                            self.open_drawer_animated(None, vy);
                        } else if self.foreground == Foreground::App {
                            self.minimize_foreground();
                        } else {
                            self.show_shell(Screen::Home);
                        }
                    }
                    NavDestination::Recents => {
                        self.show_shell(Screen::Recents);
                    }
                    NavDestination::Cancel => {
                        self.release_home_drag(false);
                    }
                }
                return;
            }
            GestureEvent::Released { dx, dy, vx, vy } => {
                if self.panels.pull.dragging() {
                    self.panel_released(vy);
                } else if self.foreground == Foreground::Shell
                    && self.nav.current == Screen::Recents
                {
                    let axis = self.navigation.recents_axis;
                    match axis {
                        // Swipe up on a card closes that app (Android).
                        Some(RecentsAxis::Vertical) if self.card_dismiss_release(dy, vy) => {
                            self.release_recents_pager(0.0);
                        }
                        // Swipe up on the space around the cards: Home.
                        Some(RecentsAxis::Vertical) if -dy > self.output.height as f32 * 0.09 => {
                            self.release_recents_pager(0.0);
                            self.show_shell(Screen::Home);
                        }
                        Some(RecentsAxis::Horizontal) => self.release_recents_pager(vx),
                        _ => self.release_recents_pager(0.0),
                    }
                } else if self.foreground == Foreground::Shell
                    && self.nav.current == Screen::Drawer
                    && self.navigation.drawer_dragging
                    && self.navigation.drawer_pull_y == 0.0
                {
                    self.release_drawer_drag(vy);
                } else if self.foreground == Foreground::Shell
                    && self.nav.current == Screen::Drawer
                    && self.navigation.drawer_pull_y > 0.0
                {
                    let pulled = self.navigation.drawer_pull_y;
                    self.navigation.drawer_dragging = false;
                    self.navigation.drawer_pull_y = 0.0;
                    self.navigation.drawer_slide.value = pulled;
                    self.navigation.drawer_last_tick = Some(std::time::Instant::now());
                    if pulled > 140.0 || vy > 500.0 {
                        self.navigation.drawer_closing = true;
                        self.navigation
                            .drawer_slide
                            .retarget_with_velocity(self.output.height as f32, vy.max(0.0));
                    } else {
                        self.navigation.drawer_closing = false;
                        self.navigation.drawer_slide.retarget(0.0);
                    }
                    self.needs_present = true;
                } else if self.foreground == Foreground::Shell
                    && self.nav.current == Screen::Home
                    && (dy < -64.0 || (dy < -28.0 && vy <= -350.0))
                    && -dy > dx.abs() * 1.4
                {
                    self.home_drag_cancelled();
                    let from_y = (self.output.height as f32 + dy).max(0.0);
                    self.open_drawer_animated(Some(from_y), vy);
                } else {
                    self.home_drag_released(vx);
                }
                return;
            }
            GestureEvent::HoldDrag { pos } => {
                self.home_icon_dragged(pos);
                return;
            }
            GestureEvent::HoldDrop { pos } => {
                self.home_icon_dropped(pos);
                return;
            }
            GestureEvent::Cancelled => {
                self.clear_back_feedback();
                self.panel_cancelled();
                self.home_drag_cancelled();
                if self.foreground == Foreground::Shell
                    && self.nav.current == Screen::Drawer
                    && self.navigation.drawer_dragging
                    && self.navigation.drawer_pull_y == 0.0
                {
                    self.release_drawer_drag(0.0);
                }
                if self.nav.current == Screen::Recents {
                    self.card_dismiss_release(0.0, 0.0);
                    self.release_recents_pager(0.0);
                }
                self.release_home_drag(false);
                return;
            }
            GestureEvent::Recognized(gesture) => gesture,
            GestureEvent::Progress { .. } => return,
        };
        self.clear_back_feedback();
        match g {
            Gesture::Home | Gesture::EdgeBack { .. } if self.panels.shown() => self.panel_close(),
            Gesture::Home => {
                crate::haptics::pulse(crate::haptics::HapticKind::NavCommit);
                if self.foreground == Foreground::App {
                    self.minimize_foreground();
                } else {
                    self.show_shell(Screen::Home);
                }
            }
            Gesture::QuickSwitch { to_previous } => {
                crate::haptics::pulse(crate::haptics::HapticKind::QuickSwitch);
                if !self.panels.shown() {
                    let windows = self.recent_windows();
                    if !windows.is_empty() {
                        let target = if self.foreground == Foreground::App {
                            if windows.len() >= 2 {
                                self.capture_topmost();
                                if to_previous {
                                    windows[1].clone()
                                } else {
                                    windows[windows.len() - 1].clone()
                                }
                            } else {
                                windows[0].clone()
                            }
                        } else if to_previous {
                            windows[0].clone()
                        } else {
                            windows[windows.len() - 1].clone()
                        };
                        self.morph = None;
                        self.launch.cancel();
                        self.show_app_window(&target);
                    }
                }
            }
            Gesture::EdgeBack { .. } => {
                if matches!(self.foreground, Foreground::App) {
                    // Android: Back belongs to the app (owner decision
                    // 2026-11-19, docs/AUDIT-UIUX-2026-11-19.md Q1).
                    self.app_back(time_ms);
                } else if self.nav.current == Screen::Recents {
                    let windows = self.recent_windows();
                    if self.navigation.recents_from_app && self.morph.is_none() {
                        if let Some(window) = windows.into_iter().next() {
                            let size = de_api::Size::new(
                                self.output.width as i32,
                                self.output.height as i32,
                            );
                            let pos = self.navigation.recents_pager.pos();
                            self.release_recents_pager(0.0);
                            self.navigation.recents_fade = None;
                            self.restore_recent(window, de_ui::recents::card_at(size, 0, pos));
                        } else {
                            self.show_shell(Screen::Home);
                        }
                    } else {
                        self.show_shell(Screen::Home);
                    }
                } else if self.nav.current == Screen::Drawer {
                    self.shell_im.set_visible(false);
                    self.navigation.drawer_dragging = false;
                    self.navigation.drawer_closing = true;
                    self.navigation.drawer_last_tick = Some(std::time::Instant::now());
                    let h = self.output.height.max(1) as f32;
                    self.navigation
                        .drawer_slide
                        .retarget_with_velocity(h, 1200.0);
                    self.needs_present = true;
                } else if self.nav.current == Screen::Settings && self.navigation.settings_keyboard
                {
                    self.navigation.settings_keyboard = false;
                    self.shell_dirty = true;
                    self.needs_present = true;
                } else {
                    self.launch.cancel();
                    self.morph = None;
                    self.nav.back();
                    self.shell_dirty = true;
                    self.needs_present = true;
                }
            }
            Gesture::Tap { .. } if self.panels.shown() => {
                if let Gesture::Tap { pos } = g {
                    self.panel_tap(pos);
                }
            }
            Gesture::Tap { pos } => {
                let size = de_api::Size::new(self.output.width as i32, self.output.height as i32);
                if self.shot_fx.tap(pos, size, Instant::now()) {
                    self.needs_present = true;
                } else if !self.release_home_drag(false) {
                    self.on_shell_tap(pos);
                }
            }
            // Long-press is the icon-context-menu hook (hide/pin) — no-op
            // until the drawer action sheet lands.
            Gesture::LongPress { pos } => {
                crate::haptics::pulse(crate::haptics::HapticKind::LongPress);
                if !self.home_lift_icon(pos) {
                    self.open_home_menu(pos);
                }
            }
        }
    }

    /// Switch the screen to a shell surface (app keeps running).
    pub(crate) fn show_shell(&mut self, screen: de_ui::nav::Screen) {
        use crate::state::Foreground;
        use de_ui::nav::Screen;
        let from_overview = self.nav.current == Screen::Recents
            && self.foreground == Foreground::Shell
            && self.morph.is_none();
        let animate_overview = !self.motion_clock.motion_restricted();
        if screen == Screen::Home && from_overview && animate_overview {
            self.fade_recents(false);
            return;
        }
        if screen == Screen::Recents && !from_overview {
            self.navigation.recents_from_app = self.foreground == Foreground::App
                || self.morph.as_ref().is_some_and(|m| m.window.is_some());
        }
        if screen == Screen::Recents {
            if self
                .morph
                .as_ref()
                .is_some_and(|m| m.phase == crate::morph::Phase::Recents)
                || self.release_home_to_recents()
            {
                return;
            }
            if from_overview {
                if self.navigation.recents_fade.is_some() {
                    self.fade_recents(true);
                }
                return;
            }
            self.reset_recents_pager();
            self.prepare_recents_backdrop();
        }
        let fade_in =
            screen == Screen::Recents && self.foreground == Foreground::Shell && animate_overview;
        self.navigation.recents_fade = None;
        if matches!(self.foreground, Foreground::App) {
            // The window is still presented — grab its last frame for the
            // recents card before the shell takes over.
            self.capture_topmost();
        }
        self.morph = None;
        self.launch.cancel();
        self.foreground = Foreground::Shell;
        self.focus_foreground();
        self.nav.current = screen;
        if fade_in {
            self.fade_recents(true);
        }
        self.shell_dirty = true;
        self.needs_present = true;
    }

    /// Tap on a shell screen: icon → launch, search pill → search,
    /// Every gesture while the lock covers the screen: drag-up unlocks,
    /// taps hit the bottom actions (or the power menu when it floats above
    /// the lock), back dismisses the menu overlay.
    fn lock_gesture(&mut self, ev: GestureEvent) {
        let h = self.output.height as f32;
        match ev {
            GestureEvent::Started { pos, .. } => {
                self.lock.begin_drag();
                if self.power.menu_over_lock {
                    self.power_menu_press(pos);
                }
            }
            GestureEvent::Progress { dy, .. } => {
                if !self.power.menu_over_lock {
                    self.lock.drag(dy, h);
                }
            }
            GestureEvent::Released { vy, .. } | GestureEvent::NavReleased { vy, .. } => {
                self.lock.release(vy, h);
                self.power_menu_release();
            }
            GestureEvent::Cancelled => {
                self.lock.cancel_drag();
                self.power_menu_release();
            }
            GestureEvent::Recognized(Gesture::Tap { pos }) => {
                self.lock.cancel_drag();
                if self.power.menu_over_lock {
                    self.power_menu_tap(pos, true);
                } else {
                    self.lock_tap(pos);
                }
            }
            GestureEvent::Recognized(Gesture::EdgeBack { .. }) if self.power.menu_over_lock => {
                self.lock.cancel_drag();
                self.power.menu_over_lock = false;
            }
            // The Recents preview means nothing on the lock screen.
            GestureEvent::NavPaused | GestureEvent::NavResumed => {}
            _ => self.lock.cancel_drag(),
        }
        self.needs_present = true;
    }

    /// recents card → back to the app.
    fn on_shell_tap(&mut self, pos: de_api::Point) {
        use crate::state::Foreground;
        use de_ui::nav::Screen;
        if !matches!(self.foreground, Foreground::Shell) {
            return;
        }
        let size = de_api::Size::new(self.output.width as i32, self.output.height as i32);
        match self.nav.current {
            Screen::Home => {
                let prefs = self.app_prefs.prefs().clone();
                let ordered: Vec<String> = self
                    .catalog
                    .home_order(&prefs)
                    .iter()
                    .map(|e| e.id.clone())
                    .collect();
                let on_page = ordered
                    .len()
                    .saturating_sub(self.home_page * de_ui::home::PAGE_SIZE)
                    .min(de_ui::home::PAGE_SIZE);
                let dock: Vec<String> = self
                    .catalog
                    .dock(&prefs)
                    .iter()
                    .map(|e| e.id.clone())
                    .collect();
                match de_ui::home::hit_test(size, pos, on_page, dock.len()) {
                    Some(de_ui::home::HomeHit::Grid(i)) => {
                        let id = ordered[self.home_page * de_ui::home::PAGE_SIZE + i].clone();
                        self.try_launch(
                            &id,
                            de_ui::home::icon_rect(size, de_ui::home::HomeHit::Grid(i), dock.len()),
                        );
                    }
                    Some(de_ui::home::HomeHit::Dock(i)) => {
                        if let Some(id) = dock.get(i).cloned() {
                            self.try_launch(
                                &id,
                                de_ui::home::icon_rect(
                                    size,
                                    de_ui::home::HomeHit::Dock(i),
                                    dock.len(),
                                ),
                            );
                        }
                    }
                    Some(de_ui::home::HomeHit::Drawer) => {
                        self.open_drawer_animated(None, 0.0);
                    }
                    Some(de_ui::home::HomeHit::Search) => {
                        self.nav.go(Screen::Search);
                        self.shell_im.set_visible(true);
                        self.shell_dirty = true;
                        self.needs_present = true;
                    }
                    None => {}
                }
            }
            Screen::Search => {
                let ids = self
                    .catalog
                    .visible(self.app_prefs.prefs())
                    .map(|e| e.id.clone())
                    .collect::<Vec<_>>();
                let rows = self.search_row_ids(&ids);
                if let Some(i) = de_ui::search::hit_test(size, &rows, pos) {
                    self.try_launch(&rows[i].id, de_ui::search::icon_rect(size, &rows, i));
                }
            }
            Screen::Drawer => {
                let recent = self.recent_drawer_icons();
                let ids = self.visible_ids();
                let apps = self.app_icons(&ids);
                let query = self.navigation.search_query.clone();
                match de_ui::drawer::hit_test_with_query(
                    size,
                    &recent,
                    &apps,
                    &query,
                    self.navigation.drawer_scroll,
                    pos,
                ) {
                    Some(de_ui::drawer::DrawerHit::App(id)) => {
                        let cx = de_api::Rect::new(size.w / 2 - 56, size.h / 2 - 56, 112, 112);
                        self.try_launch(&id, Some(cx));
                    }
                    Some(de_ui::drawer::DrawerHit::Search) => {
                        self.shell_im.set_visible(true);
                        self.shell_dirty = true;
                        self.needs_present = true;
                    }
                    Some(de_ui::drawer::DrawerHit::Letter(letter)) => {
                        if let Some(y) = de_ui::drawer::scroll_for_letter(&apps, &letter) {
                            self.navigation.drawer_scroll = y;
                            self.shell_dirty = true;
                            self.needs_present = true;
                        }
                    }
                    None => {}
                }
            }
            Screen::Error => match de_ui::error::hit_test(size, pos) {
                Some(de_ui::error::ErrorHit::Wait) if self.space.elements().next().is_some() => {
                    self.launch_error = None;
                    self.foreground = Foreground::App;
                    self.focus_foreground();
                    self.needs_present = true;
                }
                Some(de_ui::error::ErrorHit::Wait | de_ui::error::ErrorHit::Close) => {
                    self.launch_error = None;
                    self.show_shell(Screen::Home);
                }
                None => {}
            },
            Screen::Settings => {
                if self.navigation.settings_keyboard {
                    let prefs = self.app_prefs.prefs();
                    let active = de_config::keyboards::resolve_active_keyboard(prefs);
                    let options =
                        de_config::keyboards::discover_keyboards(prefs.allow_third_party_keyboards)
                            .into_iter()
                            .map(|k| de_ui::settings::keyboard::KeyboardOption {
                                active: k.id == active.id,
                                id: k.id,
                                name: k.name,
                                third_party: k.third_party,
                                installed: k.installed,
                            })
                            .collect::<Vec<_>>();
                    match de_ui::settings::keyboard::hit_test(size, &options, pos) {
                        Some(de_ui::settings::keyboard::KeyboardHit::Back) => {
                            self.navigation.settings_keyboard = false;
                            self.shell_dirty = true;
                            self.needs_present = true;
                        }
                        Some(de_ui::settings::keyboard::KeyboardHit::ToggleThirdParty) => {
                            let next = !self.app_prefs.prefs().allow_third_party_keyboards;
                            if self.app_prefs.set_allow_third_party_keyboards(next) {
                                self.restart_osk();
                                self.shell_dirty = true;
                                self.needs_present = true;
                            }
                        }
                        Some(de_ui::settings::keyboard::KeyboardHit::Select(i)) => {
                            if let Some(opt) = options.get(i) {
                                if self.app_prefs.set_keyboard(&opt.id) {
                                    self.restart_osk();
                                    self.shell_dirty = true;
                                    self.needs_present = true;
                                }
                            }
                        }
                        None => {}
                    }
                } else if let Some(idx) = de_ui::settings::index::hit_test(size, pos) {
                    match de_ui::settings::index::section_key(idx) {
                        Some("settings.display") => {
                            let next =
                                de_paint::wallpaper::next_id(&self.app_prefs.prefs().wallpaper_id);
                            if self.app_prefs.set_wallpaper(next) {
                                self.wallpaper = None;
                                self.shell_dirty = true;
                                self.needs_present = true;
                            }
                        }
                        Some("settings.keyboard") => {
                            self.navigation.settings_keyboard = true;
                            self.shell_dirty = true;
                            self.needs_present = true;
                        }
                        _ => {}
                    }
                }
            }
            Screen::Power => self.power_menu_tap(pos, false),
            Screen::Recents => {
                if self.morph.is_some() {
                    return;
                }
                let windows = self.recent_windows();
                let index = self.navigation.recents_pager.pos();
                self.release_recents_pager(0.0);
                match de_ui::recents::hit_test(size, windows.len(), index, pos) {
                    Some(de_ui::recents::RecentsHit::ClearAll) => {
                        for w in windows {
                            if let Some(t) = w.toplevel() {
                                t.send_close();
                            }
                        }
                        self.show_shell(Screen::Home);
                    }
                    Some(de_ui::recents::RecentsHit::Card(idx)) => {
                        if let Some(window) = windows.into_iter().nth(idx) {
                            self.navigation.recents_fade = None;
                            self.restore_recent(window, de_ui::recents::card_at(size, idx, index));
                        }
                    }
                    None => {}
                }
            }
            _ => {}
        }
    }

    /// `SearchRow` ids only (icon pixels are a render concern).
    fn search_row_ids(&self, ids: &[String]) -> Vec<de_ui::search::SearchRow> {
        ids.iter()
            .map(|id| de_ui::search::SearchRow {
                kind: de_ui::search::SearchKind::App,
                id: id.clone(),
                title: self
                    .catalog
                    .find(id)
                    .map_or_else(|| id.clone(), |e| e.name.clone()),
                subtitle: String::new(),
                icon: None,
            })
            .collect()
    }

    /// Spawn an app through the launch contract; failures go to the error
    /// screen, the pending launch resolves on `new_toplevel`.
    pub fn try_launch(&mut self, desktop_id: &str, source: Option<de_api::Rect>) {
        self.launch.cancel();
        self.morph = None;
        if let Some(window) = self.recent_windows().into_iter().find(|w| {
            w.toplevel().is_some_and(|t| {
                let id = crate::snapshots::window_app_id(t);
                id.trim_end_matches(".desktop") == desktop_id.trim_end_matches(".desktop")
            })
        }) {
            self.launch_error = None;
            if let Some(rect) = source.filter(|_| !self.motion_clock.motion_restricted()) {
                self.begin_launch_morph(desktop_id, rect);
                if let Some(morph) = self.morph.as_mut() {
                    morph.reuse_window(window);
                }
                self.needs_present = true;
            } else {
                self.show_app_window(&window);
            }
            return;
        }
        match self.launch.begin(desktop_id) {
            Ok(pid) => {
                eprintln!("launch: {desktop_id} pid={pid}");
                self.launch_error = None;
                if let Some(rect) = source {
                    self.begin_launch_morph(desktop_id, rect);
                }
                self.needs_present = true;
            }
            Err(e) => {
                self.launch_error = Some(format!("{desktop_id}: {e}"));
                self.foreground = crate::state::Foreground::Shell;
                self.focus_foreground();
                self.nav.go(de_ui::nav::Screen::Error);
                self.shell_dirty = true;
                self.needs_present = true;
            }
        }
    }

    /// Hit-test in the same order used to compose the output.
    fn surface_under(&self, pos: Point<f64, Logical>) -> Option<SurfaceHit> {
        if let Some(hit) = self.layer_under(pos, true) {
            return Some(hit);
        }
        if self.foreground == crate::state::Foreground::Shell {
            return None;
        }
        if self.morph.is_none() {
            if let Some((window, loc)) = self.space.element_under(pos) {
                if let Some((surface, offset)) =
                    window.surface_under(pos - loc.to_f64(), WindowSurfaceType::ALL)
                {
                    return Some(SurfaceHit {
                        surface,
                        location: (loc + offset).to_f64(),
                        keyboard_target: window.toplevel().map(|t| t.wl_surface().clone()),
                        above_windows: false,
                    });
                }
            }
            return self.layer_under(pos, false);
        }
        None
    }
}

/// Should a new contact be claimed by the shell instead of the client?
///
/// A contact is claimed when it lands outside the client content area —
/// any system zone counts — or anywhere while a shell screen is in the
/// foreground, *unless* it landed on an interactive top/overlay layer
/// (the OSK owns its region even when the shell is up).
fn claim_touch(zone: Zone, shell_foreground: bool, layer_contact: bool) -> bool {
    !layer_contact && (zone != Zone::Content || shell_foreground)
}

/// Scroll delta in logical px for shell surfaces, `(dx, dy)` with positive
/// = scroll down/right. `AxisFrame.axis`/`v120` are `(horizontal,
/// vertical)`; wheel events arrive in 1/120-notch units → 56 px per notch,
/// while finger/continuous sources carry their raw values through.
fn wheel_delta(frame: &AxisFrame) -> (f32, f32) {
    const NOTCH_PX: f32 = 56.0;
    if let Some((h120, v120)) = frame.v120 {
        (
            h120 as f32 / 120.0 * NOTCH_PX,
            v120 as f32 / 120.0 * NOTCH_PX,
        )
    } else {
        (frame.axis.0 as f32, frame.axis.1 as f32)
    }
}

/// smithay logical point → `de-api` point (physical px, i32).
fn api_point(p: Point<f64, Logical>) -> de_api::Point {
    de_api::Point::new(p.x as i32, p.y as i32)
}

/// Variant name for the `DE_INPUT_DEBUG` bring-up log.
fn variant_name<B: InputBackend>(e: &InputEvent<B>) -> &'static str {
    match e {
        InputEvent::DeviceAdded { .. } => "device_added",
        InputEvent::DeviceRemoved { .. } => "device_removed",
        InputEvent::Keyboard { .. } => "keyboard",
        InputEvent::PointerMotion { .. } => "pointer_motion",
        InputEvent::PointerMotionAbsolute { .. } => "pointer_motion_abs",
        InputEvent::PointerButton { .. } => "pointer_button",
        InputEvent::PointerAxis { .. } => "pointer_axis",
        InputEvent::TouchDown { .. } => "touch_down",
        InputEvent::TouchMotion { .. } => "touch_motion",
        InputEvent::TouchUp { .. } => "touch_up",
        InputEvent::TouchCancel { .. } => "touch_cancel",
        InputEvent::TouchFrame { .. } => "touch_frame",
        InputEvent::TabletToolAxis { .. } => "tablet_axis",
        InputEvent::TabletToolProximity { .. } => "tablet_prox",
        InputEvent::TabletToolTip { .. } => "tablet_tip",
        InputEvent::TabletToolButton { .. } => "tablet_button",
        InputEvent::SwitchToggle { .. } => "switch",
        _ => "other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The claim decision decides who sees a contact. Layer surfaces with
    /// their own interactive region (OSK, overlays) always win; otherwise
    /// system zones are shell-only, and the content zone is shell-only only
    /// while a shell screen is foreground.
    #[test]
    fn claim_touch_decision_table() {
        for zone in [
            Zone::NavBar,
            Zone::TopEdge,
            Zone::EdgeLeft,
            Zone::EdgeRight,
            Zone::Content,
        ] {
            assert!(!claim_touch(zone, false, true), "layer owns {zone:?}");
            assert!(
                !claim_touch(zone, true, true),
                "layer owns {zone:?} (shell fg)"
            );
        }
        for zone in [Zone::NavBar, Zone::TopEdge, Zone::EdgeLeft, Zone::EdgeRight] {
            assert!(claim_touch(zone, false, false), "{zone:?} is shell-only");
            assert!(claim_touch(zone, true, false), "{zone:?} is shell-only");
        }
        // Content zone: reaches the client only while an app is foreground.
        assert!(!claim_touch(Zone::Content, false, false));
        assert!(claim_touch(Zone::Content, true, false));
    }
}
