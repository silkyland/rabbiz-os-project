//! "Now playing" status pill (docs/DE-NOTIFICATIONS-SERVICES.md §2.5): a small
//! capsule centred in the status-bar band — thumbnail, ping-pong marquee
//! title, playback-state glyph — composited by `de-core` as its own tiny
//! `DisplayArea::SystemBars` texture so the marquee re-rasters ~400×36 px
//! instead of the whole shell frame.

use de_api::{Pixels, Point, Rect};
use de_paint::{text, Canvas};

/// Capsule height: fits inside the 56 px status bar with a margin above/below.
pub const PILL_H: i32 = 36;
/// Top edge of the pill inside the status-bar band.
pub const PILL_Y: i32 = 10;
/// The pill never grows wider than this, whatever the title length.
pub const PILL_MAX_W: i32 = 380;
/// Smallest clip the marquee text gets before the pill stops shrinking.
const MIN_CLIP: i32 = 48;
/// Marquee speed in logical px/s.
const MARQUEE_PX_PER_S: u64 = 45;
/// Pause at each marquee end before reversing direction.
const MARQUEE_DWELL_MS: u64 = 1400;

const PILL_BG: [u8; 4] = [0x10, 0x0B, 0x14, 0xB8];
const INK: [u8; 4] = [0xFF, 0xF8, 0xF0, 0xFF];
const INK_DIM: [u8; 4] = [0xFF, 0xF8, 0xF0, 0xB0];
const THUMB_TINT: [u8; 4] = [0xFF, 0xF8, 0xF0, 0x24];

// Material Icons codepoints.
const GLYPH_NOTE: &str = "\u{e405}";
const GLYPH_EQ: &str = "\u{e01d}"; // equalizer — playing
const GLYPH_PAUSE: &str = "\u{e034}";

const PAD: i32 = 12;
const GAP: i32 = 10;
const THUMB: i32 = 28;
const GLYPH: i32 = 26;

/// What the pill needs; `scroll_ms` is the compositor's monotonic clock, so
/// the marquee offset is a pure function of time — no accumulated state.
pub struct MediaPillModel<'a> {
    /// Marquee text — "Title — Artist", preformatted by the caller.
    pub label: &'a str,
    /// Playing → equalizer bars; paused → pause glyph.
    pub playing: bool,
    /// Album art already rasterized; `None` draws the music-note tile.
    pub art: Option<&'a Pixels>,
    pub scroll_ms: u64,
}

/// Ping-pong marquee offset for a `text_w` label clipped to `clip_w`:
/// scrolls to the end, dwells, scrolls back. `0` when the text already fits.
#[must_use]
pub fn marquee_offset(text_w: i32, clip_w: i32, ms: u64) -> i32 {
    let over = (text_w - clip_w).max(0) as u64;
    if over == 0 {
        return 0;
    }
    let travel = over * 1000 / MARQUEE_PX_PER_S;
    let leg = travel + MARQUEE_DWELL_MS;
    let t = ms % (2 * leg);
    let run = |phase: u64| ((phase.min(travel) * MARQUEE_PX_PER_S + 999) / 1000).min(over) as i32;
    if t < leg {
        run(t)
    } else {
        over as i32 - run(t - leg)
    }
}

/// Width the pill takes for a given text width (shared by raster + hit rect).
fn pill_width(text_w: i32, screen_w: i32) -> i32 {
    let fixed = PAD + THUMB + GAP + GAP + GLYPH + PAD;
    let clip = text_w
        .min(PILL_MAX_W - fixed)
        .min(screen_w - 96 - fixed)
        .max(MIN_CLIP);
    fixed + clip
}

