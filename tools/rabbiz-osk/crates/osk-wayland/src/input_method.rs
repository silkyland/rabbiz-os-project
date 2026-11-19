//! `zwp_input_method_v2` + `zwp_virtual_keyboard_v1` protocol dispatch and keymap/evdev helpers.

use osk_core::state::{Command, InputPurpose};

use smithay_client_toolkit::reexports::client::{
    protocol::wl_seat, Connection, Dispatch, Proxy, QueueHandle,
};
use wayland_protocols_misc::zwp_input_method_v2::client::{
    zwp_input_method_manager_v2::ZwpInputMethodManagerV2,
    zwp_input_method_v2::{Event as ImEvent, ZwpInputMethodV2},
};
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::{
    zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1,
    zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1,
};

use crate::app::App;

/// evdev key codes for Backspace and Enter.
const KEY_BACKSPACE: u32 = 14;
const KEY_ENTER: u32 = 28;

/// Minimal xkb keymap: the compositor needs one before it accepts key events.
const KEYMAP: &str = r#"xkb_keymap {
    xkb_keycodes  { include "evdev+aliases(qwerty)" };
    xkb_types     { include "complete" };
    xkb_compat    { include "complete" };
    xkb_symbols   { include "pc+us" };
};"#;

impl App {
    pub(crate) fn send_commands(&mut self, commands: Vec<Command>) {
        let use_shell_bridge = self.shell_active && !self.active;
        for command in commands {
            match command {
                Command::Commit(text) => {
                    if use_shell_bridge {
                        if let Some(bridge) = &mut self.shell_im {
                            let clean = text.replace('\n', " ");
                            bridge.send_line(&format!("C {clean}"));
                        }
                    } else if self.done_count > 0 {
                        self.im.commit_string(text);
                        self.im.commit(self.done_count);
                    } else {
                        self.send_ascii_text(&text);
                    }
                }
                Command::Backspace { bytes } => {
                    if use_shell_bridge {
                        if let Some(bridge) = &mut self.shell_im {
                            bridge.send_line(&format!("B {bytes}"));
                        }
                    } else if self.done_count > 0 && self.has_surrounding_text {
                        self.im.delete_surrounding_text(bytes as u32, 0);
                        self.im.commit(self.done_count);
                    } else {
                        self.send_key(KEY_BACKSPACE);
                    }
                }
                Command::Enter => {
                    if use_shell_bridge {
                        if let Some(bridge) = &mut self.shell_im {
                            bridge.send_line("E");
                        }
                    } else {
                        // A text commit with "\n" is ignored by single-line fields (Chrome's
                        // omnibox, search boxes). Send a real Return key press instead.
                        self.send_key(KEY_ENTER);
                    }
                }
                // Cursor moves and layout switches need no protocol request.
                Command::MoveCursor(_) | Command::Layer(_) | Command::Language(_) => {}
            }
        }
        let _ = self.conn.flush();
    }

    /// Press and release one evdev keycode through the virtual keyboard.
    fn send_key(&mut self, keycode: u32) {
        let time = self.start.elapsed().as_millis() as u32;
        self.virtual_keyboard.key(time, keycode, 1); // wl_keyboard.key_state.pressed
        self.virtual_keyboard.key(time, keycode, 0); // wl_keyboard.key_state.released
        let _ = self.conn.flush();
    }

