//! P9 power menu: centered sheet over a scrim — Lock, Screenshot, Safe
//! mode, Restart, Power off. Destructive rows confirm with an 800 ms
//! hold: the armed row fills from the left while the finger stays down
//! (see `de-core::power` for the timing).

use de_api::{Pixels, Point, Rect, Size};
use de_design::tokens::{self, Theme};
use de_paint::{text, Canvas};

use crate::chrome::{self, style};
use crate::i18n::Strings;

/// One row's identity — order matches `ROWS`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerAction {
    Lock,
    Screenshot,
    SafeMode,
    Restart,
    PowerOff,
}

/// Rows that must be held to fire.
pub const DESTRUCTIVE: [PowerAction; 3] = [
    PowerAction::Restart,
    PowerAction::PowerOff,
    PowerAction::SafeMode,
];
/// Hold-to-confirm duration (ms).
pub const HOLD_MS: u64 = 800;

const ROW_H: i32 = 88;
const ROWS: [(PowerAction, &str, &str); 5] = [
    (PowerAction::Lock, "power.lock", "\u{e897}"),
    (PowerAction::Screenshot, "power.screenshot", "\u{e412}"),
    (PowerAction::SafeMode, "power.safe_mode", "\u{e8b8}"),
    (PowerAction::Restart, "power.restart", "\u{e5d5}"),
    (PowerAction::PowerOff, "power.power_off", "\u{e8ac}"),
];

/// What the menu renders: `armed` = the destructive row being held and its
/// 0..1 progress.
pub struct PowerModel {
    pub armed: Option<(PowerAction, f32)>,
}

fn sheet_rect(size: Size) -> Rect {
    let h = 96 + ROWS.len() as i32 * ROW_H;
    Rect::new(
        chrome::MARGIN * 2,
        (size.h - h) / 2,
        size.w - chrome::MARGIN * 4,
        h,
    )
}

fn row_rect(size: Size, i: usize) -> Rect {
    let sheet = sheet_rect(size);
    Rect::new(
        sheet.x + 8,
        sheet.y + 96 + i as i32 * ROW_H,
        sheet.w - 16,
        ROW_H,
    )
}

/// Tap/hold hit test. `None` = scrim (close).
#[must_use]
pub fn power_hit(size: Size, pos: Point) -> Option<PowerAction> {
    if !sheet_rect(size).contains(pos) {
        return None;
    }
    (0..ROWS.len())
        .find(|i| row_rect(size, *i).contains(pos))
        .map(|i| ROWS[i].0)
}

#[must_use]
pub fn render(size: Size, s: &Strings, t: &Theme, m: &PowerModel) -> Pixels {
    let mut c = Canvas::filled(size, t.bg);
    c.fill_rect(Rect::new(0, 0, size.w, size.h), t.scrim);
    let sheet = sheet_rect(size);
    c.fill_rounded_rect(sheet, tokens::RADIUS_SHEET, t.surface1);
    chrome::draw(
        &mut c,
        s.get("power.title"),
        &style::h2(t.text),
        sheet.x + 28,
        sheet.y + 28,
        None,
    );
    chrome::draw(
        &mut c,
        s.get("power.hold_hint"),
        &style::caption(t.text3),
        sheet.x + sheet.w - 220,
        sheet.y + 36,
        Some(192),
    );
    for (i, (action, key, glyph)) in ROWS.iter().enumerate() {
        let r = row_rect(size, i);
        let armed = matches!(m.armed, Some((a, _)) if a == *action);
        let frac = match m.armed {
            Some((a, f)) if a == *action => f,
            _ => 0.0,
        };
        if armed {
            c.fill_rounded_rect(r, tokens::RADIUS_ROW, t.accent_dim);
            // Hold progress: fills left→right; the row fires at 1.0.
            let fill_w = (r.w as f32 * frac.clamp(0.0, 1.0)) as i32;
            if fill_w > 8 {
                c.fill_rounded_rect(
                    Rect::new(r.x, r.y, fill_w, r.h),
                    tokens::RADIUS_ROW,
                    t.accent,
                );
            }
        }
        let col = if *action == PowerAction::PowerOff && !armed {
            t.danger
        } else if armed {
            t.text_on_accent
        } else {
            t.text
        };
        chrome::draw_ink_centered(
            &mut c,
            glyph,
            &text::LabelStyle::icon(30.0, col),
            Rect::new(r.x + 16, r.y, 56, r.h),
        );
        chrome::draw(
            &mut c,
            s.get(key),
            &style::body(col),
            r.x + 88,
            r.y + (ROW_H - 34) / 2,
            None,
        );
        if i > 0 && !armed {
            c.fill_rect(Rect::new(sheet.x + 24, r.y, sheet.w - 48, 1), t.line);
        }
    }
    chrome::nav_pill(&mut c, t);
    c.into_pixels()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_row_hits_and_scrim_closes() {
        let size = Size::new(720, 1600);
        for (i, (action, _, _)) in ROWS.iter().enumerate() {
            let r = row_rect(size, i);
            assert_eq!(
                power_hit(size, Point::new(r.x + r.w / 2, r.y + r.h / 2)),
                Some(*action)
            );
        }
        assert_eq!(power_hit(size, Point::new(4, 4)), None);
    }
}
