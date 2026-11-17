use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};

use smithay::backend::input::{
    AbsolutePositionEvent, ButtonState, Event, InputEvent, KeyState, KeyboardKeyEvent,
    PointerButtonEvent, PointerMotionEvent, TouchEvent,
};
use smithay::backend::libinput::{LibinputInputBackend, LibinputSessionInterface};
use smithay::backend::session::libseat::LibSeatSession;
use smithay::input::keyboard::{FilterResult, Keysym};
use smithay::input::pointer::{ButtonEvent, MotionEvent as PointerMotion};
use smithay::input::touch::{
    DownEvent as TouchDownEvent, MotionEvent as TouchMotionEvent, UpEvent as TouchUpEvent,
};
use smithay::input::{Seat, SeatHandler};
use smithay::reexports::input::Libinput;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point, Size, SERIAL_COUNTER};

/// Create a `LibinputInputBackend` bound to the active libseat session and seat name.
pub fn create_libinput_backend(
    session: &LibSeatSession,
    seat_name: &str,
) -> Result<LibinputInputBackend, String> {
    let mut libinput = Libinput::new_with_udev(LibinputSessionInterface::from(session.clone()));
    libinput
        .udev_assign_seat(seat_name)
        .map_err(|()| format!("libinput udev_assign_seat({seat_name}) failed"))?;
    Ok(LibinputInputBackend::new(libinput))
}

/// Spawn `/usr/local/bin/a21s-power-key toggle` detached (`setsid`, no wait)
/// so its 300 ms combo guard never stalls the compositor event loop.
pub fn spawn_power_key_toggle() {
    let mut cmd = Command::new("/usr/local/bin/a21s-power-key");
    cmd.arg("toggle")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    unsafe {
        cmd.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    if let Err(err) = cmd.spawn() {
        eprintln!("failed to spawn a21s-power-key toggle: {err}");
    }
}

/// Forward a single `LibinputInputBackend` event to the Smithay `Seat`.
pub fn handle_input_event<D>(
    state: &mut D,
    seat: &Seat<D>,
    focus_surface: Option<WlSurface>,
    output_size: Size<i32, Logical>,
    pointer_location: &mut Point<f64, Logical>,
    event: InputEvent<LibinputInputBackend>,
) where
    D: SeatHandler<KeyboardFocus = WlSurface, PointerFocus = WlSurface, TouchFocus = WlSurface>
        + 'static,
{
    match event {
        InputEvent::Keyboard { event } => {
            let keycode = event.key_code();
            let key_state = event.state();
            let time = event.time_msec();
            let serial = SERIAL_COUNTER.next_serial();
            if let Some(kb) = seat.get_keyboard() {
                if kb.current_focus().is_none() && focus_surface.is_some() {
                    kb.set_focus(state, focus_surface.clone(), serial);
                }
                let mut trigger_power = false;
                kb.input::<(), _>(state, keycode, key_state, serial, time, |_, _, keysym| {
                    let is_power = keysym.modified_sym() == Keysym::XF86_PowerOff
                        || keysym.raw_syms().contains(&Keysym::XF86_PowerOff);
                    if is_power {
                        if key_state == KeyState::Released {
                            trigger_power = true;
                        }
                        FilterResult::Intercept(())
                    } else {
                        FilterResult::Forward
                    }
                });
                if trigger_power {
                    spawn_power_key_toggle();
                }
            }
        }
        InputEvent::PointerMotionAbsolute { event } => {
            let pos = event.position_transformed(output_size);
            *pointer_location = pos;
            let serial = SERIAL_COUNTER.next_serial();
            let time = event.time_msec();
            if let Some(pointer) = seat.get_pointer() {
                let under = focus_surface.map(|s| (s, (0.0, 0.0).into()));
                pointer.motion(
                    state,
                    under,
                    &PointerMotion {
                        location: pos,
                        serial,
                        time,
                    },
                );
                pointer.frame(state);
            }
        }
        InputEvent::PointerMotion { event } => {
            let delta = event.delta();
            pointer_location.x = (pointer_location.x + delta.x).clamp(0.0, output_size.w as f64);
            pointer_location.y = (pointer_location.y + delta.y).clamp(0.0, output_size.h as f64);
            let pos = *pointer_location;
            let serial = SERIAL_COUNTER.next_serial();
            let time = event.time_msec();
            if let Some(pointer) = seat.get_pointer() {
                let under = focus_surface.map(|s| (s, (0.0, 0.0).into()));
                pointer.motion(
                    state,
                    under,
                    &PointerMotion {
                        location: pos,
                        serial,
                        time,
                    },
                );
                pointer.frame(state);
            }
        }
        InputEvent::PointerButton { event } => {
            let serial = SERIAL_COUNTER.next_serial();
            let time = event.time_msec();
            let button = event.button_code();
            let button_state = event.state();
            if button_state == ButtonState::Pressed {
                if let Some(kb) = seat.get_keyboard() {
                    kb.set_focus(state, focus_surface, serial);
                }
            }
            if let Some(pointer) = seat.get_pointer() {
                pointer.button(
                    state,
                    &ButtonEvent {
                        button,
                        state: button_state,
                        serial,
                        time,
                    },
                );
                pointer.frame(state);
            }
        }
        InputEvent::TouchDown { event } => {
            let pos = event.position_transformed(output_size);
            let serial = SERIAL_COUNTER.next_serial();
            let time = event.time_msec();
            let slot = event.slot();
            if let Some(kb) = seat.get_keyboard() {
                kb.set_focus(state, focus_surface.clone(), serial);
            }
            if let Some(touch) = seat.get_touch() {
                let under = focus_surface.map(|s| (s, (0.0, 0.0).into()));
                touch.down(
                    state,
                    under,
                    &TouchDownEvent {
                        slot,
                        location: pos,
                        serial,
                        time,
                    },
                );
            }
        }
        InputEvent::TouchMotion { event } => {
            let pos = event.position_transformed(output_size);
            let time = event.time_msec();
            let slot = event.slot();
            if let Some(touch) = seat.get_touch() {
                touch.motion(
                    state,
                    None,
                    &TouchMotionEvent {
                        slot,
                        location: pos,
                        time,
                    },
                );
            }
        }
        InputEvent::TouchUp { event } => {
            let serial = SERIAL_COUNTER.next_serial();
            let time = event.time_msec();
            let slot = event.slot();
            if let Some(touch) = seat.get_touch() {
                touch.up(state, &TouchUpEvent { slot, serial, time });
            }
        }
        InputEvent::TouchFrame { .. } => {
            if let Some(touch) = seat.get_touch() {
                touch.frame(state);
            }
        }
        InputEvent::TouchCancel { .. } => {
            if let Some(touch) = seat.get_touch() {
                touch.cancel(state);
            }
        }
        _ => {}
    }
}
