//! P10/P11 OSD skeleton: left-edge pill 96×420 @ x=24/y=590, icon + 20
//! segments + tnum percentage.

use de_api::{Rect, Size};
use de_design::tokens::{self, Theme};
use de_paint::Canvas;

use crate::chrome::{self, style};
use crate::i18n::Strings;

#[must_use]
pub fn render(
    size: Size,
    _s: &Strings,
    t: &Theme,
    volume: f32,
    label: &str,
    glyph: &str,
) -> de_api::Pixels {
    let mut c = Canvas::filled(size, t.bg);
    chrome::status_bar(&mut c, t, &chrome::StatusInfo::default());
    let pill = Rect::new(24, (size.h - 420) / 2, 96, 420);
    c.fill_rounded_rect(pill, tokens::RADIUS_PILL, t.surface2);
    // Glyph at top.
    chrome::draw_centered_x(
        &mut c,
        glyph,
        &de_paint::LabelStyle::icon(36.0, t.text),
        pill.x + pill.w / 2,
        pill.y + 24,
    );
    // 20 segments.
    let seg_h = 12;
    let filled = (volume.clamp(0.0, 1.0) * 20.0).round() as i32;
    for i in 0..20 {
        let y = pill.y + 80 + (19 - i) * (seg_h + 4);
        let col = if i < filled { t.accent } else { t.surface3 };
        c.fill_rounded_rect(Rect::new(pill.x + 20, y, pill.w - 40, seg_h), 6.0, col);
    }
    let pct = format!("{}%", (volume * 100.0).round() as i32);
    chrome::draw_centered_x(
        &mut c,
        &pct,
        &style::label(t.text),
        pill.x + pill.w / 2,
        pill.y + pill.h - 56,
    );
    let _ = label;
    chrome::nav_pill(&mut c, t);
    c.into_pixels()
}
