//! Frosted ("matte glass") surfaces without a live blur.
//!
//! A live backdrop blur costs a full-screen convolution per frame, which the
//! A21s' CPU/GPU budget cannot afford (docs/DE-DESIGN-LIGHT-TOKENS.md open
//! item 3). Instead the backdrop (wallpaper or the last app frame) is blurred
//! **once**, cached, and every glass element samples that blurred copy under
//! a translucent tint. It looks like frosted glass, costs one blit per
//! element, and has no shadow: matte, as the owner asked.

use de_api::{Pixels, Rect, Size};

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
    upsample(&blurred, src.size)

}

/// Fixed-point bilinear upsampling of an already blurred image. Avoid a
/// general affine raster pipeline for this full-screen, axis-aligned copy.
fn upsample(src: &Pixels, size: Size) -> Pixels {
    let axis = |to: i32, from: i32| -> Vec<(usize, usize, u32)> {
        (0..to).map(|i| {
            let p = ((i as f32 + 0.5) * from as f32 / to as f32 - 0.5)
                .clamp(0.0, (from - 1) as f32);
            let low = p.floor() as usize;
            (low, (low + 1).min(from as usize - 1), ((p - low as f32) * 256.0) as u32)
        }).collect()
    };
    let xs = axis(size.w, src.size.w);
    let ys = axis(size.h, src.size.h);
    let mut out = Pixels::empty(size);
    let stride = src.size.w as usize * 4;
    for (y, &(y0, y1, fy)) in ys.iter().enumerate() {
        for (x, &(x0, x1, fx)) in xs.iter().enumerate() {
            let dest = (y * size.w as usize + x) * 4;
            for c in 0..4 {
                let a = u32::from(src.data[y0*stride+x0*4+c]) * (256-fx)
                    + u32::from(src.data[y0*stride+x1*4+c]) * fx;
                let b = u32::from(src.data[y1*stride+x0*4+c]) * (256-fx)
                    + u32::from(src.data[y1*stride+x1*4+c]) * fx;
                out.data[dest+c] = ((a*(256-fy)+b*fy+32768)>>16) as u8;
            }
        }
    }
    out
}

/// Sample the cached blurred scene through a rounded surface in one pass.
/// No per-control crop, mask image, or intermediate full-size allocation.
/// The incoming pixels and destination both use premultiplied RGBA8.
pub fn frost(c: &mut Canvas, backdrop: &Pixels, rect: Rect, radius: f32, tint: [u8; 4]) {
    let bounds = Rect::new(0, 0, c.size().w.min(backdrop.size.w), c.size().h.min(backdrop.size.h));
    let Some(clip) = rect.intersect(bounds) else { return; };
    let radius = radius.max(0.0).min(rect.w as f32 / 2.0).min(rect.h as f32 / 2.0);
    let width = c.size().w as usize;
    let target = c.pixmap_mut().data_mut();
    let ta = u32::from(tint[3]);
    for y in clip.y..clip.y+clip.h {
        for x in clip.x..clip.x+clip.w {
            let dx = (radius - (x-rect.x) as f32 - 0.5)
                .max(radius - (rect.x+rect.w-x) as f32 + 0.5).max(0.0);
            let dy = (radius - (y-rect.y) as f32 - 0.5)
                .max(radius - (rect.y+rect.h-y) as f32 + 0.5).max(0.0);
            let mask = if dx > 0.0 && dy > 0.0 {
                ((radius + 0.5 - (dx*dx+dy*dy).sqrt()).clamp(0.0, 1.0)*255.0).round() as u32
            } else { 255 };
            if mask == 0 { continue; }
            let src = ((y*backdrop.size.w+x)*4) as usize;
            let dest = (y as usize*width+x as usize)*4;
            let a = (ta + (u32::from(backdrop.data[src+3])*(255-ta)+127)/255)*mask/255;
            for channel in 0..4 {
                let value = if channel == 3 { a } else {
                    let sample = (u32::from(tint[channel])*ta
                        + u32::from(backdrop.data[src+channel])*(255-ta)+127)/255;
                    (sample*mask+127)/255
                };
                target[dest+channel] = (value + (u32::from(target[dest+channel])*(255-a)+127)/255).min(255) as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(size: Size, rgba: [u8; 4]) -> Pixels {
        Canvas::filled(size, rgba).into_pixels()
    }

    #[test]
    fn frost_preserves_premultiplied_alpha_and_clips_negative_bounds() {
        let backdrop = Pixels::new(Size::new(16, 16), [128, 0, 0, 128].repeat(16*16));
        let mut c = Canvas::new(Size::new(16, 16));
        frost(&mut c, &backdrop, Rect::new(-4, -4, 12, 12), 0.0, [0, 0, 0, 0]);
        let px = c.into_pixels();
        assert_eq!(&px.data[0..4], &[128, 0, 0, 128]);
        assert_eq!(&px.data[(15*16+15)*4..(15*16+15)*4+4], &[0, 0, 0, 0]);
    }

    #[test]
    fn upsampling_preserves_flat_translucent_pixels() {
        let src = Pixels::new(Size::new(3, 5), [128, 64, 0, 128].repeat(15));
        let out = upsample(&src, Size::new(19, 23));
        for p in out.data.chunks_exact(4) { assert_eq!(p, &[128, 64, 0, 128]); }
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
