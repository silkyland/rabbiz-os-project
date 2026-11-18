//! Input device inventory (Package 2.1): what is plugged in, from udev properties.
//!
//! The classification is a pure function over the properties udev already computed for each
//! `/dev/input/event*` node (`ID_INPUT_KEYBOARD`, `ID_INPUT_MOUSE`, ...), so it is host-testable and
//! needs no hardware. A thin Linux collector (udev enumeration plus a hotplug monitor) feeds
//! [`InputRegistry`]; the compositor publishes [`InputSummary`] through [`format_state`] so other
//! processes (the on-screen keyboard, settings) can react.
//!
//! Why udev and not libinput: libinput ignores game controllers entirely, and its "keyboard"
//! capability is true for any device with a key (the A21s volume and power buttons included).
//! udev sets `ID_INPUT_KEYBOARD` only for real keyboards, which is what "physical keyboard
//! attached" must mean. The A21s fixtures in the tests were read from the device on 2026-11-18; the
//! USB, Bluetooth and controller fixtures are typical udev output and were **not** captured from
//! hardware yet.

use std::collections::BTreeMap;
use std::io;
use std::path::Path;

/// One input class. A device can have several (a keyboard with a trackpad).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum InputKind {
    /// A real keyboard (not a power or volume button device).
    Keyboard,
    /// A mouse (`ID_INPUT_MOUSE`).
    Mouse,
    /// A touchpad.
    Touchpad,
    /// A touchscreen.
    Touchscreen,
    /// A game controller.
    Gamepad,
    /// A graphics tablet.
    Tablet,
}

/// Names of virtual (uinput) devices that are really our own bridges to physical hardware and must be
/// treated as physical. The BLE game controller bridge exists because the kernel lacks `CONFIG_UHID`.
const BRIDGED_NAMES: &[&str] = &["Xbox Wireless Controller"];

/// Test and tooling injectors that must never count as user hardware.
const IGNORED_NAMES: &[&str] = &["a21s-test-touch"];

/// Properties of one input node, as udev reports them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputProps {
    /// Sysfs device path (`DEVPATH`), used as the stable id.
    pub devpath: String,
    /// Human name of the device (sysfs `name`).
    pub name: String,
    /// udev properties (`ID_INPUT_*`, `ID_BUS`, ...).
    pub props: BTreeMap<String, String>,
}

impl InputProps {
    /// Build from a name and `KEY=value` pairs (tests and the udev collector use this).
    #[must_use]
    pub fn new(devpath: &str, name: &str, pairs: &[(&str, &str)]) -> Self {
        Self {
            devpath: devpath.to_string(),
            name: name.to_string(),
            props: pairs
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect(),
        }
    }

    fn flag(&self, key: &str) -> bool {
        self.props.get(key).is_some_and(|v| v == "1")
    }

    /// A kernel-virtual device (uinput and friends). Bluetooth HID over `uhid` is real hardware.
    fn is_virtual(&self) -> bool {
        self.devpath.contains("/devices/virtual/")
            && !self.devpath.contains("/devices/virtual/misc/uhid/")
    }
}

/// A connected, relevant input device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputDevice {
    /// Stable id (sysfs path).
    pub id: String,
    /// Device name.
    pub name: String,
    /// Classes, sorted and unique.
    pub kinds: Vec<InputKind>,
    /// `ID_BUS` (`usb`, `bluetooth`, `platform`, ...), if udev knows it.
    pub bus: Option<String>,
}

impl InputDevice {
    /// Whether the device has this class.
    #[must_use]
    pub fn is(&self, kind: InputKind) -> bool {
        self.kinds.contains(&kind)
    }
}

