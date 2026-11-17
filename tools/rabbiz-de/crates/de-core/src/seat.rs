//! Seat forwarding: libinput events → smithay `Seat` handles or shell actions.
//!
//! Routing policy (hit-test, gesture recognition, grab semantics, power key)
//! lives in the pure `input` module; this file only translates between the
//! libinput/smithay event surface and that policy.

use smithay::backend::input::{
    AbsolutePositionEvent, ButtonState, Event, InputEvent, KeyState, KeyboardKeyEvent,
    PointerButtonEvent, PointerMotionEvent, TouchEvent,
};
use smithay::backend::libinput::LibinputInputBackend;
use smithay::input::keyboard::{FilterResult, Keysym};
use smithay::input::pointer::{ButtonEvent, MotionEvent as PointerMotion};
use smithay::input::touch::{
    DownEvent as TouchDownEvent, MotionEvent as TouchMotionEvent, UpEvent as TouchUpEvent,
};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point, SERIAL_COUNTER};

use crate::input::{
    hit_test, power_key_action, FocusTracker, HomeGesture, KeyKind, OutputPoint, ShellAction,
    ShellTarget,
};
use de_protocols::ProtocolState;

/// A decision the frame loop must act on outside the seat itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeatEffect {
    /// Close the top toplevel — the user swiped up from the gesture bar.
    GoHome,
    /// Release of XF86PowerOff: spawn `a21s-power-key toggle` detached.
    TogglePower,
}

/// Owns per-event state for the one compositor seat.
pub struct SeatController {
    pointer_location: Point<f64, Logical>,
    gesture: HomeGesture,
    focus: FocusTracker<WlSurface>,
    /// Bitmask of app-surface touch slots currently down; the last lift ends
    /// the grab. Slot `-1` (slot-less single-touch devices) maps to bit 0.
    app_touch_slots: u64,
}

impl SeatController {
    pub fn new(output_w: f64, output_h: f64) -> Self {
        Self {
            pointer_location: (output_w / 2.0, output_h / 2.0).into(),
            gesture: HomeGesture::new(),
            focus: FocusTracker::new(),
            app_touch_slots: 0,
        }
    }

    fn slot_bit(slot: smithay::backend::input::TouchSlot) -> u64 {
        let raw: i32 = slot.into();
        if raw <= 0 {
            1
        } else {
            1u64 << (raw.min(63) as u64)
        }
    }

    /// Window z-order (bottom-to-top) as surfaces, for focus decisions.
    fn z_order(state: &ProtocolState) -> Vec<WlSurface> {
        state
            .space
            .elements()
            .filter_map(|w| w.toplevel().map(|t| t.wl_surface().clone()))
            .collect()
    }

    /// The surface input currently targets: grab owner while a sequence is
    /// live, else the top mapped toplevel.
    fn focus_surface(&self, state: &ProtocolState) -> Option<WlSurface> {
        self.focus.focus(&Self::z_order(state)).cloned()
    }

