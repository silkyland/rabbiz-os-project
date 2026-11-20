//! Heads-Up Notification (HUN) floating banner rendered into
//! `DisplayArea::NotificationHun`.
//!
//! Geometry is pure and shared by [`banner_rect`], [`render`], and [`hit`], so
//! touch hit-testing can never disagree with what is painted.

use de_api::{Pixels, Point, Rect, Size};
use de_design::tokens::{self, Theme};
use de_paint::{text, Canvas};

use crate::chrome::{self, style};

/// Banner height without action buttons (logical px).
pub const BANNER_H_PLAIN: i32 = 124;
/// Banner height with action pill buttons (logical px).
pub const BANNER_H_ACTIONS: i32 = 168;
/// Vertical gap below `top_inset` (status bar / cutout safe area).
pub const BANNER_TOP_GAP: i32 = 12;
/// Maximum number of action pill buttons displayed on a HUN banner.
pub const MAX_ACTIONS: usize = 3;

const BANNER_MARGIN_X: i32 = tokens::PAGE_MARGIN;
const BANNER_MAX_W: i32 = 672;
const PAD_X: i32 = 20;
const PAD_BOTTOM: i32 = 14;
const ACTION_H: i32 = 36;
const ACTION_GAP: i32 = 10;

/// Data rendered on a Heads-Up Notification banner.
#[derive(Debug, Clone, Copy)]
pub struct HunBannerModel<'a> {
    pub id: u32,
    pub app: &'a str,
    pub title: &'a str,
    pub body: &'a str,
    /// `(action_key, label)` pairs; at most [`MAX_ACTIONS`] are shown.
    pub actions: &'a [(String, String)],
    pub critical: bool,
}

/// Interactive region hit by a tap on the HUN banner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HunBannerHit {
    /// Tap on the banner card outside any action pill.
    Body,
    /// Tap on the `i`-th action pill button (`0..MAX_ACTIONS`).
    Action(usize),
}

/// Centered floating banner rectangle below `top_inset + 12`.
#[must_use]
pub fn banner_rect(output_size: Size, top_inset: i32, has_actions: bool) -> Rect {
    let avail_w = (output_size.w - BANNER_MARGIN_X * 2).max(0);
    let w = avail_w.min(BANNER_MAX_W);
    let x = (output_size.w - w) / 2;
    let y = top_inset + BANNER_TOP_GAP;
    let h = if has_actions {
        BANNER_H_ACTIONS
    } else {
        BANNER_H_PLAIN
    };
    Rect::new(x, y, w, h)
}

/// Compute action pill button rectangles inside `card_rect`.
fn action_rects(card_rect: Rect, action_count: usize) -> Vec<Rect> {
    let count = action_count.min(MAX_ACTIONS);
    if count == 0 || card_rect.w <= PAD_X * 2 {
        return Vec::new();
    }
    let row_y = card_rect.y + card_rect.h - PAD_BOTTOM - ACTION_H;
    let inner_w = card_rect.w - PAD_X * 2;
    let total_gap = ACTION_GAP * (count as i32 - 1);
    let btn_w = ((inner_w - total_gap) / count as i32).max(0);
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let bx = card_rect.x + PAD_X + (i as i32) * (btn_w + ACTION_GAP);
        out.push(Rect::new(bx, row_y, btn_w, ACTION_H));
    }
    out
}

/// Render the standalone HUN banner on a transparent RGBA buffer of `size`.
#[must_use]
pub fn render(theme: &Theme, m: &HunBannerModel<'_>, size: Size) -> Pixels {
    let mut c = Canvas::filled(size, [0, 0, 0, 0]);
    let bounds = Rect::new(0, 0, size.w, size.h);

    // Outer border (critical accent or subtle divider) + inner surface card.
    let border = if m.critical { 3 } else { 1 };
    let border_col = if m.critical { theme.danger } else { theme.line };
    c.fill_rounded_rect(bounds, tokens::RADIUS_CARD, border_col);

    let inner = Rect::new(
        border,
        border,
        (size.w - border * 2).max(0),
        (size.h - border * 2).max(0),
    );
    let inner_radius = (tokens::RADIUS_CARD - border as f32).max(0.0);
    c.fill_rounded_rect(inner, inner_radius, theme.surface2);

    let text_w = Some((size.w - PAD_X * 2).max(1));

    // App name header.
    let app_color = if m.critical {
        theme.danger
    } else {
        theme.accent
    };
    let app_style = text::LabelStyle {
        size: 18.0,
        line_height: 22.0,
        weight: 600,
        color: app_color,
        ..text::LabelStyle::default()
    };
    chrome::draw(&mut c, m.app, &app_style, PAD_X, 14, text_w);

    // Notification title.
    let title_style = text::LabelStyle {
        size: 24.0,
        line_height: 30.0,
        weight: 600,
        color: theme.text,
        ..text::LabelStyle::default()
    };
    chrome::draw(&mut c, m.title, &title_style, PAD_X, 40, text_w);

    // Optional body line.
    if !m.body.is_empty() {
        chrome::draw(
            &mut c,
            m.body,
            &style::caption(theme.text2),
            PAD_X,
            74,
            text_w,
        );
    }

    // Action pill buttons (up to MAX_ACTIONS).
    let pills = action_rects(bounds, m.actions.len());
    for (idx, (pill_rect, (key, label))) in pills
        .into_iter()
        .zip(m.actions.iter().take(MAX_ACTIONS))
        .enumerate()
    {
        let primary_critical = m.critical && idx == 0;
        let fill = if primary_critical {
            theme.danger
        } else {
            theme.surface3
        };
        let fg = if primary_critical {
            theme.text_on_accent
        } else {
            theme.text
        };
        c.fill_rounded_rect(pill_rect, tokens::RADIUS_PILL, fill);

        let text_str = if label.is_empty() {
            key.as_str()
        } else {
            label.as_str()
        };
        let pill_style = text::LabelStyle {
            size: 18.0,
            line_height: 22.0,
            weight: 600,
            color: fg,
            ..text::LabelStyle::default()
        };
        let max_label_w = Some((pill_rect.w - 16).max(1));
        if let Some(px) = text::label(text_str, pill_style, max_label_w) {
            let lw = px.width() as i32;
            let lh = px.height() as i32;
            c.blit(
                &Pixels::new(Size::new(lw, lh), px.data().to_vec()),
                Point::new(
                    pill_rect.x + (pill_rect.w - lw) / 2,
                    pill_rect.y + (pill_rect.h - lh) / 2,
                ),
            );
        }
    }

    c.into_pixels()
}

