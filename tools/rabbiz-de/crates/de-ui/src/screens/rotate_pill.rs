//! AOSP-style floating Rotate Suggestion pill shown when rotation lock is on
//! and the accelerometer detects a different orientation.

use de_api::{Pixels, Point, Rect, Size};
use de_design::tokens::{Theme, GESTURE_BAR_HEIGHT};
use de_paint::{text::LabelStyle, Canvas};

use crate::chrome;

/// Diameter of the floating rotate suggestion pill in logical px.
pub const PILL_SIZE: i32 = 52;

/// Screen-space rectangle of the rotate suggestion pill (bottom-right).
#[must_use]
pub fn pill_rect(size: Size) -> Rect {
    Rect::new(
        size.w - PILL_SIZE - 24,
        size.h - GESTURE_BAR_HEIGHT - PILL_SIZE - 20,
        PILL_SIZE,
        PILL_SIZE,
    )
}

/// Hit-test a tap on the rotate suggestion pill.
#[must_use]
pub fn hit(size: Size, pos: Point) -> bool {
    pill_rect(size).contains(pos)
}

/// Render the `PILL_SIZE × PILL_SIZE` floating rotate suggestion pill.
#[must_use]
pub fn render(theme: &Theme) -> Pixels {
    let s = Size::new(PILL_SIZE, PILL_SIZE);
    let mut c = Canvas::new(s);
    let r = Rect::new(0, 0, PILL_SIZE, PILL_SIZE);
    c.fill_rounded_rect(r, PILL_SIZE as f32 / 2.0, theme.surface2);
    chrome::draw_ink_centered(
        &mut c,
        "\u{e1c1}",
        &LabelStyle::icon(24.0, theme.accent),
        r,
    );
    c.into_pixels()
}

#[cfg(test)]
mod tests {
    use super::*;
    use de_design::tokens::DARK;

    #[test]
    fn pill_stays_on_screen_and_hits() {
        let size = Size::new(720, 1600);
        let r = pill_rect(size);
        assert!(r.x >= 0 && r.y >= 0);
        assert!(r.x + r.w <= size.w && r.y + r.h <= size.h);
        assert!(hit(size, Point::new(r.x + 10, r.y + 10)));
        assert!(!hit(size, Point::new(10, 10)));

        let px = render(&DARK);
        assert_eq!(px.size, Size::new(PILL_SIZE, PILL_SIZE));
        assert!(px.data.iter().any(|&b| b != 0));
    }
}
