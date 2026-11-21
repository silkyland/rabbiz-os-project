//! Shell-level keybindings for hardware keyboards (mouse+keyboard mandate).
//!
//! The map, chosen to match mobile-Linux DE conventions (phosh/sxmo/lomiri)
//! without fighting the touch gesture model — every binding is a key event,
//! never a gesture, so there is no overlap:
//!
//! | Keys                     | Action                                        |
//! |--------------------------|-----------------------------------------------|
//! | `Super` (tap alone)      | Toggle the app drawer (launcher)              |
//! | `Alt+Tab`                | Switch to the previous app (QuickSwitch twin) |
//! | `Escape`, `XF86Back`     | Back: app gets Escape/Back key, shell pops    |
//! | `Print` (SysRq)          | Screenshot (same path as the power menu)     |
//! | `Ctrl+Alt+L`             | Lock the screen                               |
//! | `XF86Sleep`              | Power-key sleep (lock + fade → asleep)        |
//! | `XF86MonBrightnessUp/Down` | Brightness ±10 % (quick-settings level)     |
//! | `XF86AudioRaise/LowerVolume` | Volume ±10 %                              |
//! | `XF86AudioPlay/Pause`    | `playerctl play-pause` (MPRIS, best effort)   |
//! | `XF86AudioNext/Prev/Stop`| `playerctl next/previous/stop`                |
//!
//! Deliberately *not* bound: `Ctrl+Alt+T`-style terminal spawn (no terminal
//! contract yet), `Super+key` combos beyond the bare-Super launcher, and
//! `XF86AudioMute` (the controls layer has no mute action — the key falls
//! through to the focused client instead). Media keys that reach a focused
//! client unmodified keep working too: only the ones we act on are
//! intercepted.
//!
//! `Super` alone is detected by press/release pairing in
//! [`De::keyboard_shortcut`]: any other key press cancels the pending
//! launcher toggle, so `Super` used as a real modifier never fires it.

use std::process::Command;

use smithay::backend::input::KeyState;
use smithay::input::keyboard::{Keysym, ModifiersState};

use de_ui::nav::Screen;
use de_ui::power::PowerAction;

use crate::state::{De, Foreground};

/// A matched chord (independent of press/release and focus state).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Shortcut {
    /// `Alt+Tab` — toggle to the previous app window.
    CycleWindow,
    /// `Escape` / `XF86Back` — the Android "back" affordance.
    Back,
    /// `Print` — screenshot via the power-menu path.
    Screenshot,
    /// `Ctrl+Alt+L`.
    Lock,
    /// `XF86Sleep` — same as a single power-key press.
    Sleep,
    BrightnessUp,
    BrightnessDown,
    VolumeUp,
    VolumeDown,
    MediaPlayPause,
    MediaStop,
    MediaNext,
    MediaPrev,
}

/// Match one keysym against the chord table. `sym` must be the key's
/// *raw* (level-0) keysym so `Ctrl+Alt+L` still matches when modifiers
/// would otherwise alter the produced character.
#[must_use]
pub(crate) fn match_shortcut(mods: &ModifiersState, sym: Keysym) -> Option<Shortcut> {
    use Shortcut::*;
    // `Keysym` is a newtype over u32 — compare, don't pattern-match.
    let no_mods = !mods.ctrl && !mods.alt && !mods.logo && !mods.shift;
    if sym == Keysym::Tab && mods.alt && !mods.ctrl && !mods.logo {
        return Some(CycleWindow);
    }
    if (sym == Keysym::Escape || sym == Keysym::XF86_Back) && no_mods {
        return Some(Back);
    }
    if sym == Keysym::Print && no_mods {
        return Some(Screenshot);
    }
    if (sym == Keysym::l || sym == Keysym::L) && mods.ctrl && mods.alt {
        return Some(Lock);
    }
    if sym == Keysym::XF86_Sleep && no_mods {
        return Some(Sleep);
    }
    if sym == Keysym::XF86_MonBrightnessUp && no_mods {
        return Some(BrightnessUp);
    }
    if sym == Keysym::XF86_MonBrightnessDown && no_mods {
        return Some(BrightnessDown);
    }
    if sym == Keysym::XF86_AudioRaiseVolume && no_mods {
        return Some(VolumeUp);
    }
    if sym == Keysym::XF86_AudioLowerVolume && no_mods {
        return Some(VolumeDown);
    }
    if (sym == Keysym::XF86_AudioPlay || sym == Keysym::XF86_AudioPause) && no_mods {
        return Some(MediaPlayPause);
    }
    if sym == Keysym::XF86_AudioStop && no_mods {
        return Some(MediaStop);
    }
    if sym == Keysym::XF86_AudioNext && no_mods {
        return Some(MediaNext);
    }
    if sym == Keysym::XF86_AudioPrev && no_mods {
        return Some(MediaPrev);
    }
    None
}

