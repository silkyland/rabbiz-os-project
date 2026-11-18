//! P1 lock screen skeleton (§7.18): big tnum clock, date, notifications in
//! the lower half, unlock hint above the nav pill.

use de_api::Size;
use de_design::tokens::{self, Theme};
use de_paint::Canvas;

use crate::chrome::{self, style};
use crate::i18n::Strings;

/// Render lock screen. `clock` = "HH:MM", `date` = localized date line.
pub fn render(size: Size, s: &Strings, t: &Theme, clock: &str, date: &str) -> de_api::Pixels {
    let mut c = Canvas::filled(size, t.bg);
    chrome::status_bar(&mut c, t, &chrome::StatusInfo::new(clock, ""));
    // Clock block below cutout.
    chrome::draw_centered(&mut c, clock, &style::display(t.text), 140);
    chrome::draw_centered(&mut c, date, &style::body(t.text2), 260);
    // Notification cards in lower half.
    let mut y = size.h / 2 + 60;
    for _ in 0..2 {
        c.fill_rounded_rect(de_api::Rect::new(chrome::MARGIN, y, chrome::content_w(size), 96), tokens::RADIUS_CARD, t.glass);
        y += 96 + 12;
    }
    chrome::draw_centered(&mut c, s.get("lock.unlock_hint"), &style::label(t.text3), size.h - 96);
    chrome::nav_pill(&mut c, t);
    c.into_pixels()
}
