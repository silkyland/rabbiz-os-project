//! One notification, painted the same way in the shade list and in the
//! heads-up banner: app badge, "app · time" header, bold title, two-line
//! body. Pure painter — the caller owns the surface, the geometry and hits.

use de_api::{Pixels, Point, Rect, Size};
use de_design::tokens::Theme;
use de_paint::{text, Canvas};

use crate::chrome::{self, style};

/// Content height with a two-line body (padding included).
pub const CONTENT_H: i32 = 168;
/// Padding around the content.
pub const PAD: i32 = 24;
/// App badge diameter.
pub const BADGE: i32 = 64;
const TEXT_GAP: i32 = 20;

/// Colours the painter needs; the surface behind decides light vs dark ink.
#[derive(Debug, Clone, Copy)]
pub struct Ink {
    pub title: [u8; 4],
    pub body: [u8; 4],
    pub meta: [u8; 4],
    pub badge_fill: [u8; 4],
    pub badge_ink: [u8; 4],
    pub accent: [u8; 4],
}

impl Ink {
    /// Ink for a dark surface (heads-up banner, dark theme).
    #[must_use]
    pub fn on(theme: &Theme) -> Self {
        Self {
            title: theme.text,
            body: theme.text2,
            meta: theme.text3,
            badge_fill: [theme.accent[0], theme.accent[1], theme.accent[2], 0x38],
            badge_ink: theme.accent_hi,
            accent: theme.accent_hi,
        }
    }
}

/// What to paint.
#[derive(Debug, Clone, Copy)]
pub struct CardContent<'a> {
    pub app: &'a str,
    pub title: &'a str,
    pub body: &'a str,
    /// "now", "26 min", "13:21".
    pub when: &'a str,
    pub icon: Option<&'a Pixels>,
}

/// Left edge of the text column for a card starting at `x`.
#[must_use]
pub const fn text_x(x: i32) -> i32 {
    x + PAD + BADGE + TEXT_GAP
}

/// Paint `c` into `rect` (no background).
pub fn paint(canvas: &mut Canvas, rect: Rect, ink: &Ink, c: &CardContent<'_>) {
    let badge = Rect::new(rect.x + PAD, rect.y + PAD, BADGE, BADGE);
    if let Some(px) = c.icon {
        canvas.blit_scaled(px, badge, 1.0);
    } else {
        canvas.fill_rounded_rect(badge, 999.0, ink.badge_fill);
        let initial: String = c.app.chars().next().map_or_else(String::new, |ch| ch.to_uppercase().collect());
        chrome::draw_ink_centered(
            canvas,
            &initial,
            &text::LabelStyle {
                size: 32.0,
                line_height: 40.0,
                weight: 700,
                color: ink.badge_ink,
                ..text::LabelStyle::default()
            },
            badge,
        );
    }

    let tx = text_x(rect.x);
    let tw = (rect.x + rect.w - PAD - tx).max(1);
    let header = if c.when.is_empty() {
        c.app.to_string()
    } else {
        format!("{} \u{b7} {}", c.app, c.when)
    };
    chrome::draw(
        canvas,
        &header,
        &text::LabelStyle {
            size: 20.0,
            line_height: 26.0,
            weight: 600,
            color: ink.meta,
            ..text::LabelStyle::default()
        },
        tx,
        rect.y + PAD - 4,
        Some(tw),
    );
    let y = rect.y + 46;
    if let Some(px) = text::label_clamped(
        c.title,
        text::LabelStyle {
            size: 28.0,
            line_height: 36.0,
            weight: 600,
            color: ink.title,
            ..text::LabelStyle::default()
        },
        tw,
        1,
    ) {
        let (w, h) = (px.width() as i32, px.height() as i32);
        canvas.blit(&Pixels::new(Size::new(w, h), px.data().to_vec()), Point::new(tx, y));
    }
    if !c.body.is_empty() {
        if let Some(px) = text::label_clamped(c.body, style::label(ink.body), tw, 2) {
            let (w, h) = (px.width() as i32, px.height() as i32);
            canvas.blit(&Pixels::new(Size::new(w, h), px.data().to_vec()), Point::new(tx, y + 40));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use de_design::tokens;

    #[test]
    fn paints_inside_rect_and_differs_by_content() {
        let theme = &tokens::DARK;
        let rect = Rect::new(10, 10, 600, CONTENT_H);
        let ink = Ink::on(theme);
        let mut a = Canvas::new(Size::new(640, 200));
        paint(
            &mut a,
            rect,
            &ink,
            &CardContent {
                app: "Messages",
                title: "Alice",
                body: "Are you free for lunch? I found a new place near the station.",
                when: "now",
                icon: None,
            },
        );
        let mut b = Canvas::new(Size::new(640, 200));
        paint(
            &mut b,
            rect,
            &ink,
            &CardContent {
                app: "Mail",
                title: "Bob",
                body: "",
                when: "",
                icon: None,
            },
        );
        let (a, b) = (a.into_pixels(), b.into_pixels());
        assert_ne!(a.data, b.data);
        // Nothing painted left of the rect.
        assert!(a.data[..(9 * 4)].iter().all(|&v| v == 0));
    }
}
