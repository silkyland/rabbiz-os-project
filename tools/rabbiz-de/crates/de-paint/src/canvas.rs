//! Minimal tiny-skia canvas for M0: fill, rounded rect, blit, text via
//! `crate::text`. Output is premultiplied RGBA8 — the same [`Pixels`] layout
//! `de-render` uploads on both render paths.

use de_api::{Pixels, Point, Rect, Size};
use tiny_skia::{Color, FillRule, IntSize, Paint, PathBuilder, Pixmap, PixmapPaint, Transform};

/// A drawing surface of `size`, premultiplied RGBA8.
pub struct Canvas {
    pixmap: Pixmap,
    origin: Point,
    surface_size: Size,
}

impl Canvas {
    /// Transparent canvas.
    #[must_use]
    pub fn new(size: Size) -> Self {
        let w = size.w.max(1) as u32;
        let h = size.h.max(1) as u32;
        Self {
            pixmap: Pixmap::new(w, h).expect("positive pixmap size"),
            origin: Point::ZERO,
            surface_size: Size::new(w as i32, h as i32),
        }
    }

    /// Canvas pre-filled with `color`.
    #[must_use]
    pub fn filled(size: Size, color: [u8; 4]) -> Self {
        let mut c = Self::new(size);
        c.clear(color);
        c
    }

    /// Wrap an existing [`Pixels`] buffer without copying.
    ///
    /// Returns `None` if `pixels.size` is empty or `pixels.data.len()` does not
    /// equal `4 * w * h`.
    #[must_use]
    pub fn from_existing(pixels: Pixels) -> Option<Self> {
        let w = u32::try_from(pixels.size.w).ok()?;
        let h = u32::try_from(pixels.size.h).ok()?;
        let int_size = IntSize::from_wh(w, h)?;
        let pixmap = Pixmap::from_vec(pixels.data, int_size)?;
        Some(Self {
            pixmap,
            origin: Point::ZERO,
            surface_size: pixels.size,
        })
    }

    /// Paint into `pixels` strictly within `clip` (clamped to `pixels.size`).
    ///
    /// Drawing calls inside `f` use full-surface coordinates; everything outside
    /// `clip` is clipped away and pixels outside `clip` in `pixels` are left
    /// untouched.
    ///
    /// If `clear_color` is `Some(rgba)`, the clipped sub-region is cleared to
    /// `rgba` before invoking `f`. If `None`, the existing pixels inside `clip`
    /// are preserved as the starting backdrop for `f`.
    pub fn paint_clipped(
        pixels: &mut Pixels,
        clip: Rect,
        clear_color: Option<[u8; 4]>,
        f: impl FnOnce(&mut Self),
    ) {
        if pixels.size.is_empty() || pixels.data.len() != (pixels.size.area() * 4) as usize {
            return;
        }
        let surface_rect = Rect::from_origin_size(Point::ZERO, pixels.size);
        let Some(clamped) = clip.intersect(surface_rect) else {
            return;
        };
        let Some(mut pixmap) = Pixmap::new(clamped.w as u32, clamped.h as u32) else {
            return;
        };

        let sub_stride = (clamped.w * 4) as usize;
        let full_stride = (pixels.size.w * 4) as usize;
        let x_bytes = (clamped.x * 4) as usize;

        if let Some(color) = clear_color {
            pixmap.fill(to_color(color));
        } else {
            let sub = pixmap.data_mut();
            for row in 0..clamped.h as usize {
                let sy = (clamped.y as usize + row) * full_stride + x_bytes;
                let dy = row * sub_stride;
                sub[dy..dy + sub_stride].copy_from_slice(&pixels.data[sy..sy + sub_stride]);
            }
        }

        let mut canvas = Self {
            pixmap,
            origin: clamped.origin(),
            surface_size: pixels.size,
        };
        f(&mut canvas);

        let sub = canvas.pixmap.data();
        for row in 0..clamped.h as usize {
            let dy = (clamped.y as usize + row) * full_stride + x_bytes;
            let sy = row * sub_stride;
            pixels.data[dy..dy + sub_stride].copy_from_slice(&sub[sy..sy + sub_stride]);
        }
    }

    #[must_use]
    pub fn size(&self) -> Size {
        self.surface_size
    }

