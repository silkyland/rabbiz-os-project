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

mod gestures;
mod keyboard;
mod pointer;
mod shell;
mod touch;

use smithay::backend::input::{
    AbsolutePositionEvent, Axis, AxisSource, Device as _, DeviceCapability, Event, InputBackend,
    InputEvent, PointerAxisEvent, PointerButtonEvent, PointerMotionEvent,
};
use smithay::desktop::WindowSurfaceType;
use smithay::input::pointer::AxisFrame;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point};

use crate::state::De;

pub(crate) struct SurfaceHit {
    pub surface: WlSurface,
    pub location: Point<f64, Logical>,
    pub keyboard_target: Option<WlSurface>,
    pub above_windows: bool,
}

impl SurfaceHit {
    pub(super) fn into_focus(self) -> (WlSurface, Point<f64, Logical>) {
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
            InputEvent::TouchCancel { event } => self.on_touch_cancel::<B>(&event),
            // wl_touch.frame terminates a burst of touch events; clients like
            // Chromium buffer touch points and only dispatch on frame, so
            // hardware frames must reach the seat (injected events frame
            // themselves per-burst in inject_touch_*).
            InputEvent::TouchFrame { .. } => {
                let touch = self.touch.clone();
                touch.frame(self);
            }
            InputEvent::PointerMotion { event } => {
                let pos = self.pointer.current_location() + event.delta();
                self.pointer_motion(event.time_msec(), pos, event.delta());
            }
            InputEvent::PointerMotionAbsolute { event } => {
                let pos = self.hw_point(event.position_transformed(self.rotation.native_size()));
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

    /// Track hot-plugged input devices and maintain `hw_keyboard` /
    /// `hw_pointer` (OSK suppression, cursor policy). libinput reports the
    /// platform button devices (power/volume keys) with keyboard
    /// capability too, and its `usb_id()` returns non-zero `0001:0001` for
    /// platform `gpio-keys`, so "hardware" requires an external bus in
    /// sysfs (`/usb<N>/` with non-zero VID:PID, or `/uhid/` / `/bluetooth/`).
    fn input_device<B: InputBackend>(&mut self, device: &B::Device, added: bool) {
        let syspath = device.syspath();
        if !is_external_bus(syspath.as_deref(), device.usb_id()) {
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

    /// Hit-test in the same order used to compose the output.
    pub(super) fn surface_under(&self, pos: Point<f64, Logical>) -> Option<SurfaceHit> {
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

/// smithay logical point → `de-api` point (physical px, i32).
pub(super) fn api_point(p: Point<f64, Logical>) -> de_api::Point {
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

/// Classify whether a libinput device comes from an external USB or Bluetooth
/// bus rather than built-in platform buttons (`gpio-keys` reports `0001:0001`
/// through `libinput::Device::id_vendor`/`id_product`) or virtual uinput nodes.
fn is_external_bus(syspath: Option<&std::path::Path>, usb_id: Option<(u32, u32)>) -> bool {
    let Some(path) = syspath else {
        return false;
    };
    let s = path.to_string_lossy();
    if s.contains("/uhid/") || s.contains("/bluetooth/") {
        return true;
    }
    let on_usb = path.components().any(|c| {
        c.as_os_str().to_str().is_some_and(|seg| {
            seg.len() > 3
                && seg.starts_with("usb")
                && seg[3..].bytes().all(|b| b.is_ascii_digit())
        })
    });
    on_usb && usb_id.is_some_and(|(vid, pid)| vid != 0 && pid != 0)
}

#[cfg(test)]
mod tests {
    use de_ui::gestures::Zone;

    use super::touch::claim_touch;

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

    /// The touch→pointer bridge is opt-in: only app_ids listed in
    /// `DE_TOUCH_AS_POINTER` get emulated pointer events; everyone else
    /// (including third-party touch-aware clients like Chrome) gets real
    /// `wl_touch`.
    #[test]
    fn pointer_bridge_app_denylist() {
        use super::touch::pointer_bridge_app;
        let list = "sun-awt-X11, com.example.legacy";
        assert!(pointer_bridge_app("sun-awt-X11-XFramePeer", list));
        assert!(pointer_bridge_app("com.example.legacy", list));
        assert!(!pointer_bridge_app("google-chrome", list));
        assert!(!pointer_bridge_app("rabbiz-sms", list));
        assert!(!pointer_bridge_app("sun-awt-X11-XFramePeer", ""));
        assert!(!pointer_bridge_app("", list));
    }

    #[test]
    fn external_bus_rejects_platform_gpio_keys_and_accepts_usb_and_bt() {
        use std::path::Path;
        use super::is_external_bus;

        // A21s volume buttons: platform bus with non-zero vendor/product 0001:0001.
        assert!(!is_external_bus(
            Some(Path::new("/sys/devices/platform/gpio-keys/input/input2/event2")),
            Some((1, 1)),
        ));
        // A21s power key & virtual uinput.
        assert!(!is_external_bus(
            Some(Path::new("/sys/devices/platform/s2mpu12-power-keys/input/input1/event1")),
            Some((0, 0)),
        ));
        assert!(!is_external_bus(
            Some(Path::new("/sys/devices/virtual/input/input7/event7")),
            Some((1, 1)),
        ));
        // Real USB keyboard over OTG.
        assert!(is_external_bus(
            Some(Path::new(
                "/sys/devices/platform/soc@0/13600000.usb/xhci-hcd.0.auto/usb1/1-1/1-1:1.0/0003:046D:C52B.0001/input/input5/event5"
            )),
            Some((0xC52B, 0x046D)),
        ));
        // Bluetooth HID keyboard over uhid.
        assert!(is_external_bus(
            Some(Path::new(
                "/sys/devices/virtual/misc/uhid/0005:05AC:024F.0001/input/input6/event6"
            )),
            Some((0x024F, 0x05AC)),
        ));
    }
}
