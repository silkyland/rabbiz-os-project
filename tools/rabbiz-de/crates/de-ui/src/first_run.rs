//! P19 first-run skeleton: language picker card + Get started button.

use de_api::{Rect, Size};
use de_design::tokens::{self, Theme};
use de_paint::Canvas;

use crate::chrome::{self, style};
use crate::i18n::Strings;

pub fn render(size: Size, s: &Strings, t: &Theme) -> de_api::Pixels {
    let mut c = Canvas::filled(size, t.bg);
    chrome::draw_centered(&mut c, s.get("first_run.title"), &style::h2(t.text), 240);
    chrome::draw_centered(&mut c, s.get("first_run.language"), &style::body(t.text2), 320);
    for (i, lang) in ["ไทย", "English"].iter().enumerate() {
        let r = Rect::new(chrome::MARGIN * 2, 380 + i as i32 * 96, size.w - chrome::MARGIN * 4, 80);
        c.fill_rounded_rect(r, tokens::RADIUS_ROW, t.surface1);
        chrome::draw_centered_x(&mut c, lang, &style::body(t.text), r.x + r.w / 2, r.y + 24);
    }
    let btn = Rect::new(chrome::MARGIN * 2, size.h - 240, size.w - chrome::MARGIN * 4, 72);
    c.fill_rounded_rect(btn, tokens::RADIUS_PILL, t.accent);
    chrome::draw_centered_x(&mut c, s.get("first_run.finish"), &style::label(t.text_on_accent), btn.x + btn.w / 2, btn.y + 20);
    c.into_pixels()
}
