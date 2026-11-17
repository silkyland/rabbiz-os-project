use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::texture::{TextureBuffer, TextureRenderElement};
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::{ImportMem, Renderer, Texture};
use smithay::utils::{Buffer as BufferCoords, Size, Transform};
use tiny_skia::Pixmap;

/// Upload a premultiplied RGBA8 `tiny_skia::Pixmap` to the renderer as a `TextureBuffer`.
pub fn upload_pixmap<R>(
    renderer: &mut R,
    pixmap: &Pixmap,
) -> Result<TextureBuffer<R::TextureId>, String>
where
    R: Renderer + ImportMem,
    R::TextureId: Texture + Clone + 'static,
{
    let w = pixmap.width() as i32;
    let h = pixmap.height() as i32;
    let size: Size<i32, BufferCoords> = (w, h).into();

    let supported: Vec<Fourcc> = renderer.mem_formats().collect();
    let texture = if supported.contains(&Fourcc::Abgr8888) {
        renderer
            .import_memory(pixmap.data(), Fourcc::Abgr8888, size, false)
            .map_err(|err| format!("import_memory(ABGR8888) failed: {err}"))?
    } else if supported.contains(&Fourcc::Argb8888) {
        let mut bgra = pixmap.data().to_vec();
        for px in bgra.chunks_exact_mut(4) {
            px.swap(0, 2);
        }
        renderer
            .import_memory(&bgra, Fourcc::Argb8888, size, false)
            .map_err(|err| format!("import_memory(ARGB8888) failed: {err}"))?
    } else {
        return Err(format!(
            "renderer supports neither ABGR8888 nor ARGB8888 (supported: {supported:?})"
        ));
    };

    Ok(TextureBuffer::from_texture(
        renderer,
        texture,
        1,
        Transform::Normal,
        None,
    ))
}

/// Wrap an uploaded `TextureBuffer` in a `TextureRenderElement` positioned at `(0, 0)`.
pub fn texture_element<T: Texture + Clone + 'static>(
    buffer: &TextureBuffer<T>,
) -> TextureRenderElement<T> {
    TextureRenderElement::from_texture_buffer(
        (0.0, 0.0),
        buffer,
        None,
        None,
        None,
        Kind::Unspecified,
    )
}
