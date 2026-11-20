//! Renderer-agnostic texture upload, lookup, and home/drag texture sync helpers
//! extracted from `runtime/frame.rs`.

use smithay::backend::renderer::element::texture::TextureRenderElement;
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::{ImportAll, ImportMem};
use smithay::utils::{Physical, Point};

/// Upload `px` into `renderer` as a `TextureBuffer`, boxed for storage.
pub(crate) fn upload_any<R: ImportMem>(
    r: &mut R,
    px: &de_api::Pixels,
) -> Option<Box<dyn std::any::Any>>
where
    R::TextureId: 'static,
{
    use smithay::backend::renderer::element::texture::TextureBuffer;
    TextureBuffer::from_memory(
        r,
        &px.data,
        smithay::backend::allocator::Fourcc::Abgr8888,
        smithay::utils::Size::from((px.size.w, px.size.h)),
        false,
        1,
        smithay::utils::Transform::Normal,
        None,
    )
    .ok()
    .map(|b| Box::new(b) as Box<dyn std::any::Any>)
}

/// Downcast a stored boxed texture back to the active renderer's `TextureBuffer`.
pub(crate) fn texture_buffer<R: ImportAll>(
    tex: Option<&Box<dyn std::any::Any>>,
) -> Option<&smithay::backend::renderer::element::texture::TextureBuffer<R::TextureId>>
where
    R::TextureId: 'static,
{
    // The stored texture was uploaded by the active renderer, so the
    // TextureId type always matches R.
    tex?.downcast_ref()
}

/// Build an unscaled texture render element at `loc`.
pub(crate) fn tex_el<R: ImportAll>(
    buf: &smithay::backend::renderer::element::texture::TextureBuffer<R::TextureId>,
    loc: Point<f64, Physical>,
) -> TextureRenderElement<R::TextureId>
where
    R::TextureId: smithay::backend::renderer::Texture + Clone + 'static,
{
    TextureRenderElement::from_texture_buffer(loc, buf, None, None, None, Kind::Unspecified)
}

/// A texture scaled to fill `rect` (the landing highlight spans icons of
/// different sizes).
pub(crate) fn tex_el_sized<R: ImportAll>(
    buf: &smithay::backend::renderer::element::texture::TextureBuffer<R::TextureId>,
    rect: de_api::Rect,
) -> TextureRenderElement<R::TextureId>
where
    R::TextureId: smithay::backend::renderer::Texture + Clone + 'static,
{
    TextureRenderElement::from_texture_buffer(
        Point::<f64, Physical>::from((f64::from(rect.x), f64::from(rect.y))),
        buf,
        None,
        None,
        Some(smithay::utils::Size::<i32, smithay::utils::Logical>::from(
            (rect.w, rect.h),
        )),
        Kind::Unspecified,
    )
}

/// Upload the lifted icon (and the shared highlight) once per drag.
pub(crate) fn sync_drag_textures<R: ImportMem>(r: &mut R, h: &mut crate::home_ui::HomeUi)
where
    R::TextureId: 'static,
{
    if h.drag_tex.is_none() {
        if let Some(d) = h.drag.as_ref() {
            h.drag_tex = upload_any(r, &d.px);
        }
    }
    if h.hl_tex.is_none() {
        let t = de_design::tokens::DEFAULT;
        let mut c = de_paint::Canvas::new(de_api::Size::new(112, 112));
        let mut col = t.accent;
        col[3] = 70;
        c.fill_rounded_rect(
            de_api::Rect::new(0, 0, 112, 112),
            de_design::tokens::ICON_TILE_RADIUS,
            col,
        );
        h.hl_tex = upload_any(r, &c.into_pixels());
    }
}

/// Upload the home layers that have no texture yet (drops stale ones when
/// the layer set was rebuilt).
pub(crate) fn sync_home_textures<R: ImportMem>(r: &mut R, h: &mut crate::home_ui::HomeUi)
where
    R::TextureId: 'static,
{
    let Some(key) = h.layers_key() else { return };
    if h.tex_key != key || h.base_tex.is_none() {
        h.tex_key = key;
        h.base_tex = None;
        h.page_tex.clear();
        h.dots_tex.clear();
    }
    let Some(layers) = h.layers.as_ref() else {
        return;
    };
    if h.base_tex.is_none() {
        h.base_tex = upload_any(r, &layers.base);
    }
    for (p, px) in &layers.pages {
        if !h.page_tex.contains_key(p) {
            if let Some(t) = upload_any(r, px) {
                h.page_tex.insert(*p, t);
            }
        }
    }
    for (p, px) in &layers.dots {
        if !h.dots_tex.contains_key(p) {
            if let Some(t) = upload_any(r, px) {
                h.dots_tex.insert(*p, t);
            }
        }
    }
}

