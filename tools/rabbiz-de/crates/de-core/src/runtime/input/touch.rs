//! Raw touch event dispatch (down/motion/up/cancel), shell contact claiming,
//! and gesture tracker feeding.

use de_ui::gestures::Zone;
use smithay::backend::input::{
    AbsolutePositionEvent, ButtonState, Event, InputBackend, TouchEvent as _, TouchSlot,
};
use smithay::input::touch::{DownEvent, MotionEvent as TouchMotion, UpEvent};
use smithay::reexports::wayland_server::Resource;
use smithay::utils::{Logical, Point, SERIAL_COUNTER as SCOUNTER};

use super::{api_point, SurfaceHit};
use crate::state::De;

impl De {
    pub(super) fn on_touch_down<B: InputBackend>(&mut self, evt: &B::TouchDownEvent) {
        let pos = self.hw_point(evt.position_transformed(self.rotation.native_size()));
        self.inject_touch_down(i32::from(evt.slot()), pos, evt.time_msec());
    }

    /// Inject a raw touch-down contact (shared by libinput and virtual/IPC touch injection).
    pub fn inject_touch_down(&mut self, slot: i32, pos: Point<f64, Logical>, time_ms: u32) {
        self.power_note_activity();
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
        // PIN bouncer owns every touch while open (unless power menu floats above it).
        if self.lock.shown() && self.lock.bouncer_open && !self.power.menu_over_lock {
            self.touch_claimed = true;
            return;
        }
        // Floating HUN banner sits in DisplayArea::Overlay above apps/shell.
        if !self.locked() && !self.is_session_locked() {
            let out = de_api::Size::new(self.output.width as i32, self.output.height as i32);
            let now_ms = self.uptime_ms(std::time::Instant::now());
            if self.hun_fx.on_touch_down(slot, api_point(pos), out, now_ms) {
                self.touch_claimed = true;
                self.needs_present = true;
                return;
            }
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
        if !self.rotation.policy.auto && !self.desktop_mode && !self.lock.shown() {
            if let Some(suggested) = self.rotation.sensed.filter(|&s| s != self.rotation.current) {
                let out_size =
                    de_api::Size::new(self.output.width as i32, self.output.height as i32);
                if de_ui::rotate_pill::hit(out_size, api_point(pos)) {
                    let _ = self.set_rotation_mode(suggested.name());
                    self.touch_claimed = true;
                    return;
                }
            }
        }
        // Top layers (including the OSK) own their interactive region.
        // Retain the nav strip as a way to leave the foreground client.
        let layer_contact = zone != Zone::NavBar
            && !self.panel_wants_touch()
            && hit.as_ref().is_some_and(|h| h.above_windows);
        if !layer_contact {
            self.feed_gesture(slot, de_api::input::TouchPhase::Down, pos, time_ms);
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
            self.touch_as_pointer = false;
            return;
        }
        let serial = SCOUNTER.next_serial();
        if let Some(target) = hit.as_ref().and_then(|hit| hit.keyboard_target.clone()) {
            self.focus_surface(Some(target), serial);
        }
        if slot == 0 && !layer_contact {
            let app_id = self
                .space
                .element_under(pos)
                .and_then(|(w, _)| w.toplevel().map(crate::snapshots::window_app_id))
                .unwrap_or_default();
            let pointer_list = std::env::var("DE_TOUCH_AS_POINTER").unwrap_or_default();
            if pointer_bridge_app(&app_id, &pointer_list) {
                self.touch_as_pointer = true;
                let delta = pos - self.pointer.current_location();
                self.pointer_motion(time_ms, pos, delta);
                self.pointer_button(time_ms, 0x110, ButtonState::Pressed);
                return;
            }
        }
        self.touch_as_pointer = false;
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
                slot: TouchSlot::from(Some(slot.max(0) as u32)),
                location: pos,
                serial,
                time: time_ms,
            },
        );
        touch.frame(self);
    }

    pub(super) fn on_touch_motion<B: InputBackend>(&mut self, evt: &B::TouchMotionEvent) {
        let pos = self.hw_point(evt.position_transformed(self.rotation.native_size()));
        self.inject_touch_motion(i32::from(evt.slot()), pos, evt.time_msec());
    }

    /// Inject a raw touch-motion event.
    pub fn inject_touch_motion(&mut self, slot: i32, pos: Point<f64, Logical>, time_ms: u32) {
        self.power_note_activity();
        self.last_touch_pos = pos;
        if self.sleeping() {
            return;
        }
        if self.lock.shown() && self.lock.bouncer_open && !self.power.menu_over_lock {
            return;
        }
        if self.hun_fx.on_touch_motion(slot, api_point(pos)) {
            return;
        }
        self.feed_gesture(slot, de_api::input::TouchPhase::Motion, pos, time_ms);
        if self.touch_claimed {
            return;
        }
        if slot == 0 && self.touch_as_pointer {
            let delta = pos - self.pointer.current_location();
            self.pointer_motion(time_ms, pos, delta);
            return;
        }
        let under = self.surface_under(pos).map(SurfaceHit::into_focus);
        let touch = self.touch.clone();
        touch.motion(
            self,
            under,
            &TouchMotion {
                slot: TouchSlot::from(Some(slot.max(0) as u32)),
                location: pos,
                time: time_ms,
            },
        );
        touch.frame(self);
    }

    pub(super) fn on_touch_up<B: InputBackend>(&mut self, evt: &B::TouchUpEvent) {
        self.inject_touch_up(i32::from(evt.slot()), evt.time_msec());
    }

    /// Inject a raw touch-up event.
    pub fn inject_touch_up(&mut self, slot: i32, time_ms: u32) {
        self.power_note_activity();
        let pos = self.last_touch_pos;
        if self.sleeping() {
            self.sleep_touch_up();
            self.touch_claimed = true;
            self.needs_present = true;
            return;
        }
        if self.lock.shown() && self.lock.bouncer_open && !self.power.menu_over_lock {
            self.touch_claimed = false;
            self.lock_bouncer_tap(api_point(pos));
            return;
        }
        let out = de_api::Size::new(self.output.width as i32, self.output.height as i32);
        let now_ms = self.uptime_ms(std::time::Instant::now());
        if let Some(action) = self.hun_fx.on_touch_up(slot, api_point(pos), out, now_ms) {
            self.touch_claimed = false;
            match action {
                crate::hun_ui::HunTouchAction::Dismissed(id) => {
                    self.dismiss_notification(id);
                }
                crate::hun_ui::HunTouchAction::TappedBody(id) => {
                    let is_files = self
                        .panels
                        .notifications
                        .iter()
                        .any(|n| n.id == id && n.app == "Files");
                    if is_files {
                        let _ = self.launch.begin("rabbiz-files");
                    }
                    self.dismiss_notification(id);
                }
                crate::hun_ui::HunTouchAction::Snoozed(_)
                | crate::hun_ui::HunTouchAction::TappedAction { .. }
                | crate::hun_ui::HunTouchAction::Released => {}
            }
            self.needs_present = true;
            return;
        }
        self.power_menu_release();
        self.feed_gesture(slot, de_api::input::TouchPhase::Up, pos, time_ms);
        let claimed = std::mem::take(&mut self.touch_claimed);
        if slot == 0 && std::mem::take(&mut self.touch_as_pointer) {
            self.pointer_button(time_ms, 0x110, ButtonState::Released);
            if claimed {
                self.needs_present = true;
            }
            return;
        }
        if claimed {
            self.needs_present = true;
            return;
        }
        let serial = SCOUNTER.next_serial();
        let touch = self.touch.clone();
        touch.up(
            self,
            &UpEvent {
                slot: TouchSlot::from(Some(slot.max(0) as u32)),
                serial,
                time: time_ms,
            },
        );
        touch.frame(self);
    }

    pub(super) fn on_touch_cancel<B: InputBackend>(&mut self, evt: &B::TouchCancelEvent) {
        self.inject_touch_cancel(i32::from(evt.slot()), evt.time_msec());
    }

    /// Inject a raw touch-cancel event.
    pub fn inject_touch_cancel(&mut self, slot: i32, time_ms: u32) {
        if self.sleeping() {
            self.sleep_touch_cancel();
            self.touch_claimed = true;
            return;
        }
        if self.lock.shown() && self.lock.bouncer_open && !self.power.menu_over_lock {
            self.touch_claimed = false;
            return;
        }
        let now_ms = self.uptime_ms(std::time::Instant::now());
        if self.hun_fx.on_touch_cancel(slot, now_ms) {
            self.touch_claimed = false;
            self.needs_present = true;
            return;
        }
        self.power_menu_release();
        self.feed_gesture(
            slot,
            de_api::input::TouchPhase::Cancel,
            self.last_touch_pos,
            time_ms,
        );
        let claimed = std::mem::take(&mut self.touch_claimed);
        if slot == 0 && std::mem::take(&mut self.touch_as_pointer) {
            self.pointer_button(time_ms, 0x110, ButtonState::Released);
            return;
        }
        if !claimed {
            let touch = self.touch.clone();
            touch.cancel(self);
            touch.frame(self);
        }
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
}

/// `DE_TOUCH_AS_POINTER` holds comma-separated app_id prefixes for the rare
/// client that cannot consume `wl_touch` (e.g. a pointer-only X11-style
/// toolkit). Everything else gets real touch events — desktop-class clients
/// that bind `wl_touch` (Chrome, GTK3+, Qt, Firefox) interpret a drag as a
/// scroll, whereas bridging them to `wl_pointer` turns the same drag into
/// button-held text selection.
pub(super) fn pointer_bridge_app(app_id: &str, list: &str) -> bool {
    list.split(',')
        .map(str::trim)
        .any(|p| !p.is_empty() && app_id.starts_with(p))
}

/// Should a new contact be claimed by the shell instead of the client?
///
/// A contact is claimed when it lands outside the client content area —
/// any system zone counts — or anywhere while a shell screen is in the
/// foreground, *unless* it landed on an interactive top/overlay layer
/// (the OSK owns its region even when the shell is up).
pub(super) fn claim_touch(zone: Zone, shell_foreground: bool, layer_contact: bool) -> bool {
    !layer_contact && (zone != Zone::Content || shell_foreground)
}