/// Classify one node. `None` means "not user input hardware" (buttons, vibrator, test injector,
/// sensors, unrelated virtual devices).
#[must_use]
pub fn classify(p: &InputProps) -> Option<InputDevice> {
    if IGNORED_NAMES.contains(&p.name.as_str()) {
        return None;
    }
    let bridged = BRIDGED_NAMES.contains(&p.name.as_str());
    if p.is_virtual() && !bridged {
        return None;
    }
    let mut kinds = Vec::new();
    if p.flag("ID_INPUT_KEYBOARD") {
        kinds.push(InputKind::Keyboard);
    }
    if p.flag("ID_INPUT_MOUSE") {
        kinds.push(InputKind::Mouse);
    }
    if p.flag("ID_INPUT_TOUCHPAD") {
        kinds.push(InputKind::Touchpad);
    }
    let touchscreen = p.flag("ID_INPUT_TOUCHSCREEN");
    if touchscreen {
        kinds.push(InputKind::Touchscreen);
    }
    // udev can flag some touch panels as joysticks; a touchscreen is never a game controller.
    if p.flag("ID_INPUT_JOYSTICK") && !touchscreen {
        kinds.push(InputKind::Gamepad);
    }
    if p.flag("ID_INPUT_TABLET") {
        kinds.push(InputKind::Tablet);
    }
    if kinds.is_empty() {
        return None;
    }
    kinds.sort();
    kinds.dedup();
    Some(InputDevice {
        id: p.devpath.clone(),
        name: p.name.clone(),
        kinds,
        bus: p.props.get("ID_BUS").cloned(),
    })
}

/// What changed in the registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputChange {
    /// A relevant device appeared.
    Added(InputDevice),
    /// A relevant device went away.
    Removed(InputDevice),
}

/// Counts and flags other processes care about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InputSummary {
    /// Number of physical keyboards.
    pub keyboards: u32,
    /// Number of mice and touchpads.
    pub pointers: u32,
    /// Number of game controllers.
    pub gamepads: u32,
}

/// The set of connected input devices.
#[derive(Debug, Default)]
pub struct InputRegistry {
    devices: BTreeMap<String, InputDevice>,
}

impl InputRegistry {
    /// An empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A node appeared (or was re-announced). Returns a change only when the set really changed.
    pub fn add(&mut self, p: &InputProps) -> Option<InputChange> {
        let dev = classify(p)?;
        if self.devices.get(&dev.id) == Some(&dev) {
            return None;
        }
        self.devices.insert(dev.id.clone(), dev.clone());
        Some(InputChange::Added(dev))
    }

    /// A node disappeared, identified by its sysfs path.
    pub fn remove(&mut self, devpath: &str) -> Option<InputChange> {
        self.devices.remove(devpath).map(InputChange::Removed)
    }

    /// All connected devices, in id order.
    pub fn devices(&self) -> impl Iterator<Item = &InputDevice> {
        self.devices.values()
    }

    /// Connected game controllers.
    pub fn gamepads(&self) -> impl Iterator<Item = &InputDevice> {
        self.devices().filter(|d| d.is(InputKind::Gamepad))
    }

    /// Whether a real keyboard is attached right now.
    #[must_use]
    pub fn physical_keyboard_present(&self) -> bool {
        self.devices().any(|d| d.is(InputKind::Keyboard))
    }

    /// Whether a mouse or touchpad is attached.
    #[must_use]
    pub fn pointer_present(&self) -> bool {
        self.devices()
            .any(|d| d.is(InputKind::Mouse) || d.is(InputKind::Touchpad))
    }

    /// Counts for publishing.
    #[must_use]
    pub fn summary(&self) -> InputSummary {
        let count = |f: &dyn Fn(&InputDevice) -> bool| -> u32 {
            u32::try_from(self.devices().filter(|d| f(d)).count()).unwrap_or(u32::MAX)
        };
        InputSummary {
            keyboards: count(&|d| d.is(InputKind::Keyboard)),
            pointers: count(&|d| d.is(InputKind::Mouse) || d.is(InputKind::Touchpad)),
            gamepads: count(&|d| d.is(InputKind::Gamepad)),
        }
    }
}

/// The text published for other processes: one `key=value` per line, easy to read from shell.
#[must_use]
pub fn format_state(s: InputSummary) -> String {
    format!(
        "physical_keyboard={}\npointer={}\ngamepads={}\n",
        u8::from(s.keyboards > 0),
        u8::from(s.pointers > 0),
        s.gamepads
    )
}

