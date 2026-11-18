//! Glyph shaping via cosmic-text: one `Buffer` per [`TextRun`], per-glyph
//! font fallback comes free because the bundled fontdb registers both Noto
//! Sans and Noto Sans Thai (a Thai run in a "Noto Sans" buffer falls back
//! automatically — verified by `renders_latin_and_thai`).

use cosmic_text::{Attrs, Buffer, Color as CosmicColor, Family, FontSystem, Shaping, Weight};

use super::{LabelStyle, TextWrap};

/// One shaped+wrapped buffer, ready to measure or draw.
pub struct Shaped {
    pub buf: Buffer,
}

impl Shaped {
    /// Shape `text` at `style`, wrapped to `max_width` (None = single line).
    /// `tnum` enables tabular numerals (clocks, timers, percentages).
    pub fn shape(
        fs: &mut FontSystem,
        text: &str,
        style: &LabelStyle,
        max_width: Option<f32>,
        wrap: TextWrap,
    ) -> Self {
        let metrics = cosmic_text::Metrics::new(style.size, style.line_height);
        let mut buf = Buffer::new(fs, metrics);
        buf.set_size(fs, max_width, None);
        buf.set_wrap(
            fs,
            match wrap {
                TextWrap::Glyph => cosmic_text::Wrap::Glyph,
                TextWrap::Word => cosmic_text::Wrap::Word,
                TextWrap::None => cosmic_text::Wrap::None,
            },
        );
        let attrs = Attrs::new()
            .family(Family::Name(style.family))
            .weight(Weight(style.weight));
        // NOTE: `style.tnum` is informational in cosmic-text 0.12 (no
        // OpenType feature API); bundled Noto Sans digits are already
        // tabular-width so clocks/percentages line up without `tnum`.
        buf.set_text(fs, text, attrs, Shaping::Advanced);
        buf.shape_until_scroll(fs, false);
        Self { buf }
    }

    /// `(width, height)` of the shaped content in px.
    #[must_use]
    pub fn extent(&self) -> (i32, i32) {
        let (mut w, mut h) = (0f32, 0i32);
        for run in self.buf.layout_runs() {
            w = w.max(run.line_w);
            h = h.max((run.line_y + run.line_height) as i32);
        }
        (w.ceil() as i32 + 1, h.max(1))
    }

    /// Number of laid-out lines.
    #[must_use]
    pub fn lines(&self) -> usize {
        self.buf.layout_runs().count()
    }

    /// Draw into a new tight [`tiny_skia::Pixmap`] in `color`.
    #[must_use]
    pub fn rasterize(&self, fs: &mut FontSystem, color: [u8; 4]) -> Option<tiny_skia::Pixmap> {
        let (w, h) = self.extent();
        if w <= 0 || h <= 0 {
            return None;
        }
        let mut px = Pixmap::new(w as u32, h as u32)?;
        let mut cache = super::swash_cache().lock().ok()?;
        let c = CosmicColor::rgba(color[0], color[1], color[2], color[3]);
        let (pw, ph) = (px.width() as i32, px.height() as i32);
        self.buf.draw(fs, &mut cache, c, |x, y, _w, _h, gc| {
            if x < 0 || y < 0 || x >= pw || y >= ph {
                return;
            }
            let i = ((y * pw + x) * 4) as usize;
            let d = px.data_mut();
            let sa = u32::from(gc.a());
            let inv = 255 - sa;
            d[i] = ((u32::from(gc.r()) * sa + 127) / 255 + (u32::from(d[i]) * inv + 127) / 255)
                .min(255) as u8;
            d[i + 1] = ((u32::from(gc.g()) * sa + 127) / 255
                + (u32::from(d[i + 1]) * inv + 127) / 255)
                .min(255) as u8;
            d[i + 2] = ((u32::from(gc.b()) * sa + 127) / 255
                + (u32::from(d[i + 2]) * inv + 127) / 255)
                .min(255) as u8;
            d[i + 3] = (sa + (u32::from(d[i + 3]) * inv + 127) / 255).min(255) as u8;
        });
        Some(px)
    }
}

use tiny_skia::Pixmap;
