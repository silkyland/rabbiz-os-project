//! P15 settings index: grouped inset cards, 88 px rows, section headers.

use de_api::{Rect, Size};
use de_design::tokens::{self, Theme};
use de_paint::Canvas;

use crate::chrome::{self, style};
use crate::i18n::Strings;

/// Sections in spec order.
const SECTIONS: &[&str] = &[
    "settings.network",
    "settings.bluetooth",
    "settings.display",
    "settings.sound",
    "settings.notifications",
    "settings.gestures",
    "settings.keyboard",
    "settings.apps",
    "settings.battery",
    "settings.storage",
    "settings.about",
    "settings.session",
];

/// Hit-test the settings index rows (returns index into `SECTIONS`).
#[must_use]
pub fn hit_test(size: Size, pos: de_api::Point) -> Option<usize> {
    let mut y = chrome::STATUS_H + 104;
    for i in 0..SECTIONS.len() {
        let r = Rect::new(chrome::MARGIN, y, chrome::content_w(size), 88);
        if r.contains(pos) {
            return Some(i);
        }
        y += 88;
        if i == 3 || i == 7 {
            y += 20;
        }
    }
    None
}

/// Section key at `index`.
#[must_use]
pub fn section_key(index: usize) -> Option<&'static str> {
    SECTIONS.get(index).copied()
}

#[must_use]
pub fn render_index(size: Size, s: &Strings, t: &Theme, clock: &str) -> de_api::Pixels {
    render_index_with_keyboard(size, s, t, clock, "")
}

#[must_use]
pub fn render_index_with_keyboard(
    size: Size,
    s: &Strings,
    t: &Theme,
    clock: &str,
    keyboard_name: &str,
) -> de_api::Pixels {
    let mut c = Canvas::filled(size, t.bg);
    chrome::status_bar(&mut c, t, &chrome::StatusInfo::new(clock, ""));
    chrome::draw(
        &mut c,
        s.get("settings.title"),
        &style::h2(t.text),
        chrome::MARGIN,
        chrome::STATUS_H + 24,
        None,
    );
    let mut y = chrome::STATUS_H + 96;
    for (i, key) in SECTIONS.iter().enumerate() {
        if y + 88 > size.h - chrome::NAV_H - 16 {
            break;
        }
        let last = i + 1 == SECTIONS.len() || y + 88 * 2 > size.h - chrome::NAV_H - 16;
        let val = if *key == "settings.keyboard" {
            keyboard_name
        } else {
            ""
        };
        chrome::settings_row(&mut c, t, y, s.get(key), val, i == 0, last);
        y += 88;
        if i == 3 || i == 7 {
            y += 20;
        }
    }
    let _ = Rect::new(0, 0, 0, 0);
    let _ = tokens::RADIUS_CARD;
    chrome::nav_pill(&mut c, t);
    c.into_pixels()
}