/// Planned GPU texture upload strategy for a surface frame.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UploadStrategy {
    /// Surface is empty or has no in-bounds damage; nothing to upload.
    Skip,
    /// Combined clamped damage is strictly less than 50% of the surface area.
    Partial(Vec<de_api::Rect>),
    /// Full-surface upload (dimensions changed or damage covers >= 50% of area).
    Full,
}

/// Clamp `rect` to `[0..size.w, 0..size.h]` using `i64` arithmetic to avoid
/// overflow, returning `None` if either `size` or the clipped rect is empty.
fn clamp_rect_to_size(rect: de_api::Rect, size: de_api::Size) -> Option<de_api::Rect> {
    if size.w <= 0 || size.h <= 0 || rect.is_empty() {
        return None;
    }
    let x0 = i64::from(rect.x).max(0);
    let y0 = i64::from(rect.y).max(0);
    let x1 = (i64::from(rect.x) + i64::from(rect.w)).min(i64::from(size.w));
    let y1 = (i64::from(rect.y) + i64::from(rect.h)).min(i64::from(size.h));
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    Some(de_api::Rect::new(
        x0 as i32,
        y0 as i32,
        (x1 - x0) as i32,
        (y1 - y0) as i32,
    ))
}

/// Decide whether to skip, partially update, or fully re-upload a texture.
///
/// - Empty surface (`w <= 0 || h <= 0`) -> [`UploadStrategy::Skip`].
/// - `size_changed` -> [`UploadStrategy::Full`].
/// - Each damage rect is clamped to `[0..surface.w, 0..surface.h]` and empty
///   rects are dropped; if none remain -> [`UploadStrategy::Skip`].
/// - If combined clamped damage area is at least 50% of `surface` area
///   (`damaged_area * 2 >= surface_area`) -> [`UploadStrategy::Full`],
///   otherwise [`UploadStrategy::Partial`] with the clamped rects.
#[allow(dead_code)]
#[must_use]
pub fn plan_upload(
    surface: de_api::Size,
    damage: &[de_api::Rect],
    size_changed: bool,
) -> UploadStrategy {
    if surface.w <= 0 || surface.h <= 0 {
        return UploadStrategy::Skip;
    }
    if size_changed {
        return UploadStrategy::Full;
    }

    let mut clamped = Vec::with_capacity(damage.len());
    let mut damaged_area: u128 = 0;
    for &rect in damage {
        if let Some(clipped) = clamp_rect_to_size(rect, surface) {
            let area = u128::from(clipped.w as u64) * u128::from(clipped.h as u64);
            damaged_area = damaged_area.saturating_add(area);
            clamped.push(clipped);
        }
    }

    if clamped.is_empty() {
        return UploadStrategy::Skip;
    }

    let surface_area = u128::from(surface.w as u64) * u128::from(surface.h as u64);
    if damaged_area.saturating_mul(2) >= surface_area {
        UploadStrategy::Full
    } else {
        UploadStrategy::Partial(clamped)
    }
}

/// Copy only the pixels inside `rect` (clamped to `pixels.size`) into a tightly
/// packed RGBA8 byte buffer (`clamped.w * clamped.h * 4` bytes).
///
/// The tightly packed rows are ready for `glTexSubImage2D` on both GLES2
/// (without `GL_UNPACK_ROW_LENGTH`) and GLES3. Returns `None` if the clamped
/// rect is empty or `pixels.data` is smaller than `4 * w * h`.
#[allow(dead_code)]
#[must_use]
pub fn pack_subrect_rgba(
    pixels: &de_api::Pixels,
    rect: de_api::Rect,
) -> Option<(de_api::Rect, Vec<u8>)> {
    let clamped = clamp_rect_to_size(rect, pixels.size)?;
    let full_w = usize::try_from(pixels.size.w).ok()?;
    let full_h = usize::try_from(pixels.size.h).ok()?;
    let row_stride = full_w.checked_mul(4)?;
    let expected_bytes = row_stride.checked_mul(full_h)?;
    if pixels.data.len() < expected_bytes {
        return None;
    }

    let x = usize::try_from(clamped.x).ok()?;
    let y = usize::try_from(clamped.y).ok()?;
    let w = usize::try_from(clamped.w).ok()?;
    let h = usize::try_from(clamped.h).ok()?;
    let copy_row_bytes = w.checked_mul(4)?;
    let out_bytes = copy_row_bytes.checked_mul(h)?;

    let mut out = Vec::with_capacity(out_bytes);
    for row in y..(y + h) {
        let start = row * row_stride + x * 4;
        let end = start + copy_row_bytes;
        out.extend_from_slice(&pixels.data[start..end]);
    }
    Some((clamped, out))
}

