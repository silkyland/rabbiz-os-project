//! Texture buffers uploaded from `de-api` premultiplied [`Pixels`].

use de_api::Pixels;
use smithay::backend::allocator::Fourcc as DrmFourcc;
use smithay::backend::renderer::element::texture::TextureBuffer;
use smithay::backend::renderer::gles::GlesTexture;
use smithay::backend::renderer::pixman::PixmanTexture;
use smithay::utils::{Size, Transform};

use crate::target::{DeRenderer, RenderError};

/// An uploaded texture, tagged by renderer kind.
pub enum DeTexture {
    Gl(TextureBuffer<GlesTexture>),
    Soft(TextureBuffer<PixmanTexture>),
}

/// Upload `px` (premultiplied RGBA8) into the active renderer.
///
/// Wayland/DRM buffers are ARGB little-endian == `BGRA` byte order on the
/// wire; `de-api` Pixels are RGBA, so we use `Abgr8888`.
pub fn upload(renderer: &mut DeRenderer, px: &Pixels) -> Result<DeTexture, RenderError> {
    let size = Size::from((px.size.w, px.size.h));
    match renderer {
        DeRenderer::Gl(r) => {
            let buf = TextureBuffer::from_memory(
                r,
                &px.data,
                DrmFourcc::Abgr8888,
                size,
                false,
                1,
                Transform::Normal,
                None,
            )
            .map_err(|e| RenderError::Upload(format!("{e}")))?;
            Ok(DeTexture::Gl(buf))
        }
        DeRenderer::Soft(r) => {
            let buf = TextureBuffer::from_memory(
                r,
                &px.data,
                DrmFourcc::Abgr8888,
                size,
                false,
                1,
                Transform::Normal,
                None,
            )
            .map_err(|e| RenderError::Upload(format!("{e}")))?;
            Ok(DeTexture::Soft(buf))
        }
    }
}

/// Upload a raw RGBA buffer (already premultiplied) — same as [`upload`] but
/// without wrapping in `Pixels` (used by the probe's solid test texture).
pub fn upload_rgba(
    renderer: &mut DeRenderer,
    data: &[u8],
    w: i32,
    h: i32,
) -> Result<DeTexture, RenderError> {
    upload(
        renderer,
        &Pixels {
            size: de_api::Size::new(w, h),
            data: data.to_vec(),
        },
    )
}
