//! P16 screenshot feedback: a 120 ms flash (drawn by the compositor as a
//! solid element — it must not appear in the capture itself) and a
//! thumbnail toast at the lower right for 3 s. The toast card is rendered
//! standalone here so the compositor can place it like the OSD pill.

use de_api::{Pixels, Rect, Size};
use de_design::tokens::{self, Theme};
use de_paint::Canvas;

use crate::chrome::{self, style};
use crate::i18n::Strings;

/// Toast card size; the thumbnail keeps the panel's 160×96 preview.
pub const TOAST_W: i32 = 320;
pub const TOAST_H: i32 = 128;
/// Margin from the right/bottom screen edges (above the nav zone).
const TOAST_MARGIN: i32 = 24;

/// Where the toast sits on a `size` output (bottom-right).
#[must_use]
pub fn toast_rect(size: Size) -> Rect {
    Rect::new(
        size.w - TOAST_W - TOAST_MARGIN,
        size.h - TOAST_H - tokens::GESTURE_BAR_HEIGHT - TOAST_MARGIN,
        TOAST_W,
        TOAST_H,
    )
}

/// Render the toast card: thumbnail left, filename + "Open" hint right.
#[must_use]
pub fn toast(t: &Theme, thumb: Option<&Pixels>, name: &str, s: &Strings) -> Pixels {
    let mut c = Canvas::filled(Size::new(TOAST_W, TOAST_H), [0, 0, 0, 0]);
    c.fill_rounded_rect(
        Rect::new(0, 0, TOAST_W, TOAST_H),
        tokens::RADIUS_CARD,
        t.surface2,
    );
    let thumb_r = Rect::new(16, (TOAST_H - 96) / 2, 144, 96);
    match thumb {
        Some(px) => c.blit_scaled(px, thumb_r, 1.0),
        None => c.fill_rounded_rect(thumb_r, tokens::RADIUS_CHIP, t.surface3),
    }
    let tx = thumb_r.x + thumb_r.w + 16;
    chrome::draw(&mut c, s.get("shot.saved"), &style::label(t.text), tx, 24, Some(TOAST_W - tx - 16));
    chrome::draw(&mut c, name, &style::caption(t.text3), tx, 58, Some(TOAST_W - tx - 16));
    chrome::draw(&mut c, s.get("shot.open"), &style::caption(t.accent), tx, 88, Some(TOAST_W - tx - 16));
    c.into_pixels()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toast_stays_on_screen() {
        let r = toast_rect(Size::new(720, 1600));
        assert!(r.x >= 0 && r.y >= 0);
        assert!(r.x + r.w <= 720);
        assert!(r.y + r.h <= 1600 - tokens::GESTURE_BAR_HEIGHT);
    }
}
