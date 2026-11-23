//! Keyguard credential bouncer overlay (`docs/DE-KEYGUARD-AUTH.md` §6, Phase 1).
//!
//! Full-screen PIN entry surface shown when swiping up on the ambient lock
//! screen (`screens/lock.rs`) with `lock_method != Swipe`: blurred wallpaper
//! backdrop + dim scrim, Cancel/back affordance, title + error/cooldown line,
//! PIN dot row, 3×4 numeric keypad (`1`–`9`, Backspace, `0`, Submit), and an
//! optional Emergency Call pill at the bottom.
//!
//! Layout, rendering and hit-testing share one set of pure geometry helpers so
//! pointer/touch coordinates always match the painted targets.

use de_api::{Pixels, Point, Rect, Size};
use de_design::tokens::{self, Theme};
use de_paint::{text, Canvas};

use crate::chrome::{self, style};
use crate::i18n::Strings;

/// Minimum and maximum PIN dot slots rendered in the indicator row.
const MIN_DOTS: usize = 4;
const MAX_DOTS: usize = 12;
const DOT_D: i32 = 18;
const DOT_GAP: i32 = 16;

/// Cancel pill geometry at top-left below the status bar.
const CANCEL_W: i32 = 120;
const CANCEL_H: i32 = 48;

/// Emergency pill geometry above the bottom gesture bar.
const EMERGENCY_W: i32 = 280;
const EMERGENCY_H: i32 = 52;
const EMERGENCY_LIFT: i32 = 40;

/// Keypad spacing constants (scaled down automatically on compact outputs).
const KEY_D_MAX: i32 = 116;
const KEY_COL_GAP: i32 = 36;
const KEY_ROW_GAP: i32 = 24;

/// Everything the bouncer renders — pure snapshot passed in by the compositor.
pub struct BouncerModel<'a> {
    pub entered_len: usize,
    pub max_len: usize,
    pub error: Option<&'a str>,
    pub cooldown_secs: Option<u32>,
    pub checking: bool,
    pub emergency_available: bool,
    pub backdrop: &'a Pixels,
}

/// Tappable targets on the bouncer surface (`hit` mirrors `render` geometry).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BouncerHit {
    Digit(u8),
    Backspace,
    Submit,
    Cancel,
    Emergency,
}

const KEYPAD_ROWS: [[BouncerHit; 3]; 4] = [
    [
        BouncerHit::Digit(1),
        BouncerHit::Digit(2),
        BouncerHit::Digit(3),
    ],
    [
        BouncerHit::Digit(4),
        BouncerHit::Digit(5),
        BouncerHit::Digit(6),
    ],
    [
        BouncerHit::Digit(7),
        BouncerHit::Digit(8),
        BouncerHit::Digit(9),
    ],
    [
        BouncerHit::Backspace,
        BouncerHit::Digit(0),
        BouncerHit::Submit,
    ],
];

fn cancel_rect() -> Rect {
    Rect::new(chrome::MARGIN, chrome::STATUS_H + 16, CANCEL_W, CANCEL_H)
}

fn emergency_rect(size: Size) -> Rect {
    let y = size.h - tokens::GESTURE_BAR_HEIGHT - EMERGENCY_LIFT - EMERGENCY_H;
    Rect::new((size.w - EMERGENCY_W) / 2, y, EMERGENCY_W, EMERGENCY_H)
}

/// Compute the 12 keypad button rectangles in row-major order matching [`KEYPAD_ROWS`].
fn keypad_rects(size: Size) -> [(BouncerHit, Rect); 12] {
    let max_by_w = (chrome::content_w(size) - KEY_COL_GAP * 2) / 3;
    let emerg_top = emergency_rect(size).y;
    let top_reserved = chrome::STATUS_H + 340;
    let avail_h = (emerg_top - 40 - top_reserved).max(240);
    let max_by_h = (avail_h - KEY_ROW_GAP * 3) / 4;
    let key_d = KEY_D_MAX.min(max_by_w).min(max_by_h).max(48);

    let grid_w = key_d * 3 + KEY_COL_GAP * 2;
    let grid_h = key_d * 4 + KEY_ROW_GAP * 3;
    let origin_x = (size.w - grid_w) / 2;
    let origin_y = emerg_top - 40 - grid_h;

    let mut out = [(BouncerHit::Digit(0), Rect::new(0, 0, 0, 0)); 12];
    let mut idx = 0;
    for (row_idx, row) in KEYPAD_ROWS.iter().enumerate() {
        for (col_idx, &hit) in row.iter().enumerate() {
            let x = origin_x + (col_idx as i32) * (key_d + KEY_COL_GAP);
            let y = origin_y + (row_idx as i32) * (key_d + KEY_ROW_GAP);
            out[idx] = (hit, Rect::new(x, y, key_d, key_d));
            idx += 1;
        }
    }
    out
}