    /// Best-effort fallback for apps without `zwp_text_input_v3`: map ASCII chars to evdev keys.
    fn send_ascii_text(&mut self, text: &str) {
        let time = self.start.elapsed().as_millis() as u32;
        for ch in text.chars() {
            let Some((code, shift)) = ascii_to_evdev(ch) else {
                continue;
            };
            if shift {
                self.virtual_keyboard.modifiers(1, 0, 0, 0);
            }
            self.virtual_keyboard.key(time, code, 1);
            self.virtual_keyboard.key(time, code, 0);
            if shift {
                self.virtual_keyboard.modifiers(0, 0, 0, 0);
            }
        }
        let _ = self.conn.flush();
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for App {
    fn event(
        _: &mut App,
        _: &wl_seat::WlSeat,
        _: <wl_seat::WlSeat as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<App>,
    ) {
    }
}

impl Dispatch<ZwpVirtualKeyboardManagerV1, ()> for App {
    fn event(
        _: &mut App,
        _: &ZwpVirtualKeyboardManagerV1,
        _: <ZwpVirtualKeyboardManagerV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<App>,
    ) {
    }
}

impl Dispatch<ZwpVirtualKeyboardV1, ()> for App {
    fn event(
        _: &mut App,
        _: &ZwpVirtualKeyboardV1,
        _: <ZwpVirtualKeyboardV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<App>,
    ) {
    }
}

impl Dispatch<ZwpInputMethodManagerV2, ()> for App {
    fn event(
        _: &mut App,
        _: &ZwpInputMethodManagerV2,
        _: <ZwpInputMethodManagerV2 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<App>,
    ) {
    }
}

impl Dispatch<ZwpInputMethodV2, ()> for App {
    fn event(
        state: &mut App,
        _: &ZwpInputMethodV2,
        event: ImEvent,
        _: &(),
        _: &Connection,
        _: &QueueHandle<App>,
    ) {
        match event {
            ImEvent::Activate => {
                state.keyboard.reset_for_activation();
                state.has_surrounding_text = false;
                state.active = true;
                if state.user_visible {
                    state.show();
                }
            }
            ImEvent::Deactivate => {
                state.active = false;
                state.has_surrounding_text = false;
                if !state.shell_active {
                    state.hide();
                }
            }
            ImEvent::SurroundingText {
                text,
                cursor,
                anchor,
            } => {
                let _ = anchor;
                state.has_surrounding_text = true;
                state
                    .keyboard
                    .set_surrounding_text_at(text, cursor as usize);
            }
            ImEvent::ContentType { purpose, .. } => {
                // Gboard-style adaptation: email/url bottom rows, number pad for numeric fields.
                state
                    .keyboard
                    .set_purpose(InputPurpose::from_protocol(purpose.into()));
                state.sync_size();
                state.draw();
            }
            ImEvent::Done => {
                state.done_count += 1;
                if state.active && state.user_visible {
                    state.sync_size();
                    state.draw();
                }
            }
            ImEvent::Unavailable => {
                eprintln!("rabbiz-osk: another input method owns the seat (squeekboard?)");
                std::process::exit(2);
            }
            _ => {}
        }
    }
}

fn ascii_to_evdev(ch: char) -> Option<(u32, bool)> {
    let pair = match ch {
        'a' => (30, false),
        'b' => (48, false),
        'c' => (46, false),
        'd' => (32, false),
        'e' => (18, false),
        'f' => (33, false),
        'g' => (34, false),
        'h' => (35, false),
        'i' => (23, false),
        'j' => (36, false),
        'k' => (37, false),
        'l' => (38, false),
        'm' => (50, false),
        'n' => (49, false),
        'o' => (24, false),
        'p' => (25, false),
        'q' => (16, false),
        'r' => (19, false),
        's' => (31, false),
        't' => (20, false),
        'u' => (22, false),
        'v' => (47, false),
        'w' => (17, false),
        'x' => (45, false),
        'y' => (21, false),
        'z' => (44, false),
        'A' => (30, true),
        'B' => (48, true),
        'C' => (46, true),
        'D' => (32, true),
        'E' => (18, true),
        'F' => (33, true),
        'G' => (34, true),
        'H' => (35, true),
        'I' => (23, true),
        'J' => (36, true),
        'K' => (37, true),
        'L' => (38, true),
        'M' => (50, true),
        'N' => (49, true),
        'O' => (24, true),
        'P' => (25, true),
        'Q' => (16, true),
        'R' => (19, true),
        'S' => (31, true),
        'T' => (20, true),
        'U' => (22, true),
        'V' => (47, true),
        'W' => (17, true),
        'X' => (45, true),
        'Y' => (21, true),
        'Z' => (44, true),
        '1' => (2, false),
        '2' => (3, false),
        '3' => (4, false),
        '4' => (5, false),
        '5' => (6, false),
        '6' => (7, false),
        '7' => (8, false),
        '8' => (9, false),
        '9' => (10, false),
        '0' => (11, false),
        '!' => (2, true),
        '@' => (3, true),
        '#' => (4, true),
        '$' => (5, true),
        '%' => (6, true),
        '^' => (7, true),
        '&' => (8, true),
        '*' => (9, true),
        '(' => (10, true),
        ')' => (11, true),
        '-' => (12, false),
        '_' => (12, true),
        '=' => (13, false),
        '+' => (13, true),
        '[' => (26, false),
        '{' => (26, true),
        ']' => (27, false),
        '}' => (27, true),
        ';' => (39, false),
        ':' => (39, true),
        '\'' => (40, false),
        '"' => (40, true),
        '`' => (41, false),
        '~' => (41, true),
        '\\' => (43, false),
        '|' => (43, true),
        ',' => (51, false),
        '<' => (51, true),
        '.' => (52, false),
        '>' => (52, true),
        '/' => (53, false),
        '?' => (53, true),
        ' ' => (57, false),
        _ => return None,
    };
    Some(pair)
}

pub(crate) fn upload_keymap(keyboard: &ZwpVirtualKeyboardV1) {
    use std::io::Write;
    let Ok(mut file) = tempfile_memfd() else {
        eprintln!("rabbiz-osk: cannot create keymap fd");
        return;
    };
    // wl_keyboard.keymap convention: the text is NUL-terminated and `size` includes the NUL.
    // smithay drops the last byte as the terminator, so without it the final `;` is cut off and
    // the keymap fails to parse ("xkbcommon: ERROR ... unexpected end of file" under rabbiz-de).
    let mut bytes = KEYMAP.as_bytes().to_vec();
    bytes.push(0);
    if file.write_all(&bytes).is_err() {
        eprintln!("rabbiz-osk: cannot write keymap");
        return;
    }
    let fd = std::os::fd::AsFd::as_fd(&file);
    keyboard.keymap(
        1, /* wl_keyboard.keymap_format.xkb_v1 */
        fd,
        bytes.len() as u32,
    );
}

fn tempfile_memfd() -> std::io::Result<std::fs::File> {
    use std::os::fd::FromRawFd;
    let name = c"rabbiz-osk-keymap";
    let fd = unsafe { libc::memfd_create(name.as_ptr(), libc::MFD_CLOEXEC) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(unsafe { std::fs::File::from_raw_fd(fd) })
}
