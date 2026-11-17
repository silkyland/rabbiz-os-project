//! libinput event → seat dispatch (M1-T1/M1-T8).
//!
//! Touch carries libinput timestamps through to the seat so M2 gesture code
//! can compute release velocity; the power key spawns `a21s-power-key
//! toggle` detached on release.

use std::process::Command;

use smithay::backend::input::{
    AbsolutePositionEvent, Axis, AxisSource, Event, InputBackend, InputEvent, KeyState,
    KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent,
    PointerMotionEvent, TouchEvent as _,
};
use smithay::desktop::WindowSurfaceType;
use smithay::input::keyboard::{FilterResult, Keysym, ModifiersState};
use smithay::input::pointer::{AxisFrame, ButtonEvent, MotionEvent, RelativeMotionEvent};
use smithay::input::touch::{DownEvent, MotionEvent as TouchMotion, UpEvent};
use smithay::utils::{Logical, Point, SERIAL_COUNTER as SCOUNTER, Size};

use crate::state::De;

impl De {
    /// Dispatch one libinput event into the seat.
    pub fn process_input_event<B: InputBackend>(&mut self, event: InputEvent<B>) {
        match event {
            InputEvent::Keyboard { event } => self.on_keyboard_key::<B>(event),
            InputEvent::TouchDown { event } => self.on_touch_down::<B>(event),
            InputEvent::TouchMotion { event } => self.on_touch_motion::<B>(event),
            InputEvent::TouchUp { event } => self.on_touch_up::<B>(event),
            InputEvent::PointerMotion { event } => {
                let pos = self.pointer.current_location() + event.delta();
                self.pointer_motion::<B>(event.time_msec(), pos, event.delta())
            }
            InputEvent::PointerMotionAbsolute { event } => {
                let pos = event.position_transformed(Size::from((
                    self.output.width as i32,
                    self.output.height as i32,
                )));
                let delta = pos - self.pointer.current_location();
                self.pointer_motion::<B>(event.time_msec(), pos, delta)
            }
            InputEvent::PointerButton { event } => {
                let serial = SCOUNTER.next_serial();
                let ptr = self.pointer.clone();
                ptr.button(
                    self,
                    &ButtonEvent {
                        button: event.button_code(),
                        state: event.state(),
                        serial,
                        time: event.time_msec(),
                    },
                );
            }
            InputEvent::PointerAxis { event } => {
                let mut frame = AxisFrame::new(event.time_msec()).source(event.source());
                let v = event.amount(Axis::Vertical).unwrap_or(0.0);
                let h = event.amount(Axis::Horizontal).unwrap_or(0.0);
                frame = frame
                    .relative_direction(Axis::Vertical, event.relative_direction(Axis::Vertical))
                    .relative_direction(Axis::Horizontal, event.relative_direction(Axis::Horizontal));
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
                let ptr = self.pointer.clone();
                ptr.axis(self, frame);
            }
            _ => {}
        }
    }

    fn pointer_motion<B: InputBackend>(
        &mut self,
        time: u32,
        pos: Point<f64, Logical>,
        delta: Point<f64, Logical>,
    ) {
        let serial = SCOUNTER.next_serial();
        let under = self.surface_under(pos).map(|(s, l)| (s, l.to_f64()));
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
                utime: time as u64 * 1000,
            },
        );
    }

    fn on_keyboard_key<B: InputBackend>(&mut self, evt: B::KeyboardKeyEvent) {
        let serial = SCOUNTER.next_serial();
        let keycode = evt.key_code();
        let state = evt.state();
        let time = evt.time_msec();
        let Some(keyboard) = self.seat.get_keyboard() else {
            return;
        };
        keyboard.input(
            self,
            keycode,
            state,
            serial,
            time,
            |_de, _mods: &ModifiersState, keysym| {
                let raw = keysym.raw_syms().first().copied();
                // Power button: act on release only, detached so the 300 ms
                // combo guard in a21s-power-key never stalls the loop.
                if raw == Some(Keysym::XF86_PowerOff) && state == KeyState::Released {
                    let _ = Command::new("setsid")
                        .args(["a21s-power-key", "toggle"])
                        .spawn();
                    return FilterResult::Intercept(());
                }
                FilterResult::Forward
            },
        );
    }

    fn on_touch_down<B: InputBackend>(&mut self, evt: B::TouchDownEvent) {
        let pos = evt.position_transformed(Size::from((
            self.output.width as i32,
            self.output.height as i32,
        )));
        let serial = SCOUNTER.next_serial();
        let under = self.surface_under(pos).map(|(s, l)| (s, l.to_f64()));
        self.last_touch_pos = pos;
        if let Some((surface, _)) = &under {
            if let Some(kbd) = self.seat.get_keyboard() {
                kbd.set_focus(self, Some(surface.clone()), serial);
            }
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

    fn on_touch_motion<B: InputBackend>(&mut self, evt: B::TouchMotionEvent) {
        let pos = evt.position_transformed(Size::from((
            self.output.width as i32,
            self.output.height as i32,
        )));
        let under = self.surface_under(pos).map(|(s, l)| (s, l.to_f64()));
        self.last_touch_pos = pos;
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

    fn on_touch_up<B: InputBackend>(&mut self, evt: B::TouchUpEvent) {
        let serial = SCOUNTER.next_serial();
        let pos = self.last_touch_pos;
        let touch = self.touch.clone();
        touch.up(
            self,
            &UpEvent {
                slot: evt.slot(),
                serial,
                time: evt.time_msec(),
            },
        );
        // M1 gesture stub: a release inside the bottom gesture bar closes
        // the topmost window (real edge-back gesture is M2).
        if pos.y >= self.output.height as f64 - de_ui::shell_stub::GESTURE_BAR_H as f64 {
            if let Some(top) = self.space.elements().last().cloned() {
                if let Some(toplevel) = top.toplevel() {
                    toplevel.send_close();
                }
            }
        }
    }

    /// Top-most surface under `pos` (toplevel incl. popups/subsurfaces).
    pub fn surface_under(
        &self,
        pos: Point<f64, Logical>,
    ) -> Option<(
        smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
        Point<i32, Logical>,
    )> {
        let (window, loc) = self.space.element_under(pos)?;
        let rel = pos - loc.to_f64();
        window
            .surface_under(rel, WindowSurfaceType::ALL)
            .map(|(s, l)| (s, l + loc))
    }
}
