//! Shade Now-Playing media card (`docs/DE-NOTIFICATIONS-SERVICES.md` §2.5).
//!
//! Pure geometry is shared by [`render_card`] and [`hit_card`] so touch targets
//! always match what is painted.

use de_api::{Point, Rect};
use de_design::tokens::{self, Theme};
use de_paint::{text, Canvas};

use crate::chrome::{self, style};

/// Standard height of the shade media card (matches `NOTIF_CARD_H` in `panels.rs`).
pub const CARD_HEIGHT: i32 = 148;

const PAD_L: i32 = 24;
const PAD_R: i32 = 20;
const BTN_SIZE: i32 = 52;
const PLAY_SIZE: i32 = 60;
const BTN_GAP: i32 = 10;

// Material Icons codepoints.
const GLYPH_PREV: &str = "\u{e045}";
const GLYPH_PLAY: &str = "\u{e037}";
const GLYPH_PAUSE: &str = "\u{e034}";
const GLYPH_NEXT: &str = "\u{e044}";

/// Read-only view model rendered by the shade media card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MediaCardModel<'a> {
    pub player_name: &'a str,
    pub title: &'a str,
    pub artist: &'a str,
    pub playing: bool,
    pub can_go_previous: bool,
    pub can_go_next: bool,
}

/// Interactive regions of the shade media card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaCardHit {
    Previous,
    PlayPause,
    Next,
    Body,
}

#[derive(Clone, Copy, Debug)]
struct TransportLayout {
    prev: Rect,
    play_pause: Rect,
    next: Rect,
}

fn transport_layout(rect: Rect) -> TransportLayout {
    let next_x = rect.x + rect.w - PAD_R - BTN_SIZE;
    let play_x = next_x - BTN_GAP - PLAY_SIZE;
    let prev_x = play_x - BTN_GAP - BTN_SIZE;
    let side_y = rect.y + (rect.h - BTN_SIZE) / 2;
    let play_y = rect.y + (rect.h - PLAY_SIZE) / 2;
    TransportLayout {
        prev: Rect::new(prev_x, side_y, BTN_SIZE, BTN_SIZE),
        play_pause: Rect::new(play_x, play_y, PLAY_SIZE, PLAY_SIZE),
        next: Rect::new(next_x, side_y, BTN_SIZE, BTN_SIZE),
    }
}

fn dim_color(color: [u8; 4]) -> [u8; 4] {
    [
        color[0],
        color[1],
        color[2],
        (u16::from(color[3]) * 2 / 5) as u8,
    ]
}

/// Paint the media control card inside `rect` including its background.
pub fn render_card(canvas: &mut Canvas, rect: Rect, theme: &Theme, m: &MediaCardModel<'_>) {
    canvas.fill_rounded_rect(rect, tokens::RADIUS_CARD, theme.surface1);
    render_card_contents(canvas, rect, theme, m);
}

/// Paint only the card's contents — the caller owns the background (the
/// notification panel frosts it like every other card).
pub fn render_card_contents(
    canvas: &mut Canvas,
    rect: Rect,
    theme: &Theme,
    m: &MediaCardModel<'_>,
) {
    let layout = transport_layout(rect);
    let text_x = rect.x + PAD_L;
    let text_max_w = (layout.prev.x - 16 - text_x).max(0);

    if text_max_w > 0 {
        // Player badge pill at top-left.
        if !m.player_name.is_empty() {
            let badge_max = (text_max_w - 20).max(0);
            if let Some(px) =
                text::label(m.player_name, style::caption(theme.text2), Some(badge_max))
            {
                let bw = px.width() as i32;
                let pill_w = (bw + 20).min(text_max_w);
                canvas.fill_rounded_rect(
                    Rect::new(text_x, rect.y + 16, pill_w, 28),
                    tokens::RADIUS_PILL,
                    theme.surface3,
                );
                chrome::draw(
                    canvas,
                    m.player_name,
                    &style::caption(theme.text2),
                    text_x + 10,
                    rect.y + 18,
                    Some(badge_max),
                );
            }
        }

        // Track title and artist.
        chrome::draw(
            canvas,
            m.title,
            &style::h2(theme.text),
            text_x,
            rect.y + 52,
            Some(text_max_w),
        );
        chrome::draw(
            canvas,
            m.artist,
            &style::body(theme.text2),
            text_x,
            rect.y + 98,
            Some(text_max_w),
        );
    }

    // Transport buttons: prev, play/pause, next.
    let prev_bg = if m.can_go_previous {
        theme.surface3
    } else {
        dim_color(theme.surface3)
    };
    let prev_fg = if m.can_go_previous {
        theme.text
    } else {
        dim_color(theme.text3)
    };
    canvas.fill_rounded_rect(layout.prev, tokens::RADIUS_PILL, prev_bg);
    chrome::draw_ink_centered(
        canvas,
        GLYPH_PREV,
        &text::LabelStyle::icon(28.0, prev_fg),
        layout.prev,
    );

    canvas.fill_rounded_rect(layout.play_pause, tokens::RADIUS_PILL, theme.accent);
    let play_glyph = if m.playing { GLYPH_PAUSE } else { GLYPH_PLAY };
    chrome::draw_ink_centered(
        canvas,
        play_glyph,
        &text::LabelStyle::icon(32.0, theme.text_on_accent),
        layout.play_pause,
    );

    let next_bg = if m.can_go_next {
        theme.surface3
    } else {
        dim_color(theme.surface3)
    };
    let next_fg = if m.can_go_next {
        theme.text
    } else {
        dim_color(theme.text3)
    };
    canvas.fill_rounded_rect(layout.next, tokens::RADIUS_PILL, next_bg);
    chrome::draw_ink_centered(
        canvas,
        GLYPH_NEXT,
        &text::LabelStyle::icon(28.0, next_fg),
        layout.next,
    );
}

