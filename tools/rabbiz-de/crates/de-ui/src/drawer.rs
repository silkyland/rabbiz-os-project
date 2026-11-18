//! P3 app drawer skeleton: search field on top, A–Z list of all apps in
//! 96 px rows + right-edge alphabet scrub rail.

use de_api::{Rect, Size};
use de_design::tokens::{self, Theme};
use de_paint::Canvas;

use crate::chrome::{self, style};
use crate::i18n::Strings;

pub fn render(size: Size, s: &Strings, t: &Theme, clock: &str, apps: &[String]) -> de_api::Pixels {
    let mut c = Canvas::filled(size, t.canvas);
    chrome::status_bar(&mut c, s, t, clock, "");
    let sf = Rect::new(chrome::MARGIN, chrome::STATUS_H + 16, chrome::content_w(size), 56);
    c.fill_rounded_rect(sf, tokens::RADIUS_PILL, t.surface2);
    chrome::draw(&mut c, s.get("common.search"), &style::label(t.text3), sf.x + 24, sf.y + 15, None);
    let mut y = sf.y + sf.h + 16;
    for name in apps {
        if y > size.h - chrome::NAV_H - 96 {
            break;
        }
        chrome::draw(&mut c, name, &style::body(t.text), chrome::MARGIN + 24, y + 28, None);
        c.fill_rect(Rect::new(chrome::MARGIN + 24, y + 87, chrome::content_w(size) - 72, 1), t.line);
        y += 88;
    }
    // A–Z rail on the right edge.
    let mut ry = sf.y;
    for ch in "ABCDEFGHIJKLMNOPQRSTUVWXYZ".chars() {
        if ry > size.h - 80 { break; }
        chrome::draw(&mut c, &ch.to_string(), &style::caption(t.text3), size.w - 20, ry, None);
        ry += 28;
    }
    chrome::nav_pill(&mut c, t);
    c.into_pixels()
}
