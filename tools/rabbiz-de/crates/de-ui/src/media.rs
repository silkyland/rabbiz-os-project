//! Now-playing artwork, compact status pill and expanded player.

use de_api::{Pixels, Point, Rect, Size};
use de_design::tokens::{self, Theme};
use de_paint::{text, Canvas};

use crate::chrome;

/// Capsule height inside the 56 px status band.
pub const PILL_H: i32 = 36;
/// Compact island: artwork left, equalizer right — no text, so the pill never
/// outgrows the status band or collides with the clock/indicators.
pub const PILL_W: i32 = 104;
const ART: i32 = 24;
const ART_PAD: i32 = 8;
const EQ_BARS: usize = 5;
const EQ_BAR_W: i32 = 3;
const EQ_GAP: i32 = 3;
const EQ_W: i32 = EQ_BARS as i32 * EQ_BAR_W + (EQ_BARS as i32 - 1) * EQ_GAP;
const EQ_PAD: i32 = 15;

const FILL: [u8; 4] = [0x0E, 0x0A, 0x12, 0xFF];
const RIM: [u8; 4] = [0xFF, 0xFF, 0xFF, 0x22];
const INK_DIM: [u8; 4] = [0xFF, 0xF8, 0xF0, 0xA8];

// Material Icons codepoints.
const GLYPH_NOTE: &str = "\u{e405}";

/// What the pill shows.
#[derive(Debug, Clone, Copy)]
pub struct PillModel<'a> {
    pub playing: bool,
    /// Decoded album art (any size; scaled to the badge).
    pub art: Option<&'a Pixels>,
}

/// Where a pill `width` px wide sits: horizontally centred in the status band.
#[must_use]
pub fn pill_rect(output: Size, width: i32) -> Rect {
    Rect::new(
        (output.w - width) / 2,
        (tokens::STATUS_BAR_HEIGHT - PILL_H) / 2,
        width,
        PILL_H,
    )
}

/// Equalizer bar heights (px, 5 bars) at `t_ms`; `max_h` is the tallest bar.
#[must_use]
pub fn eq_heights(t_ms: u64, max_h: i32) -> [i32; EQ_BARS] {
    let t = t_ms as f32 / 1000.0;
    let bar = |speed: f32, off: f32| {
        let v = 0.5 + 0.5 * (t * speed + off).sin();
        (4.0 + (max_h - 4) as f32 * v).round() as i32
    };
    [
        bar(7.1, 0.0),
        bar(9.3, 1.7),
        bar(5.7, 2.4),
        bar(8.4, 3.8),
        bar(6.6, 5.1),
    ]
}

/// Crop-to-fill `art` into a `side`×`side` tile with `radius` corners.
#[must_use]
pub fn rounded_art(art: &Pixels, side: i32, radius: f32) -> Pixels {
    let mut tile = Canvas::new(Size::new(side, side));
    let (aw, ah) = (art.size.w.max(1), art.size.h.max(1));
    // Cover: scale the short edge to `side`, centre the long one.
    let scale = side as f32 / aw.min(ah) as f32;
    let (dw, dh) = (
        (aw as f32 * scale).round() as i32,
        (ah as f32 * scale).round() as i32,
    );
    tile.blit_scaled(
        art,
        Rect::new((side - dw) / 2, (side - dh) / 2, dw, dh),
        1.0,
    );
    let mut px = tile.into_pixels();
    let mut mask = Canvas::new(Size::new(side, side));
    mask.fill_rounded_rect(Rect::new(0, 0, side, side), radius, [0xFF; 4]);
    let mask = mask.into_pixels();
    for (d, m) in px.data.chunks_exact_mut(4).zip(mask.data.chunks_exact(4)) {
        let a = u16::from(m[3]);
        for ch in d.iter_mut() {
            *ch = ((u16::from(*ch) * a + 127) / 255) as u8;
        }
    }
    px
}

/// Artwork tile or, without art, a tinted tile with a note glyph.
#[must_use]
pub fn art_tile(theme: &Theme, art: Option<&Pixels>, side: i32, radius: f32) -> Pixels {
    if let Some(a) = art {
        return rounded_art(a, side, radius);
    }
    let mut c = Canvas::new(Size::new(side, side));
    let r = Rect::new(0, 0, side, side);
    c.fill_rounded_rect(r, radius, theme.accent_dim);
    chrome::draw_ink_centered(
        &mut c,
        GLYPH_NOTE,
        &text::LabelStyle::icon(side as f32 * 0.5, theme.text_on_accent),
        r,
    );
    c.into_pixels()
}