/// Hit-test a tap `p` against the media card at `rect`.
///
/// Disabled previous/next buttons return `None` rather than firing an action
/// or falling through to [`MediaCardHit::Body`].
#[must_use]
pub fn hit_card(rect: Rect, p: Point, m: &MediaCardModel<'_>) -> Option<MediaCardHit> {
    if !rect.contains(p) {
        return None;
    }
    let layout = transport_layout(rect);
    if layout.play_pause.contains(p) {
        return Some(MediaCardHit::PlayPause);
    }
    if layout.prev.contains(p) {
        return m.can_go_previous.then_some(MediaCardHit::Previous);
    }
    if layout.next.contains(p) {
        return m.can_go_next.then_some(MediaCardHit::Next);
    }
    Some(MediaCardHit::Body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use de_api::Size;

    const CARD: Rect = Rect {
        x: 28,
        y: 210,
        w: 664,
        h: CARD_HEIGHT,
    };

    fn center(r: Rect) -> Point {
        Point::new(r.x + r.w / 2, r.y + r.h / 2)
    }

    fn sample_model() -> MediaCardModel<'static> {
        MediaCardModel {
            player_name: "Rabbiz Music",
            title: "Midnight Signal",
            artist: "Devded Collective",
            playing: true,
            can_go_previous: true,
            can_go_next: true,
        }
    }

    #[test]
    fn hit_card_maps_buttons_and_body() {
        let m = sample_model();
        let layout = transport_layout(CARD);

        assert_eq!(
            hit_card(CARD, center(layout.prev), &m),
            Some(MediaCardHit::Previous)
        );
        assert_eq!(
            hit_card(CARD, center(layout.play_pause), &m),
            Some(MediaCardHit::PlayPause)
        );
        assert_eq!(
            hit_card(CARD, center(layout.next), &m),
            Some(MediaCardHit::Next)
        );
        assert_eq!(
            hit_card(CARD, Point::new(CARD.x + 60, CARD.y + CARD.h / 2), &m),
            Some(MediaCardHit::Body)
        );
    }

    #[test]
    fn hit_card_respects_disabled_prev_and_next() {
        let mut m = sample_model();
        m.can_go_previous = false;
        m.can_go_next = false;
        let layout = transport_layout(CARD);

        assert_eq!(hit_card(CARD, center(layout.prev), &m), None);
        assert_eq!(hit_card(CARD, center(layout.next), &m), None);
        assert_eq!(
            hit_card(CARD, center(layout.play_pause), &m),
            Some(MediaCardHit::PlayPause)
        );
    }

    #[test]
    fn hit_card_outside_rect_returns_none() {
        let m = sample_model();
        assert_eq!(hit_card(CARD, Point::new(0, 0), &m), None);
        assert_eq!(
            hit_card(CARD, Point::new(CARD.x - 1, CARD.y + 10), &m),
            None
        );
        assert_eq!(
            hit_card(CARD, Point::new(CARD.x + CARD.w, CARD.y + 10), &m),
            None
        );
        assert_eq!(
            hit_card(CARD, Point::new(CARD.x + 10, CARD.y + CARD.h), &m),
            None
        );
    }

    #[test]
    fn render_card_smoke() {
        let theme = &tokens::DEFAULT;
        let m = sample_model();
        let mut c = Canvas::filled(Size::new(720, 400), theme.bg);
        render_card(&mut c, CARD, theme, &m);
        let playing_px = c.into_pixels();

        let mut paused = m;
        paused.playing = false;
        paused.can_go_previous = false;
        let mut c2 = Canvas::filled(Size::new(720, 400), theme.bg);
        render_card(&mut c2, CARD, theme, &paused);
        let paused_px = c2.into_pixels();

        let blank = Canvas::filled(Size::new(720, 400), theme.bg).into_pixels();
        assert_ne!(playing_px.data, blank.data);
        assert_ne!(playing_px.data, paused_px.data);
    }
}