    /// Surface-space origin of this canvas's backing pixmap (`Point::ZERO` for
    /// full-surface canvases, or `clip.origin()` inside [`Self::paint_clipped`]).
    #[must_use]
    pub fn origin(&self) -> Point {
        self.origin
    }

    /// Active clip rectangle in surface space.
    #[must_use]
    pub fn clip_rect(&self) -> Rect {
        Rect::from_origin_size(
            self.origin,
            Size::new(self.pixmap.width() as i32, self.pixmap.height() as i32),
        )
    }

    pub fn clear(&mut self, color: [u8; 4]) {
        self.pixmap.fill(to_color(color));
    }

    /// Borrow the underlying pixmap (for text rasterization).
    pub fn pixmap_mut(&mut self) -> &mut Pixmap {
        &mut self.pixmap
    }

    #[must_use]
    pub fn pixmap(&self) -> &Pixmap {
        &self.pixmap
    }

    /// Fill an axis-aligned rect (clipped to the canvas).
    pub fn fill_rect(&mut self, rect: Rect, color: [u8; 4]) {
        let Some(r) = rect.intersect(self.clip_rect()) else {
            return;
        };
        let Some(ts) = tiny_skia::Rect::from_xywh(
            (r.x - self.origin.x) as f32,
            (r.y - self.origin.y) as f32,
            r.w as f32,
            r.h as f32,
        ) else {
            return;
        };
        let mut paint = Paint::default();
        paint.set_color(to_color(color));
        self.pixmap
            .fill_rect(ts, &paint, Transform::identity(), None);
    }

    /// Fill a rounded rectangle; `radius` is clamped to half the min side.
    pub fn fill_rounded_rect(&mut self, rect: Rect, radius: f32, color: [u8; 4]) {
        let Some(r) = clip(rect, self.surface_size) else {
            return;
        };
        if r.intersect(self.clip_rect()).is_none() {
            return;
        }
        let Some(ts) = tiny_skia::Rect::from_xywh(r.x as f32, r.y as f32, r.w as f32, r.h as f32)
        else {
            return;
        };
        let radius = radius.min(ts.width() / 2.0).min(ts.height() / 2.0).max(0.0);
        let Some(path) = rounded_rect_path(ts, radius) else {
            return;
        };
        let mut paint = Paint::default();
        paint.set_color(to_color(color));
        paint.anti_alias = true;
        let transform = if self.origin == Point::ZERO {
            Transform::identity()
        } else {
            Transform::from_translate(-self.origin.x as f32, -self.origin.y as f32)
        };
        self.pixmap
            .fill_path(&path, &paint, FillRule::Winding, transform, None);
    }

    /// Blit `src` premultiplied pixels at `dst` origin, clipped to the canvas.
    pub fn blit(&mut self, src: &Pixels, dst: Point) {
        blit_premultiplied(
            &mut self.pixmap,
            src,
            dst.x - self.origin.x,
            dst.y - self.origin.y,
        );
    }

    /// Composite a scaled image with a uniform opacity.
    pub fn blit_scaled(&mut self, pixels: &Pixels, dst: Rect, opacity: f32) {
        if dst.is_empty() || opacity <= 0.0 || pixels.size.w <= 0 || pixels.size.h <= 0 {
            return;
        }
        if dst.intersect(self.clip_rect()).is_none() {
            return;
        }
        let Some(image) = tiny_skia::PixmapRef::from_bytes(
            &pixels.data,
            pixels.size.w as u32,
            pixels.size.h as u32,
        ) else {
            return;
        };
        let transform = Transform::from_scale(
            dst.w as f32 / pixels.size.w as f32,
            dst.h as f32 / pixels.size.h as f32,
        )
        .post_translate(
            (dst.x - self.origin.x) as f32,
            (dst.y - self.origin.y) as f32,
        );
        let paint = PixmapPaint {
            opacity: opacity.clamp(0.0, 1.0),
            quality: tiny_skia::FilterQuality::Bilinear,
            ..PixmapPaint::default()
        };
        self.pixmap
            .draw_pixmap(0, 0, image, &paint, transform, None);
    }

