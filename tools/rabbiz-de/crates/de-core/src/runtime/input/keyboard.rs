//! Keyboard event dispatch: power key, lock/sleep wake, shell shortcuts,
//! and Search/Drawer text entry.

use std::process::Command;

use de_config::ipc::{HwButtonKind, OsdKind};
use smithay::backend::input::{Event, InputBackend, KeyState, KeyboardKeyEvent, Keycode};
use smithay::input::keyboard::{FilterResult, Keysym, ModifiersState};
use smithay::utils::SERIAL_COUNTER as SCOUNTER;

use crate::state::De;

impl De {
    pub(super) fn on_keyboard_key<B: InputBackend>(&mut self, evt: &B::KeyboardKeyEvent) {
        self.inject_key(
            evt.key_code().raw(),
            evt.state() == KeyState::Pressed,
            evt.time_msec(),
        );
    }

    /// Inject a raw XKB keycode (shared by libinput and virtual/IPC key injection).
    pub fn inject_key(&mut self, keycode_raw: u32, pressed: bool, time: u32) {
        self.power_note_activity();
        let serial = SCOUNTER.next_serial();
        let keycode = Keycode::from(keycode_raw);
        let state = if pressed {
            KeyState::Pressed
        } else {
            KeyState::Released
        };
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
                    if de.compositor.is_none() {
                        de.inject_hw_button(HwButtonKind::Power);
                    } else {
                        let _ = Command::new("setsid")
                            .args(["a21s-power-key", "toggle"])
                            .spawn();
                    }
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
                    // Volume buttons stay usable on the lock screen (with OSD).
                    if let Some(sym) = raw {
                        if matches!(
                            sym,
                            Keysym::XF86_AudioRaiseVolume | Keysym::XF86_AudioLowerVolume
                        ) {
                            let _ = de.keyboard_shortcut(mods, sym, state, u64::from(time));
                        }
                    }
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
                            // Enter on global Search launches the default search engine.
                            Some(Keysym::Return) | Some(Keysym::KP_Enter)
                                if de.nav.current == de_ui::nav::Screen::Search =>
                            {
                                let query = de.navigation.search_query.clone();
                                if !query.trim().is_empty() {
                                    de.open_web_search(&query);
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

    /// Inject a hardware button press (Power / VolumeUp / VolumeDown).
    pub fn inject_hw_button(&mut self, button: HwButtonKind) {
        match button {
            HwButtonKind::Power => {
                if self.sleeping() {
                    self.power_wake();
                } else {
                    self.power_lock();
                }
                self.needs_present = true;
            }
            HwButtonKind::VolumeUp | HwButtonKind::VolumeDown => {
                self.power_note_activity();
                let step = if button == HwButtonKind::VolumeUp {
                    0.05
                } else {
                    -0.05
                };
                self.panels.volume = (self.panels.volume + step).clamp(0.0, 1.0);
                let pct = (self.panels.volume * 100.0).round() as u8;
                let _ = de_system::mock::apply("volume", &pct.to_string());
                self.osd_fx
                    .show(OsdKind::Volume, pct, pct == 0, std::time::Instant::now());
                self.needs_present = true;
            }
        }
    }
}
