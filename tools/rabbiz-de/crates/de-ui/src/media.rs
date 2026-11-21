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
}
