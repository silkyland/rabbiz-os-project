//! P18 error skeleton: "app not responding" sheet with Wait/Close buttons.

use de_api::{Rect, Size};
use de_design::tokens::{self, Theme};
use de_paint::Canvas;

use crate::chrome::{self, style};
use crate::i18n::Strings;

pub fn render(size: Size, s: &Strings, t: &Theme, app: &str) -> de_api::Pixels {
    let mut c = Canvas::filled(size, t.bg);
    c.fill_rect(Rect::new(0, 0, size.w, size.h), t.scrim);
    let sheet = Rect::new(chrome::MARGIN * 2, (size.h - 320) / 2, size.w - chrome::MARGIN * 4, 320);
    c.fill_rounded_rect(sheet, tokens::RADIUS_SHEET, t.surface1);
    chrome::draw(&mut c, s.get("error.app_not_responding"), &style::h2(t.text), sheet.x + 28, sheet.y + 32, None);
    chrome::draw(&mut c, app, &style::body(t.text2), sheet.x + 28, sheet.y + 100, None);
    // Wait / Close buttons.
    let bw = (sheet.w - 56 - 16) / 2;
    c.fill_rounded_rect(Rect::new(sheet.x + 28, sheet.y + sheet.h - 96, bw, 64), tokens::RADIUS_PILL, t.surface3);
    c.fill_rounded_rect(Rect::new(sheet.x + 28 + bw + 16, sheet.y + sheet.h - 96, bw, 64), tokens::RADIUS_PILL, t.danger);
    chrome::draw_centered_x(&mut c, s.get("error.wait"), &style::label(t.text), sheet.x + 28 + bw / 2, sheet.y + sheet.h - 76);
    chrome::draw_centered_x(&mut c, s.get("error.close_app"), &style::label(t.text), sheet.x + 28 + bw + 16 + bw / 2, sheet.y + sheet.h - 76);
    c.into_pixels()
}
