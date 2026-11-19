//! Software-rasterized pointer cursor sprite (moved out of `frame.rs`).

/// Cursor bitmap edge length in logical px.
pub(crate) const CURSOR_PX: usize = 32;
/// Hotspot (the arrow tip) in texture px — the top-left corner of the glyph.
pub(crate) const CURSOR_HOTSPOT: (f64, f64) = (3.0, 3.0);

/// Rasterize the shell cursor once: a classic arrow pointer, dark outline
/// with a light fill so it reads on both light and dark shells. Premultiplied
/// RGBA like every `de_api::Pixels` buffer.
pub(crate) fn cursor_pixels() -> de_api::Pixels {
    let mut px = de_api::Pixels::empty(de_api::Size::new(CURSOR_PX as i32, CURSOR_PX as i32));
    // Arrow outline (slightly larger) then the fill shrunk toward the tip.
    let outline = [
        (2.0f32, 2.0f32),
        (2.0, 25.0),
        (9.0, 19.0),
        (13.0, 29.0),
        (16.0, 27.0),
        (12.0, 17.0),
        (21.0, 17.0),
    ];
    let fill = outline.map(|(x, y)| {
        let dx = x - CURSOR_HOTSPOT.0 as f32;
        let dy = y - CURSOR_HOTSPOT.1 as f32;
        (
            CURSOR_HOTSPOT.0 as f32 + dx * 0.82,
            CURSOR_HOTSPOT.1 as f32 + dy * 0.82,
        )
    });
    fill_poly(&mut px, &outline, [18, 16, 22, 255]);
    fill_poly(&mut px, &fill, [244, 244, 246, 255]);
    px
}

/// Scanline-fill a convex-ish polygon into premultiplied RGBA pixels.
fn fill_poly(px: &mut de_api::Pixels, poly: &[(f32, f32)], color: [u8; 4]) {
    let (w, h) = (px.size.w.max(1) as usize, px.size.h.max(1) as usize);
    let n = poly.len();
    let min_y = poly
        .iter()
        .map(|p| p.1)
        .fold(f32::INFINITY, f32::min)
        .floor() as i32;
    let max_y = poly
        .iter()
        .map(|p| p.1)
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil() as i32;
    for y in min_y.max(0)..max_y.min(h as i32 - 1) {
        let yf = y as f32 + 0.5;
        let mut xs: Vec<f32> = Vec::with_capacity(8);
        for i in 0..n {
            let (x1, y1) = poly[i];
            let (x2, y2) = poly[(i + 1) % n];
            if (y1 <= yf) != (y2 <= yf) && y2 != y1 {
                xs.push(x1 + (yf - y1) * (x2 - x1) / (y2 - y1));
            }
        }
        xs.sort_by(f32::total_cmp);
        for pair in xs.chunks_exact(2) {
            let x0 = pair[0].floor().max(0.0) as usize;
            let x1 = (pair[1].ceil() as usize).min(w);
            for x in x0..x1 {
                let i = (y as usize * w + x) * 4;
                // Straight-alpha fill; the buffer is treated as
                // premultiplied and the colors are already opaque-ish.
                px.data[i..i + 4].copy_from_slice(&color);
            }
        }
    }
}
