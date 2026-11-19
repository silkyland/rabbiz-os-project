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
/// Both horizontal and vertical passes stream rows sequentially so L1 cache
/// stays hot and there are no strided column jumps.
fn box_blur_pass(src: &[u8], w: i32, h: i32, r: i32, horizontal: bool) -> Vec<u8> {
    let mut out = vec![0u8; src.len()];
    let w_us = w as usize;
    let stride = w_us * 4;
    let norm = (2 * r + 1) as u32;
    let half = norm / 2;

    if horizontal {
        for y in 0..h as usize {
            let srow = &src[y * stride..(y + 1) * stride];
            let drow = &mut out[y * stride..(y + 1) * stride];
            let mut acc = [0u32; 4];
            for i in -r..=r {
                let x = i.clamp(0, w - 1) as usize * 4;
                for c in 0..4 {
                    acc[c] += u32::from(srow[x + c]);
                }
            }
            for x in 0..w_us {
                let o = x * 4;
                for c in 0..4 {
                    drow[o + c] = ((acc[c] + half) / norm) as u8;
                }
                let add_x = (x as i32 + r + 1).clamp(0, w - 1) as usize * 4;
                let sub_x = (x as i32 - r).clamp(0, w - 1) as usize * 4;
                for c in 0..4 {
                    acc[c] = acc[c] + u32::from(srow[add_x + c]) - u32::from(srow[sub_x + c]);
                }
            }
        }
    } else {
        let mut acc = vec![[0u32; 4]; w_us];
        for i in -r..=r {
            let sy = i.clamp(0, h - 1) as usize;
            let srow = &src[sy * stride..(sy + 1) * stride];
            for (x, a) in acc.iter_mut().enumerate() {
                let p = x * 4;
                for c in 0..4 {
                    a[c] += u32::from(srow[p + c]);
                }
            }
        }
        for y in 0..h as usize {
            let drow = &mut out[y * stride..(y + 1) * stride];
            for (x, a) in acc.iter().enumerate() {
                let o = x * 4;
                for c in 0..4 {
                    drow[o + c] = ((a[c] + half) / norm) as u8;
                }
            }
            let add_y = (y as i32 + r + 1).clamp(0, h - 1) as usize;
            let sub_y = (y as i32 - r).clamp(0, h - 1) as usize;
            let add_row = &src[add_y * stride..(add_y + 1) * stride];
            let sub_row = &src[sub_y * stride..(sub_y + 1) * stride];
            for (x, a) in acc.iter_mut().enumerate() {
                let p = x * 4;
                for c in 0..4 {
                    a[c] = a[c] + u32::from(add_row[p + c]) - u32::from(sub_row[p + c]);
                }
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
        (0..to)
            .map(|i| {
                let p = ((i as f32 + 0.5) * from as f32 / to as f32 - 0.5)
                    .clamp(0.0, (from - 1) as f32);
                let low = p.floor() as usize;
                (
                    low,
                    (low + 1).min(from as usize - 1),
                    ((p - low as f32) * 256.0) as u32,
                )
            })
            .collect()
    };
    let xs: Vec<(usize, usize, u32)> = axis(size.w, src.size.w)
        .into_iter()
        .map(|(a, b, f)| (a * 4, b * 4, f))
        .collect();
    let ys = axis(size.h, src.size.h);
    let mut out = Pixels::empty(size);
    let src_stride = src.size.w as usize * 4;
    let dst_stride = size.w as usize * 4;
    for (out_row, &(y0, y1, fy)) in out.data.chunks_exact_mut(dst_stride).zip(ys.iter()) {
        let row0 = &src.data[y0 * src_stride..(y0 + 1) * src_stride];
        let row1 = &src.data[y1 * src_stride..(y1 + 1) * src_stride];
        let inv_fy = 256 - fy;
        for (dst_px, &(x0, x1, fx)) in out_row.chunks_exact_mut(4).zip(xs.iter()) {
            let inv_fx = 256 - fx;
            let p00 = &row0[x0..x0 + 4];
            let p01 = &row0[x1..x1 + 4];
            let p10 = &row1[x0..x0 + 4];
            let p11 = &row1[x1..x1 + 4];
            for c in 0..4 {
                let a = u32::from(p00[c]) * inv_fx + u32::from(p01[c]) * fx;
                let b = u32::from(p10[c]) * inv_fx + u32::from(p11[c]) * fx;
                dst_px[c] = ((a * inv_fy + b * fy + 32768) >> 16) as u8;
            }
        }
    }
    out
}

#[inline]
fn frost_pixel(dst: &mut [u8], src: &[u8], tint: [u8; 4], ta: u32, inv_ta: u32, mask: u32) {
    let a = (ta + (u32::from(src[3]) * inv_ta + 127) / 255) * mask / 255;
    let inv_a = 255 - a;
    if mask == 255 {
        for c in 0..3 {
            let sample = (u32::from(tint[c]) * ta + u32::from(src[c]) * inv_ta + 127) / 255;
            dst[c] = (sample + (u32::from(dst[c]) * inv_a + 127) / 255).min(255) as u8;
        }
    } else {
        for c in 0..3 {
            let sample = (u32::from(tint[c]) * ta + u32::from(src[c]) * inv_ta + 127) / 255;
            let value = (sample * mask + 127) / 255;
            dst[c] = (value + (u32::from(dst[c]) * inv_a + 127) / 255).min(255) as u8;
        }
    }
    dst[3] = (a + (u32::from(dst[3]) * inv_a + 127) / 255).min(255) as u8;
}

/// Sample the cached blurred scene through a rounded surface in one pass.
/// No per-control crop, mask image, or intermediate full-size allocation.
/// The incoming pixels and destination both use premultiplied RGBA8.
pub fn frost(c: &mut Canvas, backdrop: &Pixels, rect: Rect, radius: f32, tint: [u8; 4]) {
    let bounds = Rect::new(
        0,
        0,
        c.size().w.min(backdrop.size.w),
        c.size().h.min(backdrop.size.h),
    );
    let Some(clip) = rect.intersect(bounds) else {
        return;
    };
    let radius = radius
        .max(0.0)
        .min(rect.w as f32 / 2.0)
        .min(rect.h as f32 / 2.0);
    let width = c.size().w as usize;
    let b_width = backdrop.size.w as usize;
    let target = c.pixmap_mut().data_mut();
    let ta = u32::from(tint[3]);
    let inv_ta = 255 - ta;
    let r_bound = radius.ceil() as i32 + 1;
    let left_corner_end = (rect.x + r_bound).min(clip.x + clip.w).max(clip.x);
    let right_corner_start = (rect.x + rect.w - r_bound)
        .max(left_corner_end)
        .min(clip.x + clip.w);

    for y in clip.y..clip.y + clip.h {
        let dy = (radius - (y - rect.y) as f32 - 0.5)
            .max(radius - (rect.y + rect.h - y) as f32 + 0.5)
            .max(0.0);
        let src_row = &backdrop.data[y as usize * b_width * 4..(y as usize + 1) * b_width * 4];
        let dst_row = &mut target[y as usize * width * 4..(y as usize + 1) * width * 4];
        if dy == 0.0 {
            for x in clip.x..clip.x + clip.w {
                let o = x as usize * 4;
                frost_pixel(
                    &mut dst_row[o..o + 4],
                    &src_row[o..o + 4],
                    tint,
                    ta,
                    inv_ta,
                    255,
                );
            }
            continue;
        }
        let corner_mask = |x: i32| -> u32 {
            let dx = (radius - (x - rect.x) as f32 - 0.5)
                .max(radius - (rect.x + rect.w - x) as f32 + 0.5)
                .max(0.0);
            if dx > 0.0 {
                ((radius + 0.5 - (dx * dx + dy * dy).sqrt()).clamp(0.0, 1.0) * 255.0).round() as u32
            } else {
                255
            }
        };
        for x in clip.x..left_corner_end {
            let mask = corner_mask(x);
            if mask == 0 {
                continue;
            }
            let o = x as usize * 4;
            frost_pixel(
                &mut dst_row[o..o + 4],
                &src_row[o..o + 4],
                tint,
                ta,
                inv_ta,
                mask,
            );
        }
        for x in left_corner_end..right_corner_start {
            let o = x as usize * 4;
            frost_pixel(
                &mut dst_row[o..o + 4],
                &src_row[o..o + 4],
                tint,
                ta,
                inv_ta,
                255,
            );
        }
        for x in right_corner_start..clip.x + clip.w {
            let mask = corner_mask(x);
            if mask == 0 {
                continue;
            }
            let o = x as usize * 4;
            frost_pixel(
                &mut dst_row[o..o + 4],
                &src_row[o..o + 4],
                tint,
                ta,
                inv_ta,
                mask,
            );
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
        let backdrop = Pixels::new(Size::new(16, 16), [128, 0, 0, 128].repeat(16 * 16));
        let mut c = Canvas::new(Size::new(16, 16));
        frost(
            &mut c,
            &backdrop,
            Rect::new(-4, -4, 12, 12),
            0.0,
            [0, 0, 0, 0],
        );
        let px = c.into_pixels();
        assert_eq!(&px.data[0..4], &[128, 0, 0, 128]);
        assert_eq!(
            &px.data[(15 * 16 + 15) * 4..(15 * 16 + 15) * 4 + 4],
            &[0, 0, 0, 0]
        );
    }

    #[test]
    fn upsampling_preserves_flat_translucent_pixels() {
        let src = Pixels::new(Size::new(3, 5), [128, 64, 0, 128].repeat(15));
        let out = upsample(&src, Size::new(19, 23));
        for p in out.data.chunks_exact(4) {
            assert_eq!(p, &[128, 64, 0, 128]);
        }
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
        frost(
            &mut c,
            &backdrop,
            Rect::new(10, 10, 80, 80),
            30.0,
            [255, 255, 255, 100],
        );
        let px = c.into_pixels();
        let a = |x: i32, y: i32| px.data[((y * 100 + x) * 4 + 3) as usize];
        assert_eq!(a(10, 10), 0, "corner is outside the rounded rect");
        assert_eq!(a(50, 50), 255, "inside is opaque glass");
        let i = ((50 * 100 + 50) * 4) as usize;
        assert!(px.data[i + 1] > 20, "white tint lifted the red backdrop");
    }
}