// --- expanded island card --------------------------------------------------

/// Height of the expanded media card.
pub const CARD_H: i32 = 196;
const CARD_SIDE: i32 = 28;
const CARD_MAX_W: i32 = 576;
const CARD_ART: i32 = 64;
const CARD_INSET: i32 = 24;
const CARD_ART_RADIUS: i32 = 12;
const TRANSP_Y: i32 = 124;
const TRANSP_H: i32 = 56;

const INK: [u8; 4] = [0xFF, 0xF8, 0xF0, 0xFF];
const TRACK: [u8; 4] = [0xFF, 0xFF, 0xFF, 0x30];
const TRANSPORT_DIM: [u8; 4] = [0xFF, 0xF8, 0xF0, 0x62];

const GLYPH_PREV: &str = "\u{e045}";
const GLYPH_PLAY: &str = "\u{e037}";
const GLYPH_PAUSE: &str = "\u{e034}";
const GLYPH_NEXT: &str = "\u{e044}";

/// The expanded player shares the top edge of the status pill.
#[must_use]
pub fn card_rect(output: Size) -> Rect {
    let width = (output.w - CARD_SIDE * 2).clamp(1, CARD_MAX_W);
    Rect::new(
        (output.w - width) / 2,
        pill_rect(output, PILL_W).y,
        width,
        CARD_H,
    )
}

fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

/// Interpolated pill→card rect for `open` in 0..=1 (eased).
#[must_use]
pub fn morph_rect(output: Size, open: f32) -> Rect {
    let t = ease_out_cubic(open.clamp(0.0, 1.0));
    let (a, b) = (pill_rect(output, PILL_W), card_rect(output));
    let l = |x: i32, y: i32| (x as f32 + (y - x) as f32 * t).round() as i32;
    Rect::new(l(a.x, b.x), l(a.y, b.y), l(a.w, b.w), l(a.h, b.h))
}

/// What the expanded card shows.
#[derive(Debug, Clone, Copy)]
pub struct CardModel<'a> {
    pub title: &'a str,
    pub artist: &'a str,
    pub playing: bool,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub can_go_previous: bool,
    pub can_go_next: bool,
}

/// Transport tap target inside the expanded card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardHit {
    Previous,
    PlayPause,
    Next,
}

fn transport_rects(r: Rect) -> (Rect, Rect, Rect) {
    let side = 48;
    let gap = 24;
    let center = r.x + (r.w - TRANSP_H) / 2;
    let side_y = r.y + TRANSP_Y + (TRANSP_H - side) / 2;
    let y = r.y + TRANSP_Y;
    (
        Rect::new(center - gap - side, side_y, side, side),
        Rect::new(center, y, TRANSP_H, TRANSP_H),
        Rect::new(center + TRANSP_H + gap, side_y, side, side),
    )
}

/// Hit-test transport buttons inside `r` (the card rect).
#[must_use]
pub fn hit_card(r: Rect, pos: Point) -> Option<CardHit> {
    let (prev, play, next) = transport_rects(r);
    if prev.contains(pos) {
        Some(CardHit::Previous)
    } else if play.contains(pos) {
        Some(CardHit::PlayPause)
    } else {
        next.contains(pos).then_some(CardHit::Next)
    }
}

fn mmss(ms: u64) -> String {
    let s = ms / 1000;
    format!("{}:{:02}", s / 60, s % 60)
}

/// Render the expanded media card sized `r` (typically [`card_rect`]).
#[must_use]
pub fn render_card(theme: &Theme, r: Rect, m: &CardModel<'_>, art: Option<&Pixels>) -> Pixels {
    let mut c = Canvas::new(Size::new(r.w, r.h));
    let body = Rect::new(0, 0, r.w, r.h);
    draw_body(&mut c, body, 32.0);
    draw_card_controls(&mut c, theme, body, m);
    c.blit(
        &art_tile(theme, art, CARD_ART, CARD_ART_RADIUS as f32),
        Point::new(CARD_INSET, CARD_INSET),
    );
    c.into_pixels()
}

