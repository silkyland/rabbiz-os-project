//! P9 power menu skeleton: centered sheet with Power off / Restart / Lock /
//! Safe mode / Screenshot rows.

use de_api::{Rect, Size};
use de_design::tokens::{self, Theme};
use de_paint::Canvas;

use crate::chrome::{self, style};
use crate::i18n::Strings;

pub fn render(size: Size, s: &Strings, t: &Theme) -> de_api::Pixels {
    let mut c = Canvas::filled(size, t.bg);
    c.fill_rect(Rect::new(0, 0, size.w, size.h), t.scrim);
    let keys = ["power.power_off", "power.restart", "power.lock", "power.safe_mode", "power.screenshot"];
    let h = 80 + keys.len() as i32 * 88;
    let sheet = Rect::new(chrome::MARGIN * 2, (size.h - h) / 2, size.w - chrome::MARGIN * 4, h);
    c.fill_rounded_rect(sheet, tokens::RADIUS_SHEET, t.surface1);
    chrome::draw(&mut c, s.get("power.title"), &style::h2(t.text), sheet.x + 28, sheet.y + 20, None);
    let mut y = sheet.y + 80;
    for (i, k) in keys.iter().enumerate() {
        let col = if *k == "power.power_off" { t.danger } else { t.text };
        if i > 0 { c.fill_rect(Rect::new(sheet.x + 16, y, sheet.w - 32, 1), t.line); }
        chrome::draw(&mut c, s.get(k), &style::body(col), sheet.x + 28, y + 28, None);
        y += 88;
    }
    c.into_pixels()
}
