//! Minimal tiny-skia canvas for M0: fill, rounded rect, blit, text via
//! `crate::text`. Output is premultiplied RGBA8 — the same [`Pixels`] layout
//! `de-render` uploads on both render paths.

use de_api::{Pixels, Point, Rect, Size};
use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, PixmapPaint, Transform};

/// A drawing surface of `size`, premultiplied RGBA8.
pub struct Canvas {
    pixmap: Pixmap,
}

impl Canvas {
    /// Transparent canvas.
    #[must_use]
    pub fn new(size: Size) -> Self {
        Self {
            pixmap: Pixmap::new(size.w.max(1) as u32, size.h.max(1) as u32)
                .expect("positive pixmap size"),
        }
    }

    /// Canvas pre-filled with `color`.
    #[must_use]
    pub fn filled(size: Size, color: [u8; 4]) -> Self {
        let mut c = Self::new(size);
        c.clear(color);
        c
    }

    #[must_use]
    pub fn size(&self) -> Size {
        Size::new(self.pixmap.width() as i32, self.pixmap.height() as i32)
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
        let Some(r) = clip(rect, self.size()) else {
            return;
        };
        let Some(ts) = tiny_skia::Rect::from_xywh(r.x as f32, r.y as f32, r.w as f32, r.h as f32)
        else {
            return;
        };
        let mut paint = Paint::default();
        paint.set_color(to_color(color));
        self.pixmap
            .fill_rect(ts, &paint, Transform::identity(), None);
    }

    /// Fill a rounded rectangle; `radius` is clamped to half the min side.
    pub fn fill_rounded_rect(&mut self, rect: Rect, radius: f32, color: [u8; 4]) {
        let Some(r) = clip(rect, self.size()) else {
            return;
        };
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
        self.pixmap.fill_path(
            &path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    /// Blit `src` premultiplied pixels at `dst` origin, clipped to the canvas.
    pub fn blit(&mut self, src: &Pixels, dst: Point) {
        blit_premultiplied(&mut self.pixmap, src, dst.x, dst.y);
    }

    /// Consume into `de-api` premultiplied pixels.
    #[must_use]
    pub fn into_pixels(self) -> Pixels {
        Pixels {
            size: self.size(),
            data: self.pixmap.take(),
        }
    }

    /// Copy out as `de-api` premultiplied pixels without consuming.
    #[must_use]
    pub fn to_pixels(&self) -> Pixels {
        Pixels {
            size: self.size(),
            data: self.pixmap.data().to_vec(),
        }
    }
}

/// Build a rounded-rect `Path` (tiny-skia has no built-in helper).
fn rounded_rect_path(r: tiny_skia::Rect, radius: f32) -> Option<tiny_skia::Path> {
    if radius <= 0.0 {
        let mut pb = PathBuilder::new();
        pb.push_rect(r);
        return pb.finish();
    }
    // Circle-to-cubic constant.
    const K: f32 = 0.552_284_75;
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
            let sa = src.data[si + 3] as u32;
            let inv = 255 - sa;
            for c in 0..3 {
                data[di + c] = (src.data[si + c] as u32 + (data[di + c] as u32 * inv + 127) / 255)
                    .min(255) as u8;
            }
            data[di + 3] = (sa + (data[di + 3] as u32 * inv + 127) / 255).min(255) as u8;
        }
    }
}

/// `PixmapPaint` helper kept for future scaled blits.
#[allow(dead_code)]
fn pixmap_paint() -> PixmapPaint {
    PixmapPaint::default()
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