/// Upload or incrementally update a persistent `TextureRenderBuffer` using
/// [`plan_upload`] and Smithay's [`ImportMem::update_memory`] (`glTexSubImage2D`
/// on `GlesRenderer`, clipped blit on `PixmanRenderer`).
///
/// - `UploadStrategy::Skip`: no GPU upload is performed.
/// - `UploadStrategy::Partial`: updates each clamped rect in place via
///   `TextureRenderBuffer::update_from_memory` (which expects the full-surface
///   `px.data` slice and handles `GL_UNPACK_ROW_LENGTH` / skip offsets inside
///   Smithay). Falls back cleanly to a full upload if any sub-rect update fails.
/// - `UploadStrategy::Full` (or missing `slot`): allocates a fresh texture from
///   `px.data`.
#[allow(dead_code)]
pub(crate) fn upload_pixels_damaged<R: ImportMem>(
    r: &mut R,
    slot: &mut Option<
        smithay::backend::renderer::element::texture::TextureRenderBuffer<R::TextureId>,
    >,
    px: &de_api::Pixels,
    damage: &[de_api::Rect],
    size_changed: bool,
) -> UploadStrategy {
    use smithay::backend::allocator::Fourcc;
    use smithay::backend::renderer::element::texture::TextureRenderBuffer;
    use smithay::utils::{Buffer, Rectangle, Size, Transform};

    let strategy = plan_upload(px.size, damage, size_changed || slot.is_none());
    match strategy {
        UploadStrategy::Skip => UploadStrategy::Skip,
        UploadStrategy::Partial(rects) => {
            if let Some(buf) = slot.as_mut() {
                let mut all_ok = true;
                for rect in &rects {
                    let region = Rectangle::<i32, Buffer>::new(
                        (rect.x, rect.y).into(),
                        (rect.w, rect.h).into(),
                    );
                    if buf.update_from_memory(r, &px.data, region, None).is_err() {
                        all_ok = false;
                        break;
                    }
                }
                if all_ok {
                    return UploadStrategy::Partial(rects);
                }
            }
            if let Ok(fresh) = TextureRenderBuffer::from_memory(
                r,
                &px.data,
                Fourcc::Abgr8888,
                Size::from((px.size.w, px.size.h)),
                false,
                1,
                Transform::Normal,
                None,
            ) {
                *slot = Some(fresh);
                UploadStrategy::Full
            } else {
                UploadStrategy::Skip
            }
        }
        UploadStrategy::Full => {
            if let Ok(fresh) = TextureRenderBuffer::from_memory(
                r,
                &px.data,
                Fourcc::Abgr8888,
                Size::from((px.size.w, px.size.h)),
                false,
                1,
                Transform::Normal,
                None,
            ) {
                *slot = Some(fresh);
                UploadStrategy::Full
            } else {
                UploadStrategy::Skip
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_upload_skips_empty_damage() {
        let surface = de_api::Size::new(720, 1600);
        assert_eq!(plan_upload(surface, &[], false), UploadStrategy::Skip);
        assert_eq!(
            plan_upload(
                surface,
                &[
                    de_api::Rect::new(10, 10, 0, 50),
                    de_api::Rect::new(10, 10, 50, -5),
                    de_api::Rect::new(720, 0, 40, 40),
                    de_api::Rect::new(-50, -50, 50, 50),
                ],
                false,
            ),
            UploadStrategy::Skip
        );
        // Empty surface always skips, even on resize.
        assert_eq!(
            plan_upload(
                de_api::Size::new(0, 100),
                &[de_api::Rect::new(0, 0, 10, 10)],
                true
            ),
            UploadStrategy::Skip
        );
        assert_eq!(
            plan_upload(
                de_api::Size::new(100, -1),
                &[de_api::Rect::new(0, 0, 10, 10)],
                true
            ),
            UploadStrategy::Skip
        );
    }

    #[test]
    fn plan_upload_returns_full_on_resize() {
        let surface = de_api::Size::new(720, 1600);
        assert_eq!(plan_upload(surface, &[], true), UploadStrategy::Full);
        assert_eq!(
            plan_upload(surface, &[de_api::Rect::new(0, 0, 10, 10)], true),
            UploadStrategy::Full
        );
    }

    #[test]
    fn plan_upload_returns_partial_under_half_area_and_clamps() {
        let surface = de_api::Size::new(100, 100);
        let damage = [
            de_api::Rect::new(-10, -5, 30, 25), // clamps to (0, 0, 20, 20) -> area 400
            de_api::Rect::new(90, 80, 50, 50),  // clamps to (90, 80, 10, 20) -> area 200
            de_api::Rect::new(200, 200, 10, 10), // out of bounds -> dropped
        ];
        assert_eq!(
            plan_upload(surface, &damage, false),
            UploadStrategy::Partial(vec![
                de_api::Rect::new(0, 0, 20, 20),
                de_api::Rect::new(90, 80, 10, 20),
            ])
        );
    }

    #[test]
    fn plan_upload_promotes_to_full_at_fifty_percent_threshold() {
        let surface = de_api::Size::new(100, 100);
        // 49% -> Partial
        assert_eq!(
            plan_upload(surface, &[de_api::Rect::new(0, 0, 100, 49)], false),
            UploadStrategy::Partial(vec![de_api::Rect::new(0, 0, 100, 49)])
        );
        // Exactly 50% -> Full
        assert_eq!(
            plan_upload(surface, &[de_api::Rect::new(0, 0, 100, 50)], false),
            UploadStrategy::Full
        );
        // Two rects summing to 50% -> Full
        assert_eq!(
            plan_upload(
                surface,
                &[
                    de_api::Rect::new(0, 0, 100, 25),
                    de_api::Rect::new(0, 50, 100, 25),
                ],
                false,
            ),
            UploadStrategy::Full
        );
    }

    #[test]
    fn pack_subrect_rgba_extracts_exact_subregion_bytes() {
        let w = 4_i32;
        let h = 4_i32;
        let mut data = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                data.extend_from_slice(&[x as u8, y as u8, (x + y) as u8, 255]);
            }
        }
        let px = de_api::Pixels::new(de_api::Size::new(w, h), data);

        // Interior 2x2 rect at (1, 1).
        let (rect, packed) =
            pack_subrect_rgba(&px, de_api::Rect::new(1, 1, 2, 2)).expect("valid subrect");
        assert_eq!(rect, de_api::Rect::new(1, 1, 2, 2));
        assert_eq!(
            packed,
            vec![
                1, 1, 2, 255, 2, 1, 3, 255, // row y=1, x=1..3
                1, 2, 3, 255, 2, 2, 4, 255, // row y=2, x=1..3
            ]
        );

        // Overhanging rect clamps to (0, 2, 2, 2).
        let (rect, packed) =
            pack_subrect_rgba(&px, de_api::Rect::new(-1, 2, 3, 5)).expect("clamped subrect");
        assert_eq!(rect, de_api::Rect::new(0, 2, 2, 2));
        assert_eq!(
            packed,
            vec![
                0, 2, 2, 255, 1, 2, 3, 255, // row y=2, x=0..2
                0, 3, 3, 255, 1, 3, 4, 255, // row y=3, x=0..2
            ]
        );

        // Empty / out-of-bounds / short buffer return None.
        assert_eq!(pack_subrect_rgba(&px, de_api::Rect::new(4, 0, 2, 2)), None);
        assert_eq!(pack_subrect_rgba(&px, de_api::Rect::new(0, 0, 0, 2)), None);
        let short = de_api::Pixels {
            size: de_api::Size::new(4, 4),
            data: vec![0; 12],
        };
        assert_eq!(
            pack_subrect_rgba(&short, de_api::Rect::new(0, 0, 2, 2)),
            None
        );
    }
}