fn draw_body(c: &mut Canvas, body: Rect, radius: f32) {
    c.fill_rounded_rect(body, radius, RIM);
    c.fill_rounded_rect(Rect::new(1, 1, body.w - 2, body.h - 2), radius - 1.0, FILL);
}

fn draw_card_controls(c: &mut Canvas, theme: &Theme, r: Rect, m: &CardModel<'_>) {
    draw_card_title(c, r, m);
    draw_card_transport(c, theme, r, m);
}

fn draw_card_title(c: &mut Canvas, r: Rect, m: &CardModel<'_>) {
    let tx = CARD_INSET + CARD_ART + 16;
    let tw = (r.w - tx - CARD_INSET).max(1);
    let header_y = CARD_INSET + if m.artist.is_empty() { 16 } else { 2 };
    let title_style = text::LabelStyle {
        size: 26.0,
        line_height: 32.0,
        weight: 600,
        color: INK,
        ..text::LabelStyle::default()
    };
    if let Some(px) = text::label_clamped(m.title, title_style, tw, 1) {
        let h = px.height() as i32;
        c.blit(
            &Pixels::new(de_api::Size::new(px.width() as i32, h), px.data().to_vec()),
            Point::new(tx, header_y),
        );
    }
    if let Some(px) = text::label_clamped(m.artist, style_artist(), tw, 1) {
        c.blit(
            &Pixels::new(
                Size::new(px.width() as i32, px.height() as i32),
                px.data().to_vec(),
            ),
            Point::new(tx, CARD_INSET + 34),
        );
    }
}

fn draw_card_transport(c: &mut Canvas, theme: &Theme, r: Rect, m: &CardModel<'_>) {
    // Time labels bookend the track on a common centerline.
    let time_y = 98;
    let left = chrome::draw(
        c,
        &mmss(m.position_ms),
        &style_time(),
        CARD_INSET,
        time_y,
        None,
    );
    let duration = if m.duration_ms > 0 {
        mmss(m.duration_ms)
    } else {
        "–:––".into()
    };
    let mut right = r.w - CARD_INSET;
    if let Some(px) = text::label(&duration, style_time(), None) {
        let size = Size::new(px.width() as i32, px.height() as i32);
        right -= size.w;
        c.blit(
            &Pixels::new(size, px.data().to_vec()),
            Point::new(right, time_y),
        );
    }
    let bar_x = left.x + left.w + 14;
    let bar = Rect::new(bar_x, time_y + 7, (right - 14 - bar_x).max(0), 5);
    c.fill_rounded_rect(bar, 2.5, TRACK);
    if m.duration_ms > 0 {
        let fraction = (m.position_ms as f32 / m.duration_ms as f32).clamp(0.0, 1.0);
        let fill = (bar.w as f32 * fraction).round() as i32;
        if fill > 0 {
            c.fill_rounded_rect(Rect::new(bar.x, bar.y, fill, bar.h), 2.5, INK);
        }
    }

    let (prev, play, next) = transport_rects(Rect::new(0, 0, r.w, r.h));
    for (button, glyph, enabled) in [
        (prev, GLYPH_PREV, m.can_go_previous),
        (next, GLYPH_NEXT, m.can_go_next),
    ] {
        let ink = if enabled { INK } else { TRANSPORT_DIM };
        chrome::draw_ink_centered(c, glyph, &text::LabelStyle::icon(32.0, ink), button);
    }
    let play_glyph = if m.playing { GLYPH_PAUSE } else { GLYPH_PLAY };
    c.fill_rounded_rect(play, tokens::RADIUS_PILL, theme.accent);
    chrome::draw_ink_centered(
        c,
        play_glyph,
        &text::LabelStyle::icon(36.0, theme.text_on_accent),
        play,
    );
}

fn style_artist() -> text::LabelStyle {
    text::LabelStyle {
        size: 20.0,
        line_height: 24.0,
        weight: 400,
        color: INK_DIM,
        ..text::LabelStyle::default()
    }
}

fn style_time() -> text::LabelStyle {
    text::LabelStyle {
        size: 15.0,
        line_height: 18.0,
        weight: 500,
        tnum: true,
        color: INK_DIM,
        ..text::LabelStyle::default()
    }
}