    /// Handle one libinput event; returns shell actions for the frame loop.
    pub fn handle(
        &mut self,
        state: &mut ProtocolState,
        event: InputEvent<LibinputInputBackend>,
    ) -> Vec<SeatEffect> {
        match event {
            InputEvent::Keyboard { event } => self.keyboard(state, event),
            InputEvent::PointerMotionAbsolute { event } => {
                let pos = event.position_transformed(state.output_size.into());
                self.pointer_location = pos;
                self.pointer_motion(state, pos, event.time_msec());
                Vec::new()
            }
            InputEvent::PointerMotion { event } => {
                let delta = event.delta();
                self.pointer_location.x =
                    (self.pointer_location.x + delta.x).clamp(0.0, state.output_size.0 as f64);
                self.pointer_location.y =
                    (self.pointer_location.y + delta.y).clamp(0.0, state.output_size.1 as f64);
                let pos = self.pointer_location;
                self.pointer_motion(state, pos, event.time_msec());
                Vec::new()
            }
            InputEvent::PointerButton { event } => {
                let serial = SERIAL_COUNTER.next_serial();
                if event.state() == ButtonState::Pressed {
                    if let (Some(kb), Some(surface)) =
                        (state.seat.get_keyboard(), self.focus_surface(state))
                    {
                        kb.set_focus(state, Some(surface), serial);
                    }
                }
                if let Some(pointer) = state.seat.get_pointer() {
                    pointer.button(
                        state,
                        &ButtonEvent {
                            button: event.button_code(),
                            state: event.state(),
                            serial,
                            time: event.time_msec(),
                        },
                    );
                    pointer.frame(state);
                }
                Vec::new()
            }
            InputEvent::TouchDown { event } => self.touch_down(state, event),
            InputEvent::TouchMotion { event } => {
                let pos = event.position_transformed(state.output_size.into());
                let slot = event.slot();
                let slot_i: i32 = slot.into();
                if self.gesture.is_tracking(i64::from(slot_i)) {
                    self.gesture
                        .update(i64::from(slot_i), OutputPoint { x: pos.x, y: pos.y });
                } else if let Some(touch) = state.seat.get_touch() {
                    touch.motion(
                        state,
                        None,
                        &TouchMotionEvent {
                            slot,
                            location: pos,
                            time: event.time_msec(),
                        },
                    );
                }
                Vec::new()
            }
            InputEvent::TouchUp { event } => {
                let slot = event.slot();
                let slot_i: i32 = slot.into();
                if self.gesture.is_tracking(i64::from(slot_i)) {
                    return self
                        .gesture
                        .end(i64::from(slot_i))
                        .map(Self::effect_of)
                        .into_iter()
                        .collect();
                }
                self.app_touch_slots &= !Self::slot_bit(slot);
                if self.app_touch_slots == 0 {
                    self.focus.end_grab();
                }
                if let Some(touch) = state.seat.get_touch() {
                    touch.up(
                        state,
                        &TouchUpEvent {
                            slot,
                            serial: SERIAL_COUNTER.next_serial(),
                            time: event.time_msec(),
                        },
                    );
                    touch.frame(state);
                }
                Vec::new()
            }
            InputEvent::TouchFrame { .. } => {
                if let Some(touch) = state.seat.get_touch() {
                    touch.frame(state);
                }
                Vec::new()
            }
            InputEvent::TouchCancel { .. } => {
                self.gesture.cancel();
                self.app_touch_slots = 0;
                self.focus.end_grab();
                if let Some(touch) = state.seat.get_touch() {
                    touch.cancel(state);
                }
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn keyboard(
        &mut self,
        state: &mut ProtocolState,
        event: impl KeyboardKeyEvent<LibinputInputBackend>,
    ) -> Vec<SeatEffect> {
        let key_state = event.state();
        let time = event.time_msec();
        let serial = SERIAL_COUNTER.next_serial();
        let mut effects = Vec::new();
        if let Some(kb) = state.seat.get_keyboard() {
            if kb.current_focus().is_none() {
                if let Some(surface) = self.focus_surface(state) {
                    kb.set_focus(state, Some(surface), serial);
                }
            }
            kb.input::<(), _>(
                state,
                event.key_code(),
                key_state,
                serial,
                time,
                |_, _, keysym| {
                    let is_power = keysym.modified_sym() == Keysym::XF86_PowerOff
                        || keysym.raw_syms().contains(&Keysym::XF86_PowerOff);
                    let kind = if is_power {
                        KeyKind::PowerKey
                    } else {
                        KeyKind::Other
                    };
                    match power_key_action(kind, key_state == KeyState::Released) {
                        Some(action) => {
                            effects.push(Self::effect_of(action));
                            FilterResult::Intercept(())
                        }
                        None => {
                            if is_power {
                                FilterResult::Intercept(())
                            } else {
                                FilterResult::Forward
                            }
                        }
                    }
                },
            );
        }
        effects
    }

    fn touch_down(
        &mut self,
        state: &mut ProtocolState,
        event: impl smithay::backend::input::TouchDownEvent<LibinputInputBackend>,
    ) -> Vec<SeatEffect> {
        let pos = event.position_transformed(state.output_size.into());
        let slot = event.slot();
        let point = OutputPoint { x: pos.x, y: pos.y };

        // The bottom 32 px strip belongs to the shell, not the app.
        if hit_test(point, state.output_size.1 as f64) == ShellTarget::GestureBar {
            let slot_i: i32 = slot.into();
            self.gesture.begin(i64::from(slot_i), point);
            return Vec::new();
        }

        let serial = SERIAL_COUNTER.next_serial();
        if let Some(surface) = self.focus_surface(state) {
            self.focus.begin_grab(surface.clone());
            self.app_touch_slots |= Self::slot_bit(slot);
            if let Some(kb) = state.seat.get_keyboard() {
                kb.set_focus(state, Some(surface.clone()), serial);
            }
            if let Some(touch) = state.seat.get_touch() {
                touch.down(
                    state,
                    // The tuple's point is the surface origin in global
                    // space — a fullscreen app sits at (0,0).
                    Some((surface, (0.0, 0.0).into())),
                    &TouchDownEvent {
                        slot,
                        location: pos,
                        serial,
                        time: event.time_msec(),
                    },
                );
                // libinput never emits touch-frame events; clients (Qt)
                // dispatch on frame, so end each event batch explicitly.
                touch.frame(state);
            }
        }
        Vec::new()
    }

    fn pointer_motion(&self, state: &mut ProtocolState, pos: Point<f64, Logical>, time: u32) {
        if let Some(pointer) = state.seat.get_pointer() {
            let under = self.focus_surface(state).map(|s| (s, (0.0, 0.0).into()));
            pointer.motion(
                state,
                under,
                &PointerMotion {
                    location: pos,
                    serial: SERIAL_COUNTER.next_serial(),
                    time,
                },
            );
            pointer.frame(state);
        }
    }

    fn effect_of(action: ShellAction) -> SeatEffect {
        match action {
            ShellAction::GoHome => SeatEffect::GoHome,
            ShellAction::TogglePower => SeatEffect::TogglePower,
        }
    }
}
