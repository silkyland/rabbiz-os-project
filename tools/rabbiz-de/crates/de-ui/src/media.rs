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
const PILL_MIN_W: i32 = 200;
const ART: i32 = 30;
const PAD: i32 = 6;
const GAP: i32 = 12;
const EQ_W: i32 = 28;

const FILL: [u8; 4] = [0x0E, 0x0A, 0x12, 0xF2];
const RIM: [u8; 4] = [0xFF, 0xFF, 0xFF, 0x22];
const INK: [u8; 4] = [0xFF, 0xF8, 0xF0, 0xFF];
const INK_DIM: [u8; 4] = [0xFF, 0xF8, 0xF0, 0xA8];

// Material Icons codepoints.
const GLYPH_NOTE: &str = "\u{e405}";

/// What the pill shows.
#[derive(Debug, Clone, Copy)]
pub struct PillModel<'a> {
    pub title: &'a str,
    pub artist: &'a str,
    pub playing: bool,
    /// Decoded album art (any size; scaled to the badge).
    pub art: Option<&'a Pixels>,
}

/// Widest the pill may grow on this output.
#[must_use]
pub fn pill_max_w(output: Size) -> i32 {
    let side = if output.w > output.h { 560 } else { 420 };
    side.min(output.w - 2 * tokens::PAGE_MARGIN).max(PILL_MIN_W)
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

/// Ping-pong marquee: dwell, scroll `overflow` px at 45 px/s, dwell, return.
#[must_use]
pub fn marquee_offset(overflow: i32, t_ms: u64) -> i32 {
    if overflow <= 0 {
        return 0;
    }
    const DWELL: u64 = 1400;
    let travel = (overflow as u64 * 1000 / 45).max(1);
    let cycle = 2 * (DWELL + travel);
    let t = t_ms % cycle;
    let frac = if t < DWELL {
        0.0
    } else if t < DWELL + travel {
        (t - DWELL) as f32 / travel as f32
    } else if t < 2 * DWELL + travel {
        1.0
    } else {
        1.0 - (t - 2 * DWELL - travel) as f32 / travel as f32
    };
    (overflow as f32 * frac).round() as i32
}

/// Equalizer bar heights (px, 4 bars) at `t_ms`; `max_h` is the tallest bar.
#[must_use]
pub fn eq_heights(t_ms: u64, max_h: i32) -> [i32; 4] {
    let t = t_ms as f32 / 1000.0;
    let bar = |speed: f32, off: f32| {
        let v = 0.5 + 0.5 * (t * speed + off).sin();
        (4.0 + (max_h - 4) as f32 * v).round() as i32
    };
    [bar(7.1, 0.0), bar(9.3, 1.7), bar(6.2, 3.1), bar(8.4, 4.6)]
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

fn line(m: &PillModel<'_>) -> String {
    if m.artist.is_empty() {
        m.title.to_string()
    } else if m.title.is_empty() {
        m.artist.to_string()
    } else {
        format!("{} \u{b7} {}", m.title, m.artist)
    }
}

/// Render the pill at time `t_ms` (drives marquee + equalizer). Width adapts
/// to the text up to [`pill_max_w`].
#[must_use]
pub fn render_pill(theme: &Theme, output: Size, m: &PillModel<'_>, t_ms: u64) -> Pixels {
    let label_style = text::LabelStyle {
        size: 20.0,
        line_height: 26.0,
        weight: 600,
        color: INK,
        ..text::LabelStyle::default()
    };
    let label = text::label(&line(m), label_style, None).map(|p| {
        Pixels::new(
            Size::new(p.width() as i32, p.height() as i32),
            p.data().to_vec(),
        )
    });
    let text_w = label.as_ref().map_or(0, |p| p.size.w);
    let fixed = PAD + ART + GAP + GAP + EQ_W + GAP;
    let w = (fixed + text_w)
        .clamp(PILL_MIN_W, pill_max_w(output))
        .max(PILL_MIN_W);
    let view_w = w - fixed;

    let mut c = Canvas::new(Size::new(w, PILL_H));
    let body = Rect::new(0, 0, w, PILL_H);
    c.fill_rounded_rect(body, tokens::RADIUS_PILL, RIM);
    c.fill_rounded_rect(
        Rect::new(1, 1, w - 2, PILL_H - 2),
        tokens::RADIUS_PILL,
        FILL,
    );

    let art_y = (PILL_H - ART) / 2;
    c.blit(
        &art_tile(theme, m.art, ART, ART as f32 / 2.0),
        Point::new(PAD, art_y),
    );

    if let Some(label) = label {
        let tx = PAD + ART + GAP;
        let overflow = (text_w - view_w).max(0);
        let off = marquee_offset(overflow, t_ms);
        let ty = (PILL_H - label.size.h) / 2 + 2;
        c.blit_region(
            &label,
            Rect::new(off, 0, view_w.min(text_w), label.size.h),
            Point::new(tx, ty),
        );
        if overflow > 0 {
            // Soft edge so clipped text fades instead of being sliced.
            let edge = Rect::new(tx + view_w - 14, 3, 14, PILL_H - 6);
            c.fill_rect(edge, [FILL[0], FILL[1], FILL[2], 0x90]);
        }
    }

    let eq_x = w - PAD - GAP - EQ_W + 2;
    let base_y = PILL_H / 2 + 9;
    if m.playing {
        for (i, h) in eq_heights(t_ms, 18).into_iter().enumerate() {
            c.fill_rounded_rect(
                Rect::new(eq_x + i as i32 * 7, base_y - h, 4, h),
                2.0,
                theme.accent_hi,
            );
        }
    } else {
        for i in 0..2 {
            c.fill_rounded_rect(
                Rect::new(eq_x + 6 + i * 10, PILL_H / 2 - 8, 5, 16),
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
    fn marquee_dwells_then_ping_pongs() {
        assert_eq!(marquee_offset(0, 123_456), 0);
        assert_eq!(marquee_offset(90, 0), 0);
        assert_eq!(marquee_offset(90, 1_000), 0);
        // 90 px at 45 px/s = 2 s of travel after the 1.4 s dwell.
        assert_eq!(marquee_offset(90, 1_400 + 1_000), 45);
        assert_eq!(marquee_offset(90, 1_400 + 2_000 + 500), 90);
        assert_eq!(marquee_offset(90, 1_400 + 2_000 + 1_400 + 1_000), 45);
    }

    #[test]
    fn eq_bars_stay_in_range_and_move() {
        let a = eq_heights(0, 18);
        let b = eq_heights(333, 18);
        assert_ne!(a, b);
        for h in a.into_iter().chain(b) {
            assert!((4..=18).contains(&h));
        }
    }

    #[test]
    fn pill_width_tracks_text_and_output() {
        let theme = &tokens::DARK;
        let short = PillModel {
            title: "A",
            artist: "",
            playing: true,
            art: None,
        };
        let long = PillModel {
            title: "A very long song title that cannot possibly fit in the capsule",
            artist: "Some Artist With A Long Name",
            playing: false,
            art: None,
        };
        let s = render_pill(theme, OUT, &short, 0);
        let l = render_pill(theme, OUT, &long, 0);
        assert_eq!(s.size.h, PILL_H);
        assert_eq!(s.size.w, PILL_MIN_W);
        assert_eq!(l.size.w, pill_max_w(OUT));
        // Landscape allows a wider capsule.
        assert!(pill_max_w(Size::new(1600, 720)) > pill_max_w(OUT));
        assert_ne!(
            render_pill(theme, OUT, &long, 0).data,
            render_pill(theme, OUT, &long, 2_400).data,
            "overflowing text scrolls"
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
}
