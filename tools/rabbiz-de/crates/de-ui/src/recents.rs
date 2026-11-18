//! P4 recents skeleton: horizontal portrait card carousel, 1:1 drag track.

use de_api::{Rect, Size};
use de_design::tokens::{self, Theme};
use de_paint::Canvas;

use crate::chrome::{self, style};
use crate::i18n::Strings;

const CARD_W: i32 = 480;
const CARD_H: i32 = 1040;

pub fn render(size: Size, s: &Strings, t: &Theme, clock: &str, apps: &[String]) -> de_api::Pixels {
    let mut c = Canvas::filled(size, t.bg);
    chrome::status_bar(&mut c, s, t, clock, "");
    if apps.is_empty() {
        chrome::draw_centered(&mut c, s.get("recents.empty"), &style::body(t.text3), size.h / 2);
    }
    let y = (size.h - CARD_H) / 2;
    let mut x = (size.w - CARD_W) / 2;
    for name in apps.iter().take(4) {
        c.fill_rounded_rect(Rect::new(x, y, CARD_W, CARD_H), tokens::RADIUS_CARD, t.surface2);
        chrome::draw(&mut c, name, &style::label(t.text2), x + 24, y - 40, None);
        x += CARD_W + 32;
    }
    chrome::nav_pill(&mut c, t);
    c.into_pixels()
}
