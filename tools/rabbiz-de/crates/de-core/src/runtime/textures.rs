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