/// Render the pill at time `t_ms` (drives the equalizer). Fixed [`PILL_W`]
/// wide — artwork on the left, equalizer (or pause bars) on the right.
#[must_use]
pub fn render_pill(theme: &Theme, _output: Size, m: &PillModel<'_>, t_ms: u64) -> Pixels {
    let mut c = Canvas::new(Size::new(PILL_W, PILL_H));
    let body = Rect::new(0, 0, PILL_W, PILL_H);
    draw_body(&mut c, body, PILL_H as f32 / 2.0);
    c.blit(
        &art_tile(theme, m.art, ART, 7.0),
        Point::new(ART_PAD, (PILL_H - ART) / 2),
    );
    draw_equalizer(&mut c, theme, m.playing, t_ms);
    c.into_pixels()
}

/// Draw the morph at its actual size, keeping artwork square as it grows.
pub fn render_morph(
    theme: &Theme,
    r: Rect,
    model: &CardModel<'_>,
    art: Option<&Pixels>,
    open: f32,
    t_ms: u64,
) -> Pixels {
    let t = ease_out_cubic(open);
    let lerp = |a: i32, b: i32| (a as f32 + (b - a) as f32 * t).round() as i32;
    let mut c = Canvas::new(Size::new(r.w, r.h));
    let body = Rect::new(0, 0, r.w, r.h);
    draw_body(&mut c, body, lerp(PILL_H / 2, 32) as f32);
    let side = lerp(ART, CARD_ART);
    c.blit(
        &art_tile(theme, art, side, lerp(7, CARD_ART_RADIUS) as f32),
        Point::new(
            lerp(ART_PAD, CARD_INSET),
            lerp((PILL_H - ART) / 2, CARD_INSET),
        ),
    );
    if t < 0.4 {
        let mut eq = Canvas::new(Size::new(PILL_W, PILL_H));
        draw_equalizer(&mut eq, theme, model.playing, t_ms);
        c.blit_scaled(
            &eq.into_pixels(),
            Rect::new(r.w - PILL_W, (r.h - PILL_H) / 2, PILL_W, PILL_H),
            1.0 - t / 0.4,
        );
    }
    // The header appears as the artwork approaches its final position.
    if t > 0.6 {
        let mut content = Canvas::new(Size::new(r.w, r.h));
        draw_card_title(&mut content, body, model);
        c.blit_scaled(&content.into_pixels(), body, ((t - 0.6) / 0.3).min(1.0));
    }
    if r.h >= TRANSP_Y + TRANSP_H {
        let reveal = (r.h - TRANSP_Y - TRANSP_H) as f32 / (CARD_H - TRANSP_Y - TRANSP_H) as f32;
        let mut controls = Canvas::new(Size::new(r.w, r.h));
        draw_card_transport(&mut controls, theme, body, model);
        c.blit_scaled(&controls.into_pixels(), body, reveal);
    }
    c.into_pixels()
}

