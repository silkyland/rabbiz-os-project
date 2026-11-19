//! P1 lock screen (M4-T4): clock + date over a blurred wallpaper, a compact
//! charging/status pill, up to three notification cards in the lower half,
//! a rear-fingerprint prompt pill and the two circular bottom actions
//! (flashlight left, camera/quick-note right).
//!
//! The surface is one opaque texture the compositor slides 1:1 with the
//! unlock drag (`de-core::lock_ui`); this file owns layout, rendering and
//! the matching hit test.

use de_api::{Pixels, Point, Rect, Size};
use de_design::tokens::{self, Theme};
use de_paint::{glass, text, Canvas};

use crate::chrome::{self, style, StatusInfo};
use crate::i18n::Strings;
use crate::panels::Notification;

/// Notification preview cap; the remainder folds into "+N more".
pub const PREVIEW: usize = 3;
/// Card height in the preview (slimmer than the shade's 148 px cards).
const CARD_H: i32 = 96;
const CARD_GAP: i32 = 12;
/// Bottom action circle diameter and its height above the nav zone.
const ACTION_D: i32 = 88;
const ACTION_LIFT: i32 = 120;
/// Fingerprint pill height/offset above the action row.
const FP_PILL_H: i32 = 52;

/// The bottom-right action: rear camera when a camera app exists, else the
/// notes app ("Camera/Quick Note" in the spec is an either/or).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuickAction {
    Camera,
    Note,
}

/// Everything the lock screen draws — the compositor hands in live data,
/// the render fn never polls.
pub struct LockModel<'a> {
    pub status: &'a StatusInfo,
    pub clock: &'a str,
    pub date: &'a str,
    /// Charging line ("Charging"/"Fully charged"/"" when discharging).
    pub charging: &'a str,
    /// Cards shown top-down; `hidden` folds into "+N more".
    pub notifications: &'a [Notification],
    pub hidden_notifications: usize,
    /// Rear sensor present → show the prompt pill.
    pub fingerprint: bool,
    /// `Some(on)` when the torch exists; None hides the button.
    pub flashlight: Option<bool>,
    pub quick: Option<QuickAction>,
    /// Pre-blurred wallpaper, full-screen. Baked once per lock — the lock
    /// must be opaque so home content never ghosts through.
    pub backdrop: &'a Pixels,
}

/// Tappable targets (`lock_hit` mirrors `render` geometry).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockHit {
    Flashlight,
    Quick,
}

fn flashlight_rect(size: Size) -> Rect {
    Rect::new(
        chrome::MARGIN,
        size.h - tokens::GESTURE_BAR_HEIGHT - ACTION_LIFT - ACTION_D,
        ACTION_D,
        ACTION_D,
    )
}

fn quick_rect(size: Size) -> Rect {
    Rect::new(
        size.w - chrome::MARGIN - ACTION_D,
        size.h - tokens::GESTURE_BAR_HEIGHT - ACTION_LIFT - ACTION_D,
        ACTION_D,
        ACTION_D,
    )
}

fn fp_pill_rect(size: Size) -> Rect {
    let y = size.h - tokens::GESTURE_BAR_HEIGHT - ACTION_LIFT - ACTION_D - FP_PILL_H - 20;
    Rect::new((size.w - 420) / 2, y, 420, FP_PILL_H)
}

/// Card rectangles, top-down in the lower half, bounded above the fp pill.
fn card_rects(size: Size, count: usize) -> Vec<Rect> {
    let top_limit = size.h / 2 + 40;
    let bottom_limit = fp_pill_rect(size).y - 16;
    let mut out = Vec::new();
    let mut y = top_limit.max(size.h / 2);
    for _ in 0..count.min(PREVIEW) {
        if y + CARD_H > bottom_limit {
            break;
        }
        out.push(Rect::new(chrome::MARGIN, y, chrome::content_w(size), CARD_H));
        y += CARD_H + CARD_GAP;
    }
    out
}