impl De {
    /// One keyboard event for the shell binding layer. Called from the
    /// `keyboard.input` filter *after* the sleep/lock gates and *before*
    /// the Search/Drawer typing block, so chords work while those screens
    /// own the keyboard. Returns `true` when the event was consumed.
    ///
    /// `sym` is the raw keysym (level 0) of the event's keycode.
    pub(crate) fn keyboard_shortcut(
        &mut self,
        mods: &ModifiersState,
        sym: Keysym,
        state: KeyState,
        time_ms: u64,
    ) -> bool {
        // The launcher key: press arms, any other key disarms, release
        // with the arm still set toggles the drawer. Super itself is never
        // forwarded to clients.
        if matches!(sym, Keysym::Super_L | Keysym::Super_R) {
            match state {
                KeyState::Pressed => self.launcher_pending = true,
                KeyState::Released => {
                    if std::mem::take(&mut self.launcher_pending) {
                        self.toggle_drawer();
                    }
                }
            }
            return true;
        }
        if state == KeyState::Pressed {
            self.launcher_pending = false;
        }
        if state != KeyState::Pressed {
            return false;
        }
        match match_shortcut(mods, sym) {
            Some(sc) => {
                self.run_shortcut(sc, time_ms);
                true
            }
            None => false,
        }
    }

    fn run_shortcut(&mut self, sc: Shortcut, time_ms: u64) {
        match sc {
            Shortcut::CycleWindow => self.quick_switch(true),
            Shortcut::Back => self.keyboard_back(time_ms),
            Shortcut::Screenshot => self.power_action(PowerAction::Screenshot),
            Shortcut::Lock => self.power_action(PowerAction::Lock),
            Shortcut::Sleep => self.power_sleep(),
            Shortcut::BrightnessUp => self.adjust_level(Level::Brightness, 0.1),
            Shortcut::BrightnessDown => self.adjust_level(Level::Brightness, -0.1),
            Shortcut::VolumeUp => self.adjust_level(Level::Volume, 0.1),
            Shortcut::VolumeDown => self.adjust_level(Level::Volume, -0.1),
            Shortcut::MediaPlayPause => media_key("play-pause"),
            Shortcut::MediaStop => media_key("stop"),
            Shortcut::MediaNext => media_key("next"),
            Shortcut::MediaPrev => media_key("previous"),
        }
    }

    /// `Super` tap: the app drawer is the launcher. Toggling closed returns
    /// Home, matching the gesture path (`open_drawer_animated` /
    /// `show_shell`).
    fn toggle_drawer(&mut self) {
        if self.foreground == Foreground::Shell && self.nav.current == Screen::Drawer {
            self.show_shell(Screen::Home);
        } else {
            self.open_drawer_animated(None, 0.0);
        }
    }

