//! Now-playing building blocks shared by the status-bar pill and the shade
//! card: rounded artwork, equalizer, marquee, and the pill itself.
//!
//! The pill is a self-contained capsule texture (artwork, scrolling
//! "title · artist", equalizer while playing / pause glyph when paused). The
//! compositor re-rasters only this small texture while it animates, never the
//! whole shell frame. Pure data in, pixels out; width adapts to the output so
//! it works in portrait and landscape.

use de_api::{Pixels, Point, Rect, Size};
use de_design::tokens::{self, Theme};
use de_paint::{text, Canvas};

use crate::chrome;

/// Capsule height inside the 56 px status band.
pub const PILL_H: i32 = 40;
/// Compact island: artwork left, equalizer right — no text, so the pill never
/// outgrows the status band or collides with the clock/indicators.
pub const PILL_W: i32 = 148;
const ART: i32 = 30;
const ART_PAD: i32 = 7;
const EQ_BARS: usize = 5;
const EQ_BAR_W: i32 = 5;
const EQ_GAP: i32 = 5;
const EQ_W: i32 = EQ_BARS as i32 * EQ_BAR_W + (EQ_BARS as i32 - 1) * EQ_GAP;
const EQ_PAD: i32 = 14;

const FILL: [u8; 4] = [0x0E, 0x0A, 0x12, 0xF2];
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
    let (dw, dh) = ((aw as f32 * scale).round() as i32, (ah as f32 * scale).round() as i32);
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
pub const CARD_H: i32 = 244;
const CARD_SIDE: i32 = 28;
const CARD_ART: i32 = 64;
const TRANSP_Y: i32 = 168;
const TRANSP_H: i32 = 56;

const INK: [u8; 4] = [0xFF, 0xF8, 0xF0, 0xFF];
const TRACK: [u8; 4] = [0xFF, 0xFF, 0xFF, 0x30];
const SURF: [u8; 4] = [0xFF, 0xFF, 0xFF, 0x18];

const GLYPH_PREV: &str = "\u{e045}";
const GLYPH_PLAY: &str = "\u{e037}";
const GLYPH_PAUSE: &str = "\u{e034}";
const GLYPH_NEXT: &str = "\u{e044}";

/// The expanded card: wide dark sheet directly under the status band.
#[must_use]
pub fn card_rect(output: Size) -> Rect {
    Rect::new(
        CARD_SIDE,
        tokens::STATUS_BAR_HEIGHT + 4,
        output.w - CARD_SIDE * 2,
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
    let (pw, nw) = (48, 48);
    let total = nw + 24 + TRANSP_H + 24 + nw;
    let x0 = r.x + (r.w - total) / 2;
    let y = r.y + TRANSP_Y;
    (
        Rect::new(x0, y + (TRANSP_H - pw) / 2, pw, pw),
        Rect::new(x0 + nw + 24, y, TRANSP_H, TRANSP_H),
        Rect::new(x0 + nw + 24 + TRANSP_H + 24, y + (TRANSP_H - nw) / 2, nw, nw),
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
    c.fill_rounded_rect(Rect::new(0, 0, r.w, r.h), 44.0, RIM);
    c.fill_rounded_rect(Rect::new(1, 1, r.w - 2, r.h - 2), 44.0, FILL);

    c.blit(
        &art_tile(theme, art, CARD_ART, 16.0),
        Point::new(20, 20),
    );
    let tx = 20 + CARD_ART + 18;
    let tw = (r.w - tx - 20).max(0);
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
            Point::new(tx, 18),
        );
        chrome::draw(
            &mut c,
            m.artist,
            &style_artist(),
            tx,
            18 + h + 6,
            Some(tw),
        );
    }

    // Progress: track + fill + mm:ss endpoints.
    let bar_y = 118;
    let bar = Rect::new(20, bar_y, r.w - 40, 6);
    c.fill_rounded_rect(bar, 3.0, TRACK);
    let frac = if m.duration_ms > 0 {
        (m.position_ms as f32 / m.duration_ms as f32).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let fw = (bar.w as f32 * frac).round() as i32;
    if fw > 0 {
        c.fill_rounded_rect(Rect::new(bar.x, bar.y, fw, bar.h), 3.0, INK);
    }
    let pos_s = mmss(m.position_ms.min(m.duration_ms.max(m.position_ms)));
    let dur_s = mmss(m.duration_ms);
    chrome::draw(&mut c, &pos_s, &style_time(), bar.x, bar_y + 12, Some(120));
    let dw = dur_s.len() as i32 * 9 + 8;
    chrome::draw(
        &mut c,
        &dur_s,
        &style_time(),
        bar.x + bar.w - dw,
        bar_y + 12,
        Some(dw),
    );

    let (prev, play, next) = transport_rects(Rect::new(0, 0, r.w, r.h));
    let btn = |c: &mut Canvas, rr: Rect, glyph: &str, fill: [u8; 4], ink: [u8; 4], big: bool| {
        c.fill_rounded_rect(rr, tokens::RADIUS_PILL, fill);
        chrome::draw_ink_centered(
            c,
            glyph,
            &text::LabelStyle::icon(if big { 40.0 } else { 34.0 }, ink),
            rr,
        );
    };
    let dim = if m.can_go_previous { INK } else { INK_DIM };
    btn(&mut c, prev, GLYPH_PREV, SURF, dim, false);
    btn(
        &mut c,
        play,
        if m.playing { GLYPH_PAUSE } else { GLYPH_PLAY },
        theme.accent,
        theme.text_on_accent,
        true,
    );
    let dim = if m.can_go_next { INK } else { INK_DIM };
    btn(&mut c, next, GLYPH_NEXT, SURF, dim, false);
    c.into_pixels()
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
    c.fill_rounded_rect(body, tokens::RADIUS_PILL, RIM);
    c.fill_rounded_rect(
        Rect::new(1, 1, PILL_W - 2, PILL_H - 2),
        tokens::RADIUS_PILL,
        FILL,
    );

    c.blit(
        &art_tile(theme, m.art, ART, ART as f32 / 2.0),
        Point::new(ART_PAD, (PILL_H - ART) / 2),
    );

    let eq_x = PILL_W - EQ_PAD - EQ_W;
    let base_y = PILL_H / 2 + 10;
    if m.playing {
        for (i, h) in eq_heights(t_ms, 20).into_iter().enumerate() {
            c.fill_rounded_rect(
                Rect::new(eq_x + i as i32 * (EQ_BAR_W + EQ_GAP), base_y - h, EQ_BAR_W, h),
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
    c.into_pixels()
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
        assert!(mid.y > start.y && mid.y < end.y);
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
        assert!(px.data[mid + 3] > 0x80);
    }
}