/// Write [`format_state`] atomically (temp file in the same directory, then rename), so readers never see
/// a partial file.
pub fn write_state_atomic(path: &Path, s: InputSummary) -> io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, format_state(s))?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- real A21s fixtures (read with udevadm on 2026-11-18, no external device attached) ----
    fn touchscreen() -> InputProps {
        InputProps::new(
            "/devices/platform/soc@0/11d000c0.usi/11d00000.spi/spi_master/spi0/spi0.0/input/input0/event0",
            "Omnivision TD4150 Touchscreen",
            &[("ID_INPUT", "1"), ("ID_INPUT_TOUCHSCREEN", "1"), ("ID_BUS", "spi")],
        )
    }
    fn power_key() -> InputProps {
        InputProps::new(
            "/devices/platform/firmware:power-management/firmware:power-management:pmic-1/s2mpu12-power-keys/input/input1/event1",
            "s2mpu12-pwrkey",
            &[("ID_INPUT", "1"), ("ID_INPUT_KEY", "1"), ("ID_BUS", "platform")],
        )
    }
    fn gpio_keys() -> InputProps {
        InputProps::new(
            "/devices/platform/gpio-keys/input/input2/event2",
            "gpio-keys",
            &[
                ("ID_INPUT", "1"),
                ("ID_INPUT_KEY", "1"),
                ("ID_BUS", "platform"),
            ],
        )
    }
    fn vibrator() -> InputProps {
        InputProps::new(
            "/devices/virtual/input/input3/event3",
            "a21s-vibrator",
            &[("ID_INPUT", "1")],
        )
    }
    fn headset() -> InputProps {
        InputProps::new(
            "/devices/platform/aud3004x/input/input4/event4",
            "AUD3004X Headset Input",
            &[
                ("ID_INPUT", "1"),
                ("ID_INPUT_KEY", "1"),
                ("ID_INPUT_SWITCH", "1"),
                ("ID_BUS", "platform"),
            ],
        )
    }

    // ---- typical udev output, NOT captured from hardware ----
    fn usb_keyboard() -> InputProps {
        InputProps::new(
            "/devices/platform/soc@0/13200000.usb/usb1/1-1/1-1:1.0/0003:046D:C31C.0001/input/input7/event7",
            "Logitech USB Keyboard",
            &[("ID_INPUT", "1"), ("ID_INPUT_KEY", "1"), ("ID_INPUT_KEYBOARD", "1"), ("ID_BUS", "usb")],
        )
    }
    fn usb_mouse() -> InputProps {
        InputProps::new(
            "/devices/platform/soc@0/13200000.usb/usb1/1-1/1-1:1.1/0003:046D:C077.0002/input/input8/event8",
            "Logitech USB Optical Mouse",
            &[("ID_INPUT", "1"), ("ID_INPUT_MOUSE", "1"), ("ID_BUS", "usb")],
        )
    }
    fn bt_keyboard_uhid() -> InputProps {
        InputProps::new(
            "/devices/virtual/misc/uhid/0005:05AC:0255.0003/input/input9/event9",
            "Bluetooth Keyboard",
            &[
                ("ID_INPUT", "1"),
                ("ID_INPUT_KEY", "1"),
                ("ID_INPUT_KEYBOARD", "1"),
                ("ID_BUS", "bluetooth"),
            ],
        )
    }
    fn bridged_gamepad() -> InputProps {
        InputProps::new(
            "/devices/virtual/input/input11/event11",
            "Xbox Wireless Controller",
            &[("ID_INPUT", "1"), ("ID_INPUT_JOYSTICK", "1")],
        )
    }
    fn steam_virtual_pad() -> InputProps {
        InputProps::new(
            "/devices/virtual/input/input12/event12",
            "Microsoft X-Box 360 pad 0",
            &[("ID_INPUT", "1"), ("ID_INPUT_JOYSTICK", "1")],
        )
    }
    fn uinput_fake_keyboard() -> InputProps {
        InputProps::new(
            "/devices/virtual/input/input13/event13",
            "some injector",
            &[
                ("ID_INPUT", "1"),
                ("ID_INPUT_KEY", "1"),
                ("ID_INPUT_KEYBOARD", "1"),
            ],
        )
    }

    #[test]
    fn the_a21s_without_peripherals_has_no_keyboard_pointer_or_gamepad() {
        let mut r = InputRegistry::new();
        for p in [
            touchscreen(),
            power_key(),
            gpio_keys(),
            vibrator(),
            headset(),
        ] {
            r.add(&p);
        }
        assert!(
            !r.physical_keyboard_present(),
            "volume/power buttons are not a keyboard"
        );
        assert!(!r.pointer_present());
        assert_eq!(r.summary(), InputSummary::default());
        let names: Vec<&str> = r.devices().map(|d| d.name.as_str()).collect();
        assert_eq!(
            names,
            ["Omnivision TD4150 Touchscreen"],
            "only the touchscreen is user input"
        );
    }

    #[test]
    fn usb_keyboard_and_mouse_are_detected_and_removed() {
        let mut r = InputRegistry::new();
        assert!(matches!(
            r.add(&usb_keyboard()),
            Some(InputChange::Added(_))
        ));
        assert!(r.physical_keyboard_present());
        assert!(!r.pointer_present());
        assert!(matches!(r.add(&usb_mouse()), Some(InputChange::Added(_))));
        assert!(r.pointer_present());
        assert_eq!(
            r.summary(),
            InputSummary {
                keyboards: 1,
                pointers: 1,
                gamepads: 0
            }
        );

        let kb_path = usb_keyboard().devpath;
        assert!(
            matches!(r.remove(&kb_path), Some(InputChange::Removed(d)) if d.name == "Logitech USB Keyboard")
        );
        assert!(!r.physical_keyboard_present());
        assert_eq!(r.remove(&kb_path), None, "removing twice is a no-op");
    }

    #[test]
    fn a_repeated_announcement_is_not_a_change() {
        let mut r = InputRegistry::new();
        assert!(r.add(&usb_keyboard()).is_some());
        assert_eq!(r.add(&usb_keyboard()), None);
    }

    #[test]
    fn bluetooth_hid_over_uhid_counts_as_physical_hardware() {
        let d = classify(&bt_keyboard_uhid()).expect("a bluetooth keyboard is real hardware");
        assert!(d.is(InputKind::Keyboard));
        assert_eq!(d.bus.as_deref(), Some("bluetooth"));
    }

    #[test]
    fn virtual_keyboards_do_not_count_as_a_physical_keyboard() {
        assert_eq!(classify(&uinput_fake_keyboard()), None);
    }

    #[test]
    fn our_ble_controller_bridge_is_a_gamepad_but_other_virtual_pads_are_not() {
        let d = classify(&bridged_gamepad())
            .expect("the bridge is the only path to the controller today");
        assert!(d.is(InputKind::Gamepad));
        assert_eq!(
            classify(&steam_virtual_pad()),
            None,
            "virtual duplicates must not double-count"
        );
    }

    #[test]
    fn a_touchscreen_flagged_as_joystick_is_not_a_gamepad() {
        let p = InputProps::new(
            "/devices/platform/x/input/input5/event5",
            "weird panel",
            &[("ID_INPUT_TOUCHSCREEN", "1"), ("ID_INPUT_JOYSTICK", "1")],
        );
        let d = classify(&p).unwrap();
        assert!(d.is(InputKind::Touchscreen) && !d.is(InputKind::Gamepad));
    }

    #[test]
    fn a_combo_device_has_every_kind_sorted_once() {
        let p = InputProps::new(
            "/devices/platform/x/usb/input/input6/event6",
            "Keyboard with trackpad",
            &[
                ("ID_INPUT_KEYBOARD", "1"),
                ("ID_INPUT_TOUCHPAD", "1"),
                ("ID_BUS", "usb"),
            ],
        );
        let d = classify(&p).unwrap();
        assert_eq!(d.kinds, [InputKind::Keyboard, InputKind::Touchpad]);
    }

    #[test]
    fn the_touch_test_injector_is_ignored() {
        let p = InputProps::new(
            "/devices/virtual/input/input20/event20",
            "a21s-test-touch",
            &[("ID_INPUT_TOUCHSCREEN", "1")],
        );
        assert_eq!(classify(&p), None);
    }

    #[test]
    fn state_text_is_stable() {
        let s = InputSummary {
            keyboards: 2,
            pointers: 0,
            gamepads: 1,
        };
        assert_eq!(
            format_state(s),
            "physical_keyboard=1\npointer=0\ngamepads=1\n"
        );
        assert_eq!(
            format_state(InputSummary::default()),
            "physical_keyboard=0\npointer=0\ngamepads=0\n"
        );
    }

    #[test]
    fn state_file_is_written_atomically_and_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rabbiz-input-state");
        write_state_atomic(
            &path,
            InputSummary {
                keyboards: 1,
                pointers: 1,
                gamepads: 0,
            },
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "physical_keyboard=1\npointer=1\ngamepads=0\n"
        );
        write_state_atomic(&path, InputSummary::default()).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "physical_keyboard=0\npointer=0\ngamepads=0\n"
        );
        assert!(
            !dir.path().join("rabbiz-input-state.tmp").exists(),
            "no temp file is left behind"
        );
    }
}
