//! P7 quick-settings sub-sheet skeleton (Wi-Fi/BT inline expansion per P6).

use de_api::{Rect, Size};
use de_design::tokens::{self, Theme};
use de_paint::Canvas;

use crate::chrome::{self, style};
use crate::i18n::Strings;

#[must_use]
pub fn render(
    size: Size,
    s: &Strings,
    t: &Theme,
    clock: &str,
    title_key: &str,
    rows: &[&str],
) -> de_api::Pixels {
    let mut c = Canvas::filled(size, t.bg);
    chrome::status_bar(&mut c, t, &chrome::StatusInfo::new(clock, ""));
    c.fill_rect(Rect::new(0, 0, size.w, size.h), t.scrim);
    let h = 120 + rows.len() as i32 * 88;
    let sheet = Rect::new(
        chrome::MARGIN,
        chrome::STATUS_H + 24,
        chrome::content_w(size),
        h.min(size.h - 300),
    );
    c.fill_rounded_rect(sheet, tokens::RADIUS_SHEET, t.surface1);
    chrome::draw(
        &mut c,
        s.get(title_key),
        &style::h2(t.text),
        sheet.x + 28,
        sheet.y + 28,
        None,
    );
    let mut y = sheet.y + 100;
    for (i, r) in rows.iter().enumerate() {
        if y + 88 > sheet.y + sheet.h {
            break;
        }
        let last = i + 1 == rows.len() || y + 88 * 2 > sheet.y + sheet.h;
        chrome::settings_row(&mut c, t, y, r, "", i == 0, last);
        y += 88;
    }
    chrome::nav_pill(&mut c, t);
    c.into_pixels()
}