    /// `Alt+Tab` / QuickSwitch shared by the nav-bar scrub and the key
    /// chord: switch to the previous app window (or the most recent one
    /// when the shell is foreground).
    pub(crate) fn quick_switch(&mut self, to_previous: bool) {
        if self.panels.shown() {
            return;
        }
        let windows = self.recent_windows();
        if windows.is_empty() {
            return;
        }
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

    /// `Escape`/`XF86Back`: the keyboard twin of the edge-back gesture.
    /// In an app the app gets first say via [`De::app_back`] (popup
    /// ungrab → Escape for dialogs → `XF86Back` otherwise); on shell
    /// screens it pops the nav stack, closes Recents/panels first.
    pub(crate) fn keyboard_back(&mut self, time_ms: u64) {
        if self.foreground == Foreground::App {
            self.app_back(time_ms);
            return;
        }
        if self.panels.shown() {
            self.panel_close();
            return;
        }
        if self.menu_is_open() {
            self.close_menu();
            return;
        }
        match self.nav.current {
            Screen::Recents => self.show_shell(Screen::Home),
            Screen::Settings if self.navigation.settings_keyboard => {
                self.navigation.settings_keyboard = false;
                self.shell_dirty = true;
                self.needs_present = true;
            }
            _ => {
                self.launch.cancel();
                self.morph = None;
                self.nav.back();
                self.shell_dirty = true;
                self.needs_present = true;
            }
        }
    }

    /// Brightness/volume keys drive the same `Controls` channel the
    /// quick-settings sliders use (`request_control` marks the panel
    /// rasters stale; `poll_panels` snaps the knob to the real level),
    /// update the local level optimistically so rapid presses step cleanly,
    /// and pop the GPU-composited OSD pill immediately.
    fn adjust_level(&mut self, level: Level, delta: f32) {
        use de_config::ipc::OsdKind;
        use de_system::controls::Action;
        let cur = match level {
            Level::Brightness => self
                .panels
                .control_state
                .brightness
                .unwrap_or(self.panels.brightness),
            Level::Volume => self
                .panels
                .control_state
                .volume
                .unwrap_or(self.panels.volume),
        };
        let next = (cur + delta).clamp(0.0, 1.0);
        let (kind, action) = match level {
            Level::Brightness => {
                self.panels.brightness = next;
                self.panels.control_state.brightness = Some(next);
                (OsdKind::Brightness, Action::Brightness(next))
            }
            Level::Volume => {
                self.panels.volume = next;
                self.panels.control_state.volume = Some(next);
                (OsdKind::Volume, Action::Volume(next))
            }
        };
        self.panels.request_control(action);
        let pct = (next * 100.0).round() as u8;
        self.osd_fx.show(
            kind,
            pct,
            kind == OsdKind::Volume && pct == 0,
            std::time::Instant::now(),
        );
        self.needs_present = true;
    }
}

#[derive(Clone, Copy)]
enum Level {
    Brightness,
    Volume,
}

/// MPRIS media keys go to whatever player is running. `playerctl` is the
/// standard frontend; when it is not installed the spawn fails silently —
/// the key is still consumed so apps never see a half-bound chord.
fn media_key(action: &'static str) {
    let _ = Command::new("playerctl").arg(action).spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mods(alt: bool, ctrl: bool, logo: bool) -> ModifiersState {
        ModifiersState {
            alt,
            ctrl,
            logo,
            ..ModifiersState::default()
        }
    }

    #[test]
    fn alt_tab_needs_alt_and_no_other_modifiers() {
        assert_eq!(
            match_shortcut(&mods(true, false, false), Keysym::Tab),
            Some(Shortcut::CycleWindow)
        );
        assert_eq!(match_shortcut(&mods(true, true, false), Keysym::Tab), None);
        assert_eq!(
            match_shortcut(&mods(false, false, false), Keysym::Tab),
            None
        );
        assert_eq!(match_shortcut(&mods(true, false, true), Keysym::Tab), None);
    }

    #[test]
    fn escape_and_print_are_unmodified() {
        assert_eq!(
            match_shortcut(&ModifiersState::default(), Keysym::Escape),
            Some(Shortcut::Back)
        );
        assert_eq!(
            match_shortcut(&ModifiersState::default(), Keysym::Print),
            Some(Shortcut::Screenshot)
        );
        // Extra modifiers fall through to the client.
        assert_eq!(
            match_shortcut(&mods(false, true, false), Keysym::Escape),
            None
        );
        assert_eq!(
            match_shortcut(&mods(true, false, false), Keysym::Print),
            None
        );
    }

    #[test]
    fn ctrl_alt_l_locks() {
        assert_eq!(
            match_shortcut(&mods(true, true, false), Keysym::l),
            Some(Shortcut::Lock)
        );
        assert_eq!(
            match_shortcut(&mods(true, true, false), Keysym::L),
            Some(Shortcut::Lock)
        );
        assert_eq!(match_shortcut(&mods(false, true, false), Keysym::l), None);
        assert_eq!(match_shortcut(&mods(true, false, false), Keysym::l), None);
    }

    #[test]
    fn media_and_levels_match() {
        let m = ModifiersState::default();
        assert_eq!(
            match_shortcut(&m, Keysym::XF86_AudioPlay),
            Some(Shortcut::MediaPlayPause)
        );
        assert_eq!(
            match_shortcut(&m, Keysym::XF86_AudioPause),
            Some(Shortcut::MediaPlayPause)
        );
        assert_eq!(
            match_shortcut(&m, Keysym::XF86_AudioNext),
            Some(Shortcut::MediaNext)
        );
        assert_eq!(
            match_shortcut(&m, Keysym::XF86_AudioPrev),
            Some(Shortcut::MediaPrev)
        );
        assert_eq!(
            match_shortcut(&m, Keysym::XF86_MonBrightnessUp),
            Some(Shortcut::BrightnessUp)
        );
        assert_eq!(match_shortcut(&m, Keysym::XF86_AudioMute), None);
        assert_eq!(match_shortcut(&m, Keysym::XF86_AudioMicMute), None);
    }
}