/// Rasterize the pill; the returned `Pixels` size is the rect the caller
/// centres on the screen.
#[must_use]
pub fn render_pill(m: &MediaPillModel<'_>, screen_w: i32) -> Option<Pixels> {
    let st = text::LabelStyle {
        size: 19.0,
        line_height: 24.0,
        color: INK,
        ..text::LabelStyle::default()
    };
    let label = text::label(m.label, st, None)?;
    let text_w = label.width() as i32;
    let text_h = label.height() as i32;
    let w = pill_width(text_w, screen_w);
    let clip_w = w - PAD - THUMB - GAP - GAP - GLYPH - PAD;
    let mut c = Canvas::new(de_api::Size::new(w, PILL_H));
    c.fill_rounded_rect(Rect::new(0, 0, w, PILL_H), (PILL_H / 2) as f32, PILL_BG);

    // Thumbnail: album art scaled into the tile, else the note glyph.
    let thumb = Rect::new(PAD - 4, (PILL_H - THUMB) / 2, THUMB, THUMB);
    if let Some(art) = m.art {
        c.blit_scaled(art, thumb, 1.0);
    } else {
        c.fill_rounded_rect(thumb, 7.0, THUMB_TINT);
        crate::chrome::draw_ink_centered(
            &mut c,
            GLYPH_NOTE,
            &text::LabelStyle::icon(18.0, INK),
            thumb,
        );
    }

    // Marquee text, clipped to the space the glyph row leaves.
    let text_x = thumb.x + thumb.w + GAP;
    let off = marquee_offset(text_w, clip_w, m.scroll_ms);
    let vis_w = (text_w - off).min(clip_w);
    if vis_w > 0 {
        let px = Pixels::new(de_api::Size::new(text_w, text_h), label.data().to_vec());
        c.blit_region(
            &px,
            Rect::new(off, 0, vis_w, text_h),
            Point::new(text_x, (PILL_H - text_h) / 2),
        );
    }

    // Playback-state glyph on the trailing edge.
    let glyph = if m.playing { GLYPH_EQ } else { GLYPH_PAUSE };
    crate::chrome::draw_ink_centered(
        &mut c,
        glyph,
        &text::LabelStyle::icon(20.0, INK_DIM),
        Rect::new(w - PAD - GLYPH, 0, GLYPH, PILL_H),
    );
    Some(c.into_pixels())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marquee_stays_put_when_text_fits() {
        for ms in [0, 500, 10_000, u64::MAX - 1] {
            assert_eq!(marquee_offset(100, 200, ms), 0);
        }
    }

    #[test]
    fn marquee_pings_pong_between_ends() {
        // over = 100 px → travel = 2222 ms, dwell 1400, leg = 3622 ms.
        let (tw, cw) = (300, 200);
        let travel = 100 * 1000 / MARQUEE_PX_PER_S; // 2222
        let leg = travel + MARQUEE_DWELL_MS;
        assert_eq!(marquee_offset(tw, cw, 0), 0);
        assert!((marquee_offset(tw, cw, travel / 2) - 50).abs() <= 1);
        assert_eq!(marquee_offset(tw, cw, travel), 100);
        assert_eq!(marquee_offset(tw, cw, travel + MARQUEE_DWELL_MS - 1), 100);
        assert!((marquee_offset(tw, cw, leg + travel / 2) - 50).abs() <= 1);
        assert_eq!(marquee_offset(tw, cw, leg + travel + MARQUEE_DWELL_MS), 0);
        // Wraps the period cleanly.
        assert_eq!(marquee_offset(tw, cw, 2 * leg), 0);
    }

    #[test]
    fn pill_rasterizes_and_sizing_bounds() {
        let m = MediaPillModel {
            label: "Some reasonably long track title — Some Artist",
            playing: true,
            art: None,
            scroll_ms: 0,
        };
        let px = render_pill(&m, 720).expect("pill renders");
        assert_eq!(px.size.h, PILL_H);
        assert!(px.size.w > 100 && px.size.w <= PILL_MAX_W);
        // Capsule + glyph + text paint ink; the corners stay transparent.
        assert!(px.data.iter().any(|a| *a != 0));
        let corner = &px.data[0..4];
        assert_eq!(corner[3], 0);
    }
}
