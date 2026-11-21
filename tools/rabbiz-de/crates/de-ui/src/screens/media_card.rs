//! Shade Now-Playing media card (`docs/DE-NOTIFICATIONS-SERVICES.md` §2.5).
//!
//! Artwork and track on top, transport centred below. Pure geometry is shared
//! by [`render_card`]/[`render_card_contents`] and [`hit_card`] so touch
//! targets always match what is painted. The card is size-agnostic: it fills
//! whatever width the layout gives it (portrait full width, landscape column).

use de_api::{Pixels, Point, Rect};
use de_design::tokens::{self, Theme};
use de_paint::{text, Canvas};

use crate::chrome::{self, style};
use crate::media::art_tile;

/// Height of the shade media card.
pub const CARD_HEIGHT: i32 = 260;

const PAD: i32 = 24;
const ART: i32 = 128;
const BTN: i32 = 64;
const PLAY_W: i32 = 96;
const BTN_GAP: i32 = 20;

// Material Icons codepoints.
const GLYPH_PREV: &str = "\u{e045}";
const GLYPH_PLAY: &str = "\u{e037}";
const GLYPH_PAUSE: &str = "\u{e034}";
const GLYPH_NEXT: &str = "\u{e044}";

/// Text on the dimmed, frosted shade backdrop.
const INK: [u8; 4] = [0xFF, 0xF8, 0xF0, 0xFF];
const INK_DIM: [u8; 4] = [0xFF, 0xF8, 0xF0, 0xC0];
const INK_FAINT: [u8; 4] = [0xFF, 0xF8, 0xF0, 0x80];
const BTN_FILL: [u8; 4] = [0xFF, 0xF8, 0xF0, 0x24];

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
struct Layout {
    art: Rect,
    text_x: i32,
    text_w: i32,
    prev: Rect,
    play_pause: Rect,
    next: Rect,
}

fn layout(rect: Rect) -> Layout {
    let art = Rect::new(rect.x + PAD, rect.y + PAD, ART, ART);
    let text_x = art.x + ART + 20;
    let row_y = art.y + ART + 20;
    let total = BTN + BTN_GAP + PLAY_W + BTN_GAP + BTN;
    let x0 = rect.x + (rect.w - total) / 2;
    Layout {
        art,
        text_x,
        text_w: (rect.x + rect.w - PAD - text_x).max(0),
        prev: Rect::new(x0, row_y, BTN, BTN),
        play_pause: Rect::new(x0 + BTN + BTN_GAP, row_y, PLAY_W, BTN),
        next: Rect::new(x0 + BTN + BTN_GAP + PLAY_W + BTN_GAP, row_y, BTN, BTN),
    }
}

fn dim(color: [u8; 4]) -> [u8; 4] {
    [color[0], color[1], color[2], (u16::from(color[3]) * 2 / 5) as u8]
}

/// Paint the card contents (no background) inside `rect`; the caller lays
/// down the frosted surface first.
pub fn render_card_contents(
    canvas: &mut Canvas,
    rect: Rect,
    theme: &Theme,
    m: &MediaCardModel<'_>,
    art: Option<&Pixels>,
) {
    let l = layout(rect);
    canvas.blit(&art_tile(theme, art, ART, 28.0), l.art.origin());

    if l.text_w > 0 {
        chrome::draw(
            canvas,
            m.player_name,
            &style::caption(INK_FAINT),
            l.text_x,
            l.art.y,
            Some(l.text_w),
        );
        let title_style = text::LabelStyle {
            size: 32.0,
            line_height: 40.0,
            weight: 600,
            color: INK,
            ..text::LabelStyle::default()
        };
        if let Some(px) = text::label_clamped(m.title, title_style, l.text_w, 1) {
            let h = px.height() as i32;
            canvas.blit(
                &Pixels::new(
                    de_api::Size::new(px.width() as i32, h),
                    px.data().to_vec(),
                ),
                Point::new(l.text_x, l.art.y + 34),
            );
            chrome::draw(
                canvas,
                m.artist,
                &style::label(INK_DIM),
                l.text_x,
                l.art.y + 34 + h + 10,
                Some(l.text_w),
            );
        }
    }

    let ctl = |canvas: &mut Canvas, r: Rect, glyph: &str, enabled: bool| {
        let (fill, ink) = if enabled {
            (BTN_FILL, INK)
        } else {
            (dim(BTN_FILL), dim(INK))
        };
        canvas.fill_rounded_rect(r, tokens::RADIUS_PILL, fill);
        chrome::draw_ink_centered(canvas, glyph, &text::LabelStyle::icon(36.0, ink), r);
    };
    ctl(canvas, l.prev, GLYPH_PREV, m.can_go_previous);
    ctl(canvas, l.next, GLYPH_NEXT, m.can_go_next);
    canvas.fill_rounded_rect(l.play_pause, tokens::RADIUS_PILL, theme.accent);
    chrome::draw_ink_centered(
        canvas,
        if m.playing { GLYPH_PAUSE } else { GLYPH_PLAY },
        &text::LabelStyle::icon(42.0, theme.text_on_accent),
        l.play_pause,
    );
}

