//! Frosted ("matte glass") surfaces without a live blur.
//!
//! A live backdrop blur costs a full-screen convolution per frame, which the
//! A21s' CPU/GPU budget cannot afford (docs/DE-DESIGN-LIGHT-TOKENS.md open
//! item 3). Instead the backdrop (wallpaper or the last app frame) is blurred
//! **once**, cached, and every glass element samples that blurred copy under
//! a translucent tint. It looks like frosted glass, costs one blit per
//! element, and has no shadow: matte, as the owner asked.

use de_api::{Pixels, Point, Rect, Size};

use crate::Canvas;

/// Blur radius (px at the 1/4 scale the blur runs at) per pass; two passes of a
/// box blur approximate a Gaussian closely enough for a backdrop.
const RADIUS: i32 = 7;
/// The blur runs on a copy this many times smaller than the screen.
const DOWNSCALE: i32 = 4;

/// One separable box-blur pass over premultiplied RGBA8 (edges clamp).
fn box_blur_pass(src: &[u8], w: i32, h: i32, r: i32, horizontal: bool) -> Vec<u8> {
    let mut out = vec![0u8; src.len()];
    let (len, lines) = if horizontal { (w, h) } else { (h, w) };
    let norm = (2 * r + 1) as u32;
    for line in 0..lines {
        let at = |i: i32| -> usize {
            let i = i.clamp(0, len - 1);
            let (x, y) = if horizontal { (i, line) } else { (line, i) };
            ((y * w + x) * 4) as usize
        };
        let mut acc = [0u32; 4];
        for i in -r..=r {
            let p = at(i);
            for (c, a) in acc.iter_mut().enumerate() {
                *a += u32::from(src[p + c]);
            }
        }
        for i in 0..len {
            let o = at(i);
            for c in 0..4 {
                out[o + c] = ((acc[c] + norm / 2) / norm) as u8;
            }
            let (add, sub) = (at(i + r + 1), at(i - r));
            for (c, a) in acc.iter_mut().enumerate() {
                *a = *a + u32::from(src[add + c]) - u32::from(src[sub + c]);
            }
        }
    }
    out
}

/// Blurred copy of `src` at the same size: downscale, blur twice in both
/// directions, scale back up (bilinear).
#[must_use]
pub fn blurred_backdrop(src: &Pixels) -> Pixels {
    let (sw, sh) = (
        (src.size.w / DOWNSCALE).max(1),
        (src.size.h / DOWNSCALE).max(1),
    );
    let mut small = Canvas::new(Size::new(sw, sh));
    small.blit_scaled(src, Rect::new(0, 0, sw, sh), 1.0);
    let mut data = small.into_pixels().data;
    for _ in 0..2 {
        data = box_blur_pass(&data, sw, sh, RADIUS, true);
        data = box_blur_pass(&data, sw, sh, RADIUS, false);
    }
    let blurred = Pixels::new(Size::new(sw, sh), data);
    let mut full = Canvas::new(src.size);
    full.blit_scaled(&blurred, Rect::new(0, 0, src.size.w, src.size.h), 1.0);
    full.into_pixels()
}

/// Paint a frosted rounded rectangle onto `c`: the blurred `backdrop` seen
/// through `rect` (same coordinates as the canvas), a flat `tint` over it, no
/// shadow. `tint` is straight RGBA (alpha sets how milky the glass is).
#[allow(clippy::chunks_exact_to_as_chunks)] // runtime-sized RGBA pixels
pub fn frost(c: &mut Canvas, backdrop: &Pixels, rect: Rect, radius: f32, tint: [u8; 4]) {
    let Some(clip) = rect.intersect(Rect::from_origin_size(Point::ZERO, c.size())) else {
        return;
    };
    // Backdrop cut-out, clipped by a rounded-rect mask.
    let mut piece = Canvas::new(rect.size());
    piece.blit_region(
        backdrop,
        Rect::new(rect.x, rect.y, rect.w, rect.h),
        Point::ZERO,
    );
    let mut mask = Canvas::new(rect.size());
    mask.fill_rounded_rect(Rect::new(0, 0, rect.w, rect.h), radius, [255, 255, 255, 255]);
    let mask = mask.into_pixels();
    let mut px = piece.into_pixels();
    for (p, m) in px.data.chunks_exact_mut(4).zip(mask.data.chunks_exact(4)) {
        let a = u32::from(m[3]);
        for v in p.iter_mut() {
            *v = ((u32::from(*v) * a + 127) / 255) as u8;
        }
    }
    c.blit(&px, Point::new(rect.x, rect.y));
    c.fill_rounded_rect(rect, radius, tint);
    let _ = clip;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(size: Size, rgba: [u8; 4]) -> Pixels {
        Canvas::filled(size, rgba).into_pixels()
    }

    #[test]
    fn blur_keeps_a_flat_image_flat() {
        let src = solid(Size::new(64, 64), [200, 100, 50, 255]);
        let out = blurred_backdrop(&src);
        assert_eq!(out.size, src.size);
        for p in out.data.chunks(4) {
            assert!(p[0].abs_diff(200) <= 2 && p[1].abs_diff(100) <= 2 && p[3] == 255);
        }
    }

    #[test]
    fn blur_spreads_a_bright_spot() {
        let mut c = Canvas::filled(Size::new(160, 160), [0, 0, 0, 255]);
        c.fill_rect(Rect::new(76, 76, 8, 8), [255, 255, 255, 255]);
        let out = blurred_backdrop(&c.into_pixels());
        let at = |x: i32, y: i32| out.data[((y * 160 + x) * 4) as usize];
        assert!(at(80, 80) < 255, "peak is flattened");
        assert!(at(60, 80) > 0, "light bled sideways");
        assert_eq!(at(2, 2), 0, "far corner untouched");
    }

    #[test]
    fn frost_clips_to_the_rounded_corner_and_tints() {
        let backdrop = solid(Size::new(100, 100), [255, 0, 0, 255]);
        let mut c = Canvas::new(Size::new(100, 100));
        frost(&mut c, &backdrop, Rect::new(10, 10, 80, 80), 30.0, [255, 255, 255, 100]);
        let px = c.into_pixels();
        let a = |x: i32, y: i32| px.data[((y * 100 + x) * 4 + 3) as usize];
        assert_eq!(a(10, 10), 0, "corner is outside the rounded rect");
        assert_eq!(a(50, 50), 255, "inside is opaque glass");
        let i = ((50 * 100 + 50) * 4) as usize;
        assert!(px.data[i + 1] > 20, "white tint lifted the red backdrop");
    }
}
