//! Keyboard event dispatch: power key, lock/sleep wake, shell shortcuts,
//! and Search/Drawer text entry.

use std::process::Command;

use smithay::backend::input::{Event, InputBackend, KeyState, KeyboardKeyEvent};
use smithay::input::keyboard::{FilterResult, Keysym, ModifiersState};
use smithay::utils::SERIAL_COUNTER as SCOUNTER;

use crate::state::De;

impl De {
    pub(super) fn on_keyboard_key<B: InputBackend>(&mut self, evt: &B::KeyboardKeyEvent) {
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
}