/// Opaque variant (own surface) for contexts without a frosted backdrop.
pub fn render_card(canvas: &mut Canvas, rect: Rect, theme: &Theme, m: &MediaCardModel<'_>) {
    canvas.fill_rounded_rect(rect, tokens::RADIUS_SHEET, [0x24, 0x1A, 0x2A, 0xFF]);
    render_card_contents(canvas, rect, theme, m, None);
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
    let l = layout(rect);
    if l.play_pause.contains(p) {
        return Some(MediaCardHit::PlayPause);
    }
    if l.prev.contains(p) {
        return m.can_go_previous.then_some(MediaCardHit::Previous);
    }
    if l.next.contains(p) {
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
    fn transport_fits_inside_card_at_any_width() {
        for w in [520, 664, 880] {
            let rect = Rect::new(0, 0, w, CARD_HEIGHT);
            let l = layout(rect);
            for r in [l.prev, l.play_pause, l.next, l.art] {
                assert!(rect.contains(r.origin()) && r.x + r.w <= rect.w && r.y + r.h <= rect.h);
            }
            assert_eq!(l.prev.x - rect.x, rect.x + rect.w - (l.next.x + l.next.w));
        }
    }

    #[test]
    fn hit_card_maps_buttons_and_body() {
        let m = sample_model();
        let l = layout(CARD);
        assert_eq!(hit_card(CARD, center(l.prev), &m), Some(MediaCardHit::Previous));
        assert_eq!(hit_card(CARD, center(l.play_pause), &m), Some(MediaCardHit::PlayPause));
        assert_eq!(hit_card(CARD, center(l.next), &m), Some(MediaCardHit::Next));
        assert_eq!(
            hit_card(CARD, Point::new(CARD.x + 60, CARD.y + 60), &m),
            Some(MediaCardHit::Body)
        );
    }

    #[test]
    fn hit_card_respects_disabled_prev_and_next() {
        let mut m = sample_model();
        m.can_go_previous = false;
        m.can_go_next = false;
        let l = layout(CARD);
        assert_eq!(hit_card(CARD, center(l.prev), &m), None);
        assert_eq!(hit_card(CARD, center(l.next), &m), None);
        assert_eq!(hit_card(CARD, center(l.play_pause), &m), Some(MediaCardHit::PlayPause));
    }

    #[test]
    fn hit_card_outside_rect_returns_none() {
        let m = sample_model();
        assert_eq!(hit_card(CARD, Point::new(0, 0), &m), None);
        assert_eq!(hit_card(CARD, Point::new(CARD.x - 1, CARD.y + 10), &m), None);
        assert_eq!(hit_card(CARD, Point::new(CARD.x + CARD.w, CARD.y + 10), &m), None);
        assert_eq!(hit_card(CARD, Point::new(CARD.x + 10, CARD.y + CARD.h), &m), None);
    }

    #[test]
    fn render_card_smoke() {
        let theme = &tokens::DEFAULT;
        let m = sample_model();
        let mut c = Canvas::filled(Size::new(720, 520), theme.bg);
        render_card(&mut c, CARD, theme, &m);
        let playing_px = c.into_pixels();
        let mut paused = m;
        paused.playing = false;
        paused.can_go_previous = false;
        let mut c2 = Canvas::filled(Size::new(720, 520), theme.bg);
        render_card(&mut c2, CARD, theme, &paused);
        let blank = Canvas::filled(Size::new(720, 520), theme.bg).into_pixels();
        assert_ne!(playing_px.data, blank.data);
        assert_ne!(playing_px.data, c2.into_pixels().data);
    }
}