    /// Blit only `region` of `src` at `dst` (scroll/parallax crops without
    /// materialising a slice).
    pub fn blit_region(&mut self, src: &Pixels, region: Rect, dst: Point) {
        let r = region.intersect(Rect::from_origin_size(Point::ZERO, src.size));
        let Some(r) = r else {
            return;
        };
        let mut slice = Pixels::empty(r.size());
        for y in 0..r.h {
            let (si, di) = (
                (((r.y + y) * src.size.w + r.x) * 4) as usize,
                ((y * r.w) * 4) as usize,
            );
            let n = (r.w * 4) as usize;
            slice.data[di..di + n].copy_from_slice(&src.data[si..si + n]);
        }
        blit_premultiplied(
            &mut self.pixmap,
            &slice,
            dst.x - self.origin.x,
            dst.y - self.origin.y,
        );
    }

    /// Consume into `de-api` premultiplied pixels.
    #[must_use]
    pub fn into_pixels(self) -> Pixels {
        let size = Size::new(self.pixmap.width() as i32, self.pixmap.height() as i32);
        Pixels {
            size,
            data: self.pixmap.take(),
        }
    }

    /// Copy out as `de-api` premultiplied pixels without consuming.
    #[must_use]
    pub fn to_pixels(&self) -> Pixels {
        Pixels {
            size: Size::new(self.pixmap.width() as i32, self.pixmap.height() as i32),
            data: self.pixmap.data().to_vec(),
        }
    }
}

/// Build a rounded-rect `Path` (tiny-skia has no built-in helper).
fn rounded_rect_path(r: tiny_skia::Rect, radius: f32) -> Option<tiny_skia::Path> {
    // Circle-to-cubic constant.
    const K: f32 = 0.552_284_8;
    if radius <= 0.0 {
        let mut pb = PathBuilder::new();
        pb.push_rect(r);
        return pb.finish();
    }
    let (l, t, rr, b) = (r.left(), r.top(), r.right(), r.bottom());
    let rad = radius.min((rr - l) / 2.0).min((b - t) / 2.0);
    let k = rad * K;
    let mut pb = PathBuilder::new();
    pb.move_to(l + rad, t);
    pb.line_to(rr - rad, t);
    pb.cubic_to(rr - rad + k, t, rr, t + rad - k, rr, t + rad);
    pb.line_to(rr, b - rad);
    pb.cubic_to(rr, b - rad + k, rr - rad + k, b, rr - rad, b);
    pb.line_to(l + rad, b);
    pb.cubic_to(l + rad - k, b, l, b - rad + k, l, b - rad);
    pb.line_to(l, t + rad);
    pb.cubic_to(l, t + rad - k, l + rad - k, t, l + rad, t);
    pb.close();
    pb.finish()
}

fn to_color([r, g, b, a]: [u8; 4]) -> Color {
    Color::from_rgba8(r, g, b, a)
}

fn clip(rect: Rect, bounds: Size) -> Option<Rect> {
    rect.intersect(Rect::from_origin_size(Point::ZERO, bounds))
}