/// Hit-test `p` against a HUN banner positioned at `rect`.
///
/// Action buttons are checked before the card body.
#[must_use]
pub fn hit(rect: Rect, p: Point, m: &HunBannerModel<'_>) -> Option<HunBannerHit> {
    if !rect.contains(p) {
        return None;
    }
    for (idx, btn) in action_rects(rect, m.actions.len()).into_iter().enumerate() {
        if btn.contains(p) {
            return Some(HunBannerHit::Action(idx));
        }
    }
    Some(HunBannerHit::Body)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_model<'a>(actions: &'a [(String, String)], critical: bool) -> HunBannerModel<'a> {
        HunBannerModel {
            id: 42,
            app: "Messages",
            title: "Alice",
            body: "Are you free for lunch?",
            actions,
            critical,
        }
    }

    #[test]
    fn banner_rect_geometry_plain_and_actions() {
        let output = Size::new(720, 1600);
        let top_inset = tokens::STATUS_BAR_HEIGHT;

        let plain = banner_rect(output, top_inset, false);
        assert_eq!(plain.y, top_inset + BANNER_TOP_GAP);
        assert_eq!(plain.h, BANNER_H_PLAIN);
        assert_eq!(plain.w, 720 - BANNER_MARGIN_X * 2);
        assert_eq!(plain.x, BANNER_MARGIN_X);
        assert_eq!(plain.x + plain.w + BANNER_MARGIN_X, output.w);

        let with_actions = banner_rect(output, top_inset, true);
        assert_eq!(with_actions.y, top_inset + BANNER_TOP_GAP);
        assert_eq!(with_actions.h, BANNER_H_ACTIONS);
        assert_eq!(with_actions.w, plain.w);
        assert_eq!(with_actions.x, plain.x);

        // Wide tablet output stays centered and capped at BANNER_MAX_W.
        let tablet = banner_rect(Size::new(1280, 800), 40, false);
        assert_eq!(tablet.w, BANNER_MAX_W);
        assert_eq!(tablet.x, (1280 - BANNER_MAX_W) / 2);
        assert_eq!(tablet.y, 52);
    }

    #[test]
    fn hit_tests_actions_body_and_outside() {
        let actions = vec![
            ("reply".to_string(), "Reply".to_string()),
            ("mark".to_string(), "Mark Read".to_string()),
            ("mute".to_string(), "Mute".to_string()),
            ("extra".to_string(), "Ignored".to_string()),
        ];
        let m = sample_model(&actions, false);
        let rect = banner_rect(Size::new(720, 1600), 56, true);

        // Outside the card returns None.
        assert_eq!(hit(rect, Point::new(rect.x - 1, rect.y + 10), &m), None);
        assert_eq!(
            hit(rect, Point::new(rect.x + 10, rect.y + rect.h + 1), &m),
            None
        );

        // Upper body area hits Body.
        assert_eq!(
            hit(rect, Point::new(rect.x + rect.w / 2, rect.y + 30), &m),
            Some(HunBannerHit::Body)
        );

        // Exactly 3 action pills are active and hit-tested in order.
        let pills = action_rects(rect, m.actions.len());
        assert_eq!(pills.len(), MAX_ACTIONS);
        for (idx, pill) in pills.iter().enumerate() {
            let center = Point::new(pill.x + pill.w / 2, pill.y + pill.h / 2);
            assert_eq!(hit(rect, center, &m), Some(HunBannerHit::Action(idx)));
        }
    }

    #[test]
    fn render_smoke_produces_exact_byte_length() {
        let output = Size::new(720, 1600);
        let plain_rect = banner_rect(output, 56, false);
        let plain_model = sample_model(&[], false);
        let plain_px = render(&tokens::DARK, &plain_model, plain_rect.size());
        assert_eq!(plain_px.size, plain_rect.size());
        assert_eq!(
            plain_px.data.len(),
            (plain_rect.w * plain_rect.h * 4) as usize
        );

        let actions = vec![
            ("accept".to_string(), "Answer".to_string()),
            ("decline".to_string(), "Decline".to_string()),
        ];
        let crit_rect = banner_rect(output, 56, true);
        let crit_model = sample_model(&actions, true);
        let crit_px = render(&tokens::LIGHT, &crit_model, crit_rect.size());
        assert_eq!(crit_px.size, crit_rect.size());
        assert_eq!(crit_px.data.len(), (crit_rect.w * crit_rect.h * 4) as usize);
    }
}