/// Lookup an i18n key, falling back to `default` when the locale catalog does
/// not yet define that key.
fn tr<'a>(strings: &'a Strings, key: &'a str, default: &'a str) -> &'a str {
    let resolved = strings.get(key);
    if resolved == key {
        default
    } else {
        resolved
    }
}

/// Hit-test `p` against the bouncer controls.
///
/// While `m.cooldown_secs.is_some()`, all keypad keys (`Digit`, `Backspace`,
/// `Submit`) are disabled, while `Cancel` and `Emergency` remain tappable.
#[must_use]
pub fn hit(size: Size, p: Point, m: &BouncerModel) -> Option<BouncerHit> {
    if cancel_rect().contains(p) {
        return Some(BouncerHit::Cancel);
    }
    if m.emergency_available && emergency_rect(size).contains(p) {
        return Some(BouncerHit::Emergency);
    }
    if m.cooldown_secs.is_some() || m.checking {
        return None;
    }
    for (target, rect) in keypad_rects(size) {
        if rect.contains(p) {
            return Some(target);
        }
    }
    None
}

/// Render the full-screen PIN bouncer surface (opaque over blurred backdrop).
#[must_use]
pub fn render(size: Size, theme: &Theme, m: &BouncerModel, strings: &Strings) -> Pixels {
    let mut c = Canvas::new(size);
    c.blit(m.backdrop, Point::new(0, 0));
    // Stronger dim scrim than ambient lock so keypad digits read cleanly on any wallpaper.
    c.fill_rect(Rect::new(0, 0, size.w, size.h), [0x10, 0x0B, 0x14, 0xA8]);

    // Top-left Cancel / back pill.
    let cancel = cancel_rect();
    c.fill_rounded_rect(cancel, tokens::RADIUS_PILL, [0xFF, 0xF8, 0xF0, 0x2E]);
    chrome::draw_ink_centered(
        &mut c,
        tr(strings, "common.cancel", "Cancel"),
        &style::label([0xFF, 0xF8, 0xF0, 0xF0]),
        cancel,
    );

    // Title.
    let title_y = chrome::STATUS_H + 104;
    chrome::draw_centered(
        &mut c,
        tr(strings, "bouncer.enter_pin", "Enter PIN"),
        &style::h2([0xFF, 0xF8, 0xF0, 0xFF]),
        title_y,
    );

    // Status line: cooldown countdown takes precedence over checking/error text.
    let status_y = title_y + 56;
    if let Some(secs) = m.cooldown_secs {
        let msg =
            tr(strings, "bouncer.cooldown", "Try again in {s}s").replace("{s}", &secs.to_string());
        chrome::draw_centered(&mut c, &msg, &style::label(theme.warning), status_y);
    } else if m.checking {
        let msg = tr(strings, "bouncer.checking", "Checking PIN…");
        chrome::draw_centered(
            &mut c,
            msg,
            &style::label([0xFF, 0xF8, 0xF0, 0xCC]),
            status_y,
        );
    } else if let Some(err) = m.error {
        chrome::draw_centered(&mut c, err, &style::label(theme.danger), status_y);
    }

    // PIN dot row.
    let dot_count = m.max_len.clamp(MIN_DOTS, MAX_DOTS);
    let filled = m.entered_len.min(dot_count);
    let row_w = (dot_count as i32) * DOT_D + ((dot_count - 1) as i32) * DOT_GAP;
    let dots_x = (size.w - row_w) / 2;
    let dots_y = status_y + 64;
    for i in 0..dot_count {
        let x = dots_x + (i as i32) * (DOT_D + DOT_GAP);
        let dot_rect = Rect::new(x, dots_y, DOT_D, DOT_D);
        let color = if i < filled {
            [0xFF, 0xF8, 0xF0, 0xFF]
        } else {
            [0xFF, 0xF8, 0xF0, 0x44]
        };
        c.fill_rounded_rect(dot_rect, tokens::RADIUS_PILL, color);
    }

    // 3×4 numeric keypad.
    let locked_out = m.cooldown_secs.is_some() || m.checking;
    let digit_style = text::LabelStyle {
        size: 40.0,
        line_height: 44.0,
        weight: 400,
        tnum: true,
        color: if locked_out {
            [0xFF, 0xF8, 0xF0, 0x55]
        } else {
            [0xFF, 0xF8, 0xF0, 0xFF]
        },
        ..text::LabelStyle::default()
    };
    let icon_fg = if locked_out {
        [0xFF, 0xF8, 0xF0, 0x55]
    } else {
        [0xFF, 0xF8, 0xF0, 0xFF]
    };

    for (key, rect) in keypad_rects(size) {
        let fill = match key {
            BouncerHit::Submit if !locked_out && m.entered_len > 0 => theme.accent,
            _ if locked_out => [0xFF, 0xF8, 0xF0, 0x1A],
            _ => [0xFF, 0xF8, 0xF0, 0x38],
        };
        c.fill_rounded_rect(rect, tokens::RADIUS_PILL, fill);

        match key {
            BouncerHit::Digit(d) => {
                let label = (b'0' + d) as char;
                let mut buf = [0_u8; 4];
                chrome::draw_ink_centered(&mut c, label.encode_utf8(&mut buf), &digit_style, rect);
            }
            BouncerHit::Backspace => {
                chrome::draw_ink_centered(
                    &mut c,
                    "\u{e14a}",
                    &text::LabelStyle::icon(34.0, icon_fg),
                    rect,
                );
            }
            BouncerHit::Submit => {
                let fg = if !locked_out && m.entered_len > 0 {
                    theme.text_on_accent
                } else {
                    icon_fg
                };
                chrome::draw_ink_centered(
                    &mut c,
                    "\u{e5ca}",
                    &text::LabelStyle::icon(36.0, fg),
                    rect,
                );
            }
            BouncerHit::Cancel | BouncerHit::Emergency => {}
        }
    }

    // Optional Emergency Call pill at bottom.
    if m.emergency_available {
        let emerg = emergency_rect(size);
        c.fill_rounded_rect(emerg, tokens::RADIUS_PILL, [0xFF, 0xF8, 0xF0, 0x33]);
        chrome::draw_ink_centered(
            &mut c,
            tr(strings, "bouncer.emergency", "Emergency"),
            &style::label([0xFF, 0xF8, 0xF0, 0xF5]),
            emerg,
        );
    }

    chrome::nav_pill(&mut c, theme);
    c.into_pixels()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn center(r: Rect) -> Point {
        Point::new(r.x + r.w / 2, r.y + r.h / 2)
    }

    fn base_model(backdrop: &Pixels) -> BouncerModel<'_> {
        BouncerModel {
            entered_len: 2,
            max_len: 6,
            error: None,
            cooldown_secs: None,
            checking: false,
            emergency_available: true,
            backdrop,
        }
    }

    #[test]
    fn hit_tests_all_digits_and_controls() {
        let size = Size::new(720, 1600);
        let backdrop = Pixels::empty(size);
        let m = base_model(&backdrop);

        // Every keypad button (0..=9, Backspace, Submit) hits at its center.
        let mut seen_digits = [false; 10];
        let mut seen_backspace = false;
        let mut seen_submit = false;
        for (expected, rect) in keypad_rects(size) {
            assert_eq!(hit(size, center(rect), &m), Some(expected));
            match expected {
                BouncerHit::Digit(d) => seen_digits[d as usize] = true,
                BouncerHit::Backspace => seen_backspace = true,
                BouncerHit::Submit => seen_submit = true,
                BouncerHit::Cancel | BouncerHit::Emergency => {}
            }
        }
        assert!(seen_digits.into_iter().all(|v| v));
        assert!(seen_backspace);
        assert!(seen_submit);

        // Cancel and Emergency pills hit at their centers.
        assert_eq!(
            hit(size, center(cancel_rect()), &m),
            Some(BouncerHit::Cancel)
        );
        assert_eq!(
            hit(size, center(emergency_rect(size)), &m),
            Some(BouncerHit::Emergency)
        );

        // Emergency hidden when unavailable.
        let no_emerg = BouncerModel {
            emergency_available: false,
            ..m
        };
        assert_eq!(hit(size, center(emergency_rect(size)), &no_emerg), None);
    }

    #[test]
    fn keypad_disabled_during_cooldown_but_cancel_and_emergency_stay_tappable() {
        let size = Size::new(720, 1600);
        let backdrop = Pixels::empty(size);
        let m = BouncerModel {
            cooldown_secs: Some(30),
            ..base_model(&backdrop)
        };

        for (_, rect) in keypad_rects(size) {
            assert_eq!(hit(size, center(rect), &m), None);
        }
        assert_eq!(
            hit(size, center(cancel_rect()), &m),
            Some(BouncerHit::Cancel)
        );
        assert_eq!(
            hit(size, center(emergency_rect(size)), &m),
            Some(BouncerHit::Emergency)
        );
    }

    #[test]
    fn render_smoke_produces_full_rgba_buffer() {
        let size = Size::new(720, 1600);
        let backdrop = Pixels::empty(size);
        let strings = Strings::default();
        let m = BouncerModel {
            error: Some("Incorrect PIN"),
            ..base_model(&backdrop)
        };

        let px = render(size, &tokens::DEFAULT, &m, &strings);
        assert_eq!(px.size, size);
        assert_eq!(px.data.len(), (size.w * size.h * 4) as usize);

        let cooldown_m = BouncerModel {
            cooldown_secs: Some(25),
            ..m
        };
        let px_cd = render(size, &tokens::DARK, &cooldown_m, &strings);
        assert_eq!(px_cd.data.len(), (size.w * size.h * 4) as usize);
    }
}