/// Hit test against the two bottom circles; everything else is a drag.
#[must_use]
pub fn lock_hit(size: Size, pos: Point) -> Option<LockHit> {
    if flashlight_rect(size).contains(pos) {
        Some(LockHit::Flashlight)
    } else if quick_rect(size).contains(pos) {
        Some(LockHit::Quick)
    } else {
        None
    }
}

fn action_circle(c: &mut Canvas, t: &Theme, r: Rect, glyph: &str, lit: bool) {
    let tint = if lit { [0xFF, 0xF8, 0xF0, 0xB8] } else { [0xFF, 0xF8, 0xF0, 0x50] };
    c.fill_rounded_rect(r, tokens::RADIUS_PILL, tint);
    let fg = if lit { t.accent } else { [0xFF, 0xF8, 0xF0, 0xFF] };
    chrome::draw_ink_centered(c, glyph, &text::LabelStyle::icon(36.0, fg), r);
}

/// Render the whole lock surface (opaque — caller moves/fades the texture).
#[must_use]
pub fn render(size: Size, s: &Strings, t: &Theme, m: &LockModel) -> Pixels {
    let mut c = Canvas::new(size);
    c.blit(m.backdrop, Point::new(0, 0));
    // Darken toward the bottom so cream glyphs read on any wallpaper.
    c.fill_rect(Rect::new(0, 0, size.w, size.h), [0x10, 0x0B, 0x14, 0x30]);
    c.fill_rect(
        Rect::new(0, size.h / 2, size.w, size.h - size.h / 2),
        [0x10, 0x0B, 0x14, 0x40],
    );

    chrome::status_bar_on_wallpaper(&mut c, t, m.status);

    // Date above the clock; clock uses tnum at the P1 size/weight.
    chrome::draw(&mut c, m.date, &style::label([0xFF, 0xF8, 0xF0, 0xD8]), 40, 128, None);
    let clock_style = text::LabelStyle {
        size: 120.0,
        line_height: 128.0,
        weight: 300,
        tnum: true,
        color: [0xFF, 0xF8, 0xF0, 0xFF],
        ..text::LabelStyle::default()
    };
    chrome::draw(&mut c, m.clock, &clock_style, 40, 168, None);

    if !m.charging.is_empty() {
        let r = Rect::new(40, 320, 300, 48);
        c.fill_rounded_rect(r, tokens::RADIUS_PILL, [0xFF, 0xF8, 0xF0, 0x3D]);
        chrome::draw_ink_centered(
            &mut c,
            "\u{e1a3}",
            &text::LabelStyle::icon(26.0, t.accent),
            Rect::new(r.x + 12, r.y, 48, r.h),
        );
        chrome::draw(&mut c, m.charging, &style::label([0xFF, 0xF8, 0xF0, 0xFF]), r.x + 60, r.y + 10, None);
    }

    // Notification preview: same frosted cards as the shade, capped.
    for (n, r) in m.notifications.iter().zip(card_rects(size, m.notifications.len())) {
        glass::frost(&mut c, m.backdrop, r, tokens::RADIUS_CARD, [0xFF, 0xF8, 0xF0, 0xC8]);
        let icon = Rect::new(r.x + 20, r.y + (CARD_H - 56) / 2, 56, 56);
        if let Some(px) = &n.icon {
            c.blit_scaled(px, icon, 1.0);
        } else {
            c.fill_rounded_rect(icon, 999.0, [0x30, 0x21, 0x35, 0x24]);
            chrome::draw_ink_centered(
                &mut c,
                &n.app
                    .chars()
                    .next()
                    .map_or_else(String::new, |ch| ch.to_uppercase().collect()),
                &style::h2(tokens::LIGHT.text),
                icon,
            );
        }
        // Card uses light cream frost, so keep dark plum ink in both themes.
        let ink = tokens::LIGHT;
        let tx = r.x + 96;
        let when = text::label(&n.when, style::caption(ink.text3), None);
        let when_w = when.as_ref().map_or(0, |p| p.width() as i32);
        if let Some(px) = text::label_clamped(&n.title, style::label(ink.text), r.w - 96 - when_w - 48, 1) {
            c.blit(
                &Pixels::new(Size::new(px.width() as i32, px.height() as i32), px.data().to_vec()),
                Point::new(tx, r.y + 16),
            );
        }
        chrome::draw(
            &mut c,
            &n.when,
            &style::caption(ink.text3),
            r.x + r.w - 20 - when_w,
            r.y + 18,
            None,
        );
        if let Some(px) = text::label_clamped(&n.body, style::caption(ink.text2), r.w - 96 - 24, 1) {
            c.blit(
                &Pixels::new(Size::new(px.width() as i32, px.height() as i32), px.data().to_vec()),
                Point::new(tx, r.y + 50),
            );
        }
    }
    if m.hidden_notifications > 0 {
        let last_y = card_rects(size, m.notifications.len())
            .last()
            .map_or(size.h / 2, |r| r.y + r.h);
        let more = s
            .get("lock.more_notifications")
            .replace("{n}", &m.hidden_notifications.to_string());
        chrome::draw(
            &mut c,
            &more,
            &style::caption([0xFF, 0xF8, 0xF0, 0xB0]),
            chrome::MARGIN + 20,
            last_y + 8,
            None,
        );
    }

    // Rear-fingerprint prompt — a hint, never a fake on-screen button.
    if m.fingerprint {
        let r = fp_pill_rect(size);
        c.fill_rounded_rect(r, tokens::RADIUS_PILL, [0xFF, 0xF8, 0xF0, 0x40]);
        chrome::draw_ink_centered(
            &mut c,
            "\u{e90d}",
            &text::LabelStyle::icon(30.0, [0xFF, 0xF8, 0xF0, 0xE0]),
            Rect::new(r.x + 16, r.y, FP_PILL_H, FP_PILL_H),
        );
        chrome::draw(
            &mut c,
            s.get("lock.fingerprint_hint"),
            &style::caption([0xFF, 0xF8, 0xF0, 0xE0]),
            r.x + 76,
            r.y + 12,
            Some(r.w - 92),
        );
    }

    // Bottom actions: flashlight left, camera/quick-note right.
    if let Some(on) = m.flashlight {
        action_circle(&mut c, t, flashlight_rect(size), "\u{e3e7}", on);
    }
    if let Some(q) = m.quick {
        let glyph = match q {
            QuickAction::Camera => "\u{e412}",
            QuickAction::Note => "\u{e3c9}",
        };
        action_circle(&mut c, t, quick_rect(size), glyph, false);
    }

    chrome::draw_centered(
        &mut c,
        s.get("lock.unlock_hint"),
        &style::caption([0xFF, 0xF8, 0xF0, 0xD8]),
        size.h - tokens::GESTURE_BAR_HEIGHT - 44,
    );
    chrome::nav_pill(&mut c, t);
    c.into_pixels()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bottom_circles_hit_nothing_else_does() {
        let size = Size::new(720, 1600);
        assert_eq!(
            lock_hit(size, Point::new(chrome::MARGIN + 10, 1600 - 32 - 120 - 80)),
            Some(LockHit::Flashlight)
        );
        assert_eq!(
            lock_hit(size, Point::new(720 - chrome::MARGIN - 10, 1600 - 32 - 120 - 80)),
            Some(LockHit::Quick)
        );
        assert_eq!(lock_hit(size, Point::new(360, 400)), None);
    }

    #[test]
    fn cards_stay_in_the_lower_half() {
        let size = Size::new(720, 1600);
        let rects = card_rects(size, 9);
        assert!(rects.len() <= PREVIEW);
        for r in &rects {
            assert!(r.y >= size.h / 2);
            assert!(r.y + r.h <= fp_pill_rect(size).y);
        }
    }
}
