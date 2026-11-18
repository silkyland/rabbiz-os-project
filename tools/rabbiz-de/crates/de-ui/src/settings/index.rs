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

#[must_use]
pub fn render_index(size: Size, s: &Strings, t: &Theme, clock: &str) -> de_api::Pixels {
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
        let _ = i;
        chrome::settings_row(&mut c, t, y, s.get(key), "", i == 0, false);
        y += 88;
    }
    let _ = Rect::new(0, 0, 0, 0);
    let _ = tokens::RADIUS_CARD;
    chrome::nav_pill(&mut c, t);
    c.into_pixels()
}