/// Src-over blit of premultiplied `src` into `dst` pixmap at (`dx`, `dy`).
pub fn blit_premultiplied(dst: &mut Pixmap, src: &Pixels, dx: i32, dy: i32) {
    let dw = dst.width() as i32;
    let dh = dst.height() as i32;
    let sw = src.size.w;
    let sh = src.size.h;
    let data = dst.data_mut();
    for sy in 0..sh {
        let y = sy + dy;
        if y < 0 || y >= dh {
            continue;
        }
        for sx in 0..sw {
            let x = sx + dx;
            if x < 0 || x >= dw {
                continue;
            }
            let si = ((sy * sw + sx) * 4) as usize;
            let di = ((y * dw + x) * 4) as usize;
            let sa = u32::from(src.data[si + 3]);
            let inv = 255 - sa;
            for c in 0..3 {
                data[di + c] = (u32::from(src.data[si + c])
                    + (u32::from(data[di + c]) * inv + 127) / 255)
                    .min(255) as u8;
            }
            data[di + 3] = (sa + (u32::from(data[di + 3]) * inv + 127) / 255).min(255) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::damage::DamageTracker;
    use crate::region_cache::RegionCache;
    use crate::text::{draw_label, LabelStyle};

    #[test]
    fn fill_and_blit() {
        let mut c = Canvas::new(Size::new(8, 8));
        c.fill_rect(Rect::new(0, 0, 8, 8), [255, 0, 0, 255]);
        let mut tile = Canvas::new(Size::new(4, 4));
        tile.fill_rounded_rect(Rect::new(0, 0, 4, 4), 2.0, [0, 255, 0, 255]);
        c.blit(&tile.to_pixels(), Point::new(2, 2));
        let px = c.to_pixels();
        // Center pixel: green over red at full coverage.
        let i = ((4 * 8 + 4) * 4) as usize;
        assert_eq!(px.data[i + 1], 255);
        assert_eq!(px.data[i + 3], 255);
    }

    #[test]
    fn from_existing_round_trip_and_validation() {
        let px = Canvas::filled(Size::new(12, 10), [10, 20, 30, 255]).into_pixels();
        let mut c = Canvas::from_existing(px.clone()).expect("valid pixels");
        assert_eq!(c.size(), Size::new(12, 10));
        c.fill_rect(Rect::new(2, 2, 4, 4), [200, 100, 50, 255]);
        let out = c.into_pixels();
        assert_eq!(out.size, Size::new(12, 10));
        assert_ne!(out.data, px.data);

        // Reject empty size or mismatched byte length.
        assert!(Canvas::from_existing(Pixels::empty(Size::ZERO)).is_none());
        let bad = Pixels {
            size: Size::new(4, 4),
            data: vec![0; 12],
        };
        assert!(Canvas::from_existing(bad).is_none());
    }

    #[test]
    fn paint_clipped_leaves_outside_untouched_and_clamps_bounds() {
        let bg = [18, 24, 36, 255];
        let mut px = Canvas::filled(Size::new(32, 32), bg).into_pixels();
        let before = px.data.clone();

        // Empty or out-of-bounds clip is a no-op.
        Canvas::paint_clipped(
            &mut px,
            Rect::new(40, 40, 10, 10),
            Some([255, 0, 0, 255]),
            |c| {
                c.fill_rect(Rect::new(0, 0, 32, 32), [0, 255, 0, 255]);
            },
        );
        assert_eq!(px.data, before);

        // Clip extends partially past top-left; draw over entire surface, only clamped clip changes.
        Canvas::paint_clipped(&mut px, Rect::new(-4, -4, 12, 12), None, |c| {
            assert_eq!(c.clip_rect(), Rect::new(0, 0, 8, 8));
            c.fill_rect(Rect::new(0, 0, 32, 32), [255, 128, 0, 255]);
        });

        for y in 0..32 {
            for x in 0..32 {
                let i = ((y * 32 + x) * 4) as usize;
                let got = &px.data[i..i + 4];
                if x < 8 && y < 8 {
                    assert_eq!(got, &[255, 128, 0, 255], "inside clip at ({x},{y})");
                } else {
                    assert_eq!(got, &bg, "outside clip at ({x},{y})");
                }
            }
        }
    }

    #[test]
    fn incremental_clipped_paint_is_byte_identical_to_full_paint() {
        #[derive(Clone, Copy)]
        struct SceneState {
            clock: &'static str,
            clock_hash: u64,
            pill_color: [u8; 4],
            pill_hash: u64,
            card_color: [u8; 4],
            card_hash: u64,
        }

        let size = Size::new(240, 120);
        let bg = [16, 20, 28, 255];

        // Reusable icon tile for blit & blit_scaled coverage.
        let mut badge = Canvas::new(Size::new(16, 16));
        badge.fill_rounded_rect(Rect::new(0, 0, 16, 16), 5.0, [240, 180, 60, 255]);
        let badge_px = badge.into_pixels();

        let r_banner = Rect::new(8, 8, 224, 24);
        let r_clock = Rect::new(12, 42, 96, 36);
        let r_pill = Rect::new(120, 42, 108, 36);
        let r_card = Rect::new(12, 84, 216, 28);

        let paint_widgets = |c: &mut Canvas, state: &SceneState| {
            // Static top banner (rounded rect + blitted badge + scaled badge).
            c.fill_rounded_rect(r_banner, 8.0, [34, 42, 58, 255]);
            c.blit(&badge_px, Point::new(14, 12));
            c.blit_scaled(&badge_px, Rect::new(206, 10, 20, 20), 0.75);

            // Clock widget (rounded pill + anti-aliased text).
            c.fill_rounded_rect(r_clock, 10.0, [48, 56, 76, 255]);
            let _ = draw_label(
                c,
                state.clock,
                Point::new(r_clock.x + 10, r_clock.y + 4),
                LabelStyle {
                    size: 20.0,
                    line_height: 26.0,
                    color: [255, 255, 255, 255],
                    ..LabelStyle::default()
                },
                None,
            );

            // Status pill widget (changes in frame 2).
            c.fill_rounded_rect(r_pill, 12.0, state.pill_color);
            c.fill_rect(
                Rect::new(r_pill.x + 14, r_pill.y + 12, 24, 12),
                [255, 255, 255, 200],
            );

            // Bottom card (static).
            c.fill_rounded_rect(r_card, 6.0, state.card_color);
        };

        let frame1_state = SceneState {
            clock: "10:24",
            clock_hash: 1,
            pill_color: [60, 140, 220, 255],
            pill_hash: 10,
            card_color: [38, 46, 62, 255],
            card_hash: 100,
        };
        let frame2_state = SceneState {
            clock: "10:24",
            clock_hash: 1,
            pill_color: [220, 80, 100, 255],
            pill_hash: 20, // only the pill changed
            card_color: [38, 46, 62, 255],
            card_hash: 100,
        };

        // Full paint of frame 1.
        let mut c1 = Canvas::filled(size, bg);
        paint_widgets(&mut c1, &frame1_state);
        let frame1_pixels = c1.into_pixels();

        // Full paint of frame 2 (ground truth).
        let mut c2 = Canvas::filled(size, bg);
        paint_widgets(&mut c2, &frame2_state);
        let full_frame2 = c2.into_pixels();

        // Incremental paint: track regions across frame 1 -> frame 2.
        let mut cache = RegionCache::new();
        let mut damage = DamageTracker::new();

        cache.begin_frame();
        cache.record(1, 1, r_banner);
        cache.record(2, frame1_state.clock_hash, r_clock);
        cache.record(3, frame1_state.pill_hash, r_pill);
        cache.record(4, frame1_state.card_hash, r_card);
        cache.finish_frame(&mut damage);
        let _ = damage.take();

        cache.begin_frame();
        assert!(!cache.record(1, 1, r_banner));
        assert!(!cache.record(2, frame2_state.clock_hash, r_clock));
        assert!(cache.record(3, frame2_state.pill_hash, r_pill));
        assert!(!cache.record(4, frame2_state.card_hash, r_card));
        cache.finish_frame(&mut damage);

        let dirty = damage.take();
        assert_eq!(dirty, vec![r_pill]);

        let mut incremental_frame2 = frame1_pixels;
        for rect in dirty {
            Canvas::paint_clipped(&mut incremental_frame2, rect, Some(bg), |c| {
                paint_widgets(c, &frame2_state);
            });
        }

        assert_eq!(full_frame2.size, incremental_frame2.size);
        assert_eq!(
            full_frame2.data, incremental_frame2.data,
            "incremental clipped repaint must be byte-identical to full repaint"
        );
    }

    #[test]
    fn partial_rounded_rect_across_clip_boundary_is_byte_identical() {
        let size = Size::new(100, 60);
        let bg = [10, 12, 16, 255];
        let big_pill = Rect::new(10, 10, 80, 40);
        let clip_rect = Rect::new(40, 20, 35, 30);

        let mut full = Canvas::filled(size, bg);
        full.fill_rounded_rect(big_pill, 16.0, [90, 200, 140, 255]);
        let full_px = full.into_pixels();

        let mut inc = Canvas::filled(size, bg).into_pixels();
        Canvas::paint_clipped(&mut inc, clip_rect, Some(bg), |c| {
            c.fill_rounded_rect(big_pill, 16.0, [90, 200, 140, 255]);
        });

        // Inside `clip_rect`, every pixel must match `full_px` byte-for-byte.
        for y in clip_rect.y..clip_rect.bottom() {
            for x in clip_rect.x..clip_rect.right() {
                let i = ((y * size.w + x) * 4) as usize;
                assert_eq!(
                    &inc.data[i..i + 4],
                    &full_px.data[i..i + 4],
                    "mismatch at ({x},{y})"
                );
            }
        }
    }
}
