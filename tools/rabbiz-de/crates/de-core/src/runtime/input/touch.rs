//! Raw touch event dispatch (down/motion/up/cancel), shell contact claiming,
//! and gesture tracker feeding.

use de_ui::gestures::Zone;
use smithay::backend::input::{AbsolutePositionEvent, Event, InputBackend, TouchEvent as _};
use smithay::input::touch::{DownEvent, MotionEvent as TouchMotion, UpEvent};
use smithay::reexports::wayland_server::Resource;
use smithay::utils::{Logical, Point, Size, SERIAL_COUNTER as SCOUNTER};

use super::{api_point, SurfaceHit};
use crate::state::De;

impl De {
    pub(super) fn on_touch_down<B: InputBackend>(&mut self, evt: &B::TouchDownEvent) {
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
        // PIN bouncer owns every touch while open (unless power menu floats above it).
        if self.lock.shown() && self.lock.bouncer_open && !self.power.menu_over_lock {
            self.touch_claimed = true;
            return;
        }
        // Floating HUN banner sits in DisplayArea::Overlay above apps/shell.
        if !self.locked() && !self.is_session_locked() {
            let out = de_api::Size::new(self.output.width as i32, self.output.height as i32);
            let now_ms = self.uptime_ms(std::time::Instant::now());
            if self
                .hun_fx
                .on_touch_down(i32::from(evt.slot()), api_point(pos), out, now_ms)
            {
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

    pub(super) fn on_touch_motion<B: InputBackend>(&mut self, evt: &B::TouchMotionEvent) {
        let pos = evt.position_transformed(Size::from((
            self.output.width as i32,
            self.output.height as i32,
        )));
        self.last_touch_pos = pos;
        if self.sleeping() {
            return;
        }
        if self.lock.shown() && self.lock.bouncer_open && !self.power.menu_over_lock {
            return;
        }
        if self
            .hun_fx
            .on_touch_motion(i32::from(evt.slot()), api_point(pos))
        {
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

    pub(super) fn on_touch_up<B: InputBackend>(&mut self, evt: &B::TouchUpEvent) {
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
        if let Some(action) =
            self.hun_fx
                .on_touch_up(i32::from(evt.slot()), api_point(pos), out, now_ms)
        {
            self.touch_claimed = false;
            match action {
                crate::hun_ui::HunTouchAction::Dismissed(id) => {
                    self.dismiss_notification(id);
                }
                crate::hun_ui::HunTouchAction::Snoozed(_)
                | crate::hun_ui::HunTouchAction::TappedBody(_)
                | crate::hun_ui::HunTouchAction::TappedAction { .. }
                | crate::hun_ui::HunTouchAction::Released => {}
            }
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

    pub(super) fn on_touch_cancel<B: InputBackend>(&mut self, evt: &B::TouchCancelEvent) {
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
        if self.hun_fx.on_touch_cancel(i32::from(evt.slot()), now_ms) {
            self.touch_claimed = false;
            self.needs_present = true;
            return;
        }
        self.power_menu_release();
        self.feed_gesture(
            i32::from(evt.slot()),
            de_api::input::TouchPhase::Cancel,
            self.last_touch_pos,
            evt.time_msec(),
        );
        if !std::mem::take(&mut self.touch_claimed) {
            self.touch.clone().cancel(self);
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

/// Should a new contact be claimed by the shell instead of the client?
///
/// A contact is claimed when it lands outside the client content area —
/// any system zone counts — or anywhere while a shell screen is in the
/// foreground, *unless* it landed on an interactive top/overlay layer
/// (the OSK owns its region even when the shell is up).
pub(super) fn claim_touch(zone: Zone, shell_foreground: bool, layer_contact: bool) -> bool {
    !layer_contact && (zone != Zone::Content || shell_foreground)
}
