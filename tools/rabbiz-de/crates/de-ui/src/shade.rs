//! P6 shade skeleton: notification center + quick settings on one sheet
//! (phone width stacks them; QS sub-sheets live in `qs.rs`).

use de_api::{Rect, Size};
use de_design::tokens::{self, Theme};
use de_paint::Canvas;

use crate::chrome::{self, style};
use crate::i18n::Strings;

/// Full shade: scrim + sheet with QS grid at top and notification rows below.
#[must_use]
pub fn render(size: Size, s: &Strings, t: &Theme, clock: &str) -> de_api::Pixels {
    let mut c = Canvas::filled(size, t.bg);
    chrome::status_bar(&mut c, t, &chrome::StatusInfo::new(clock, ""));
    c.fill_rect(Rect::new(0, 0, size.w, size.h), t.scrim);
    let sheet = Rect::new(
        chrome::MARGIN / 2,
        chrome::STATUS_H + 8,
        size.w - chrome::MARGIN,
        (size.h * 3 / 5).min(900),
    );
    c.fill_rounded_rect(sheet, tokens::RADIUS_SHEET, t.surface1);
    let labels = [
        "qs.wifi",
        "qs.bluetooth",
        "qs.dnd",
        "qs.airplane",
        "qs.rotate",
        "qs.battery_saver",
    ];
    let tile = (sheet.w - 64 - 5 * 16) / 6;
    for (i, k) in labels.iter().enumerate() {
        let tx = sheet.x + 32 + i as i32 * (tile + 16);
        c.fill_rounded_rect(
            Rect::new(tx, sheet.y + 24, tile, tile),
            tokens::RADIUS_CHIP,
            t.surface3,
        );
        chrome::draw_centered_x(
            &mut c,
            s.get(k),
            &style::caption(t.text2),
            tx + tile / 2,
            sheet.y + 24 + tile + 8,
        );
    }
    let mut y = sheet.y + 24 + tile + 48;
    for _ in 0..3 {
        c.fill_rounded_rect(
            Rect::new(sheet.x + 16, y, sheet.w - 32, 112),
            tokens::RADIUS_ROW,
            t.glass,
        );
        y += 112 + 12;
        if y + 112 > sheet.y + sheet.h - 16 {
            break;
        }
    }
    chrome::nav_pill(&mut c, t);
    c.into_pixels()
}