fn draw_equalizer(c: &mut Canvas, theme: &Theme, playing: bool, t_ms: u64) {
    let eq_x = PILL_W - EQ_PAD - EQ_W;
    let base_y = PILL_H / 2 + 10;
    if playing {
        for (i, h) in eq_heights(t_ms, 20).into_iter().enumerate() {
            c.fill_rounded_rect(
                Rect::new(
                    eq_x + i as i32 * (EQ_BAR_W + EQ_GAP),
                    base_y - h,
                    EQ_BAR_W,
                    h,
                ),
                2.0,
                theme.accent_hi,
            );
        }
    } else {
        for i in 0..2 {
            c.fill_rounded_rect(
                Rect::new(eq_x + EQ_W / 2 - 8 + i * 11, PILL_H / 2 - 10, 6, 20),
                2.0,
                INK_DIM,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OUT: Size = Size::new(720, 1600);

    #[test]
    fn eq_bars_stay_in_range_and_move() {
        let a = eq_heights(0, 20);
        let b = eq_heights(333, 20);
        assert_ne!(a, b);
        for h in a.into_iter().chain(b) {
            assert!((4..=20).contains(&h));
        }
    }

    #[test]
    fn pill_is_compact_and_animates_only_while_playing() {
        let theme = &tokens::DARK;
        let playing = PillModel {
            playing: true,
            art: None,
        };
        let paused = PillModel {
            playing: false,
            art: None,
        };
        let p = render_pill(theme, OUT, &playing, 0);
        assert_eq!(p.size, Size::new(PILL_W, PILL_H));
        // Paused: static (no re-raster churn); playing: the eq moves.
        assert_eq!(
            render_pill(theme, OUT, &paused, 0).data,
            render_pill(theme, OUT, &paused, 2_400).data
        );
        assert_ne!(
            render_pill(theme, OUT, &playing, 0).data,
            render_pill(theme, OUT, &playing, 2_400).data
        );
    }

    #[test]
    fn pill_is_centred_in_status_band() {
        let r = pill_rect(OUT, 300);
        assert_eq!(r.x * 2 + r.w, OUT.w);
        assert_eq!(r.y * 2 + r.h, tokens::STATUS_BAR_HEIGHT);
    }

    #[test]
    fn rounded_art_clears_corners() {
        let art = Pixels::new(Size::new(4, 2), vec![0xFF; 4 * 2 * 4]);
        let t = rounded_art(&art, 32, 16.0);
        assert_eq!(t.size, Size::new(32, 32));
        assert_eq!(&t.data[0..4], &[0, 0, 0, 0], "corner masked");
        let mid = ((16 * 32 + 16) * 4) as usize;
        assert_eq!(t.data[mid + 3], 0xFF, "centre opaque");
    }

    #[test]
    fn morph_rect_endpoints_and_monotonic() {
        let out = Size::new(720, 1600);
        let start = morph_rect(out, 0.0);
        let end = morph_rect(out, 1.0);
        assert_eq!(start, pill_rect(out, PILL_W));
        assert_eq!(end, card_rect(out));
        let mid = morph_rect(out, 0.5);
        assert!(mid.w > start.w && mid.w < end.w);
        assert!(mid.h > start.h && mid.h < end.h);
        assert_eq!(mid.y, start.y);
    }

    #[test]
    fn morph_card_hit_test_transport() {
        let card = card_rect(Size::new(720, 1600));
        let cx = card.x + card.w / 2;
        let cy = card.y + TRANSP_Y + TRANSP_H / 2;
        assert_eq!(hit_card(card, Point::new(cx, cy)), Some(CardHit::PlayPause));
        assert_eq!(
            hit_card(card, Point::new(cx - TRANSP_H / 2 - 24 - 24, cy)),
            Some(CardHit::Previous)
        );
        assert_eq!(
            hit_card(card, Point::new(cx + TRANSP_H / 2 + 24 + 24, cy)),
            Some(CardHit::Next)
        );
        // Artwork area and above the transport row are not transport hits.
        assert_eq!(hit_card(card, Point::new(card.x + 10, card.y + 10)), None);
        // Outside the card entirely.
        assert_eq!(hit_card(card, Point::new(0, card.y - 20)), None);
    }

    #[test]
    fn render_card_paints_something() {
        let theme = de_design::tokens::DARK;
        let card = card_rect(Size::new(720, 1600));
        let m = CardModel {
            title: "Midnight Signal",
            artist: "Devded",
            playing: true,
            position_ms: 19_000,
            duration_ms: 252_000,
            can_go_previous: true,
            can_go_next: true,
        };
        let px = render_card(&theme, Rect::new(0, 0, card.w, card.h), &m, None);
        assert_eq!(px.size.w, card.w);
        assert_eq!(px.size.h, card.h);
        // Centre of the card is opaque dark fill.
        let mid = ((card.h / 2 * card.w + card.w / 2) * 4) as usize;
        assert_eq!(px.data[mid + 3], 0xFF);

        // During the morph the backdrop must stay opaque, just like the pill
        // and card endpoints, so status text never shows through the player.
        let mid_rect = morph_rect(OUT, 0.4);
        let frame = render_morph(&theme, mid_rect, &m, None, 0.4, 0);
        assert_eq!(frame.size, Size::new(mid_rect.w, mid_rect.h));
        let centre = ((mid_rect.h / 2 * mid_rect.w + mid_rect.w / 2) * 4) as usize;
        assert_eq!(frame.data[centre + 3], 0xFF);
    }
}
