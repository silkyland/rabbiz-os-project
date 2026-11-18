//! P14 global search skeleton: 72 px field @ y=96, grouped results list.

use de_api::{Rect, Size};
use de_design::tokens::{self, Theme};
use de_paint::Canvas;

use crate::chrome::{self, style};
use crate::i18n::Strings;

pub fn render(size: Size, s: &Strings, t: &Theme, query: &str, results: &[&str]) -> de_api::Pixels {
    let mut c = Canvas::filled(size, t.bg);
    c.fill_rect(Rect::new(0, 0, size.w, size.h), t.scrim);
    let field = Rect::new(chrome::MARGIN, 96, chrome::content_w(size), 72);
    c.fill_rounded_rect(field, tokens::RADIUS_PILL, t.surface2);
    let shown = if query.is_empty() { s.get("search.hint") } else { query };
    chrome::draw(&mut c, shown, &style::body(if query.is_empty() { t.text3 } else { t.text }), field.x + 28, field.y + 20, None);
    let mut y = field.y + field.h + 24;
    for r in results {
        if y + 88 > size.h - 120 { break; }
        c.fill_rounded_rect(Rect::new(chrome::MARGIN, y, chrome::content_w(size), 80), tokens::RADIUS_ROW, t.surface1);
        chrome::draw(&mut c, r, &style::body(t.text), chrome::MARGIN + 24, y + 26, None);
        y += 80 + 12;
    }
    chrome::nav_pill(&mut c, t);
    c.into_pixels()
}
