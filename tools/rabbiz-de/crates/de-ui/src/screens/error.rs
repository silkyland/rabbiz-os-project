//! P18 error skeleton: "app not responding" sheet with Wait/Close buttons.

use de_api::{Point, Rect, Size};
use de_design::tokens::{self, Theme};
use de_paint::Canvas;

use crate::chrome::{self, style};
use crate::i18n::Strings;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorHit {
    Wait,
    Close,
}

fn layout(size: Size) -> (Rect, Rect, Rect) {
    let sheet = Rect::new(
        chrome::MARGIN * 2,
        (size.h - 344) / 2,
        size.w - chrome::MARGIN * 4,
        344,
    );
    let bw = (sheet.w - 56 - 16) / 2;
    let wait = Rect::new(sheet.x + 28, sheet.y + sheet.h - 116, bw, 88);
    let close = Rect::new(sheet.x + 28 + bw + 16, sheet.y + sheet.h - 116, bw, 88);
    (sheet, wait, close)
}

#[must_use]
pub fn hit_test(size: Size, pos: Point) -> Option<ErrorHit> {
    let (_, wait, close) = layout(size);
    if wait.contains(pos) {
        Some(ErrorHit::Wait)
    } else if close.contains(pos) {
        Some(ErrorHit::Close)
    } else {
        None
    }
}

#[must_use]
pub fn render(size: Size, s: &Strings, t: &Theme, app: &str) -> de_api::Pixels {
    let mut c = Canvas::filled(size, t.bg);
    c.fill_rect(Rect::new(0, 0, size.w, size.h), t.scrim);
    let (sheet, wait, close) = layout(size);
    c.fill_rounded_rect(sheet, tokens::RADIUS_SHEET, t.surface1);
    chrome::draw(
        &mut c,
        s.get("error.app_not_responding"),
        &style::h2(t.text),
        sheet.x + 28,
        sheet.y + 32,
        Some(sheet.w - 56),
    );
    chrome::draw(
        &mut c,
        app,
        &style::body(t.text2),
        sheet.x + 28,
        sheet.y + 100,
        Some(sheet.w - 56),
    );
    // Wait / Close buttons (88 px tall touch targets).
    c.fill_rounded_rect(wait, tokens::RADIUS_PILL, t.surface3);
    c.fill_rounded_rect(close, tokens::RADIUS_PILL, t.danger);
    chrome::draw_centered_x(
        &mut c,
        s.get("error.wait"),
        &style::label(t.text),
        wait.x + wait.w / 2,
        wait.y + 30,
    );
    chrome::draw_centered_x(
        &mut c,
        s.get("error.close_app"),
        &style::label(t.text_on_accent),
        close.x + close.w / 2,
        close.y + 30,
    );
    c.into_pixels()
}
