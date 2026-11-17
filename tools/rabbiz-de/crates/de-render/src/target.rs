//! `DeRenderer` — the single-enum renderer the compositor uses per run.

use de_api::RendererKind;
use smithay::backend::allocator::Format as DrmFormat;
use smithay::backend::allocator::Fourcc as DrmFourcc;
use smithay::backend::allocator::Modifier as DrmModifier;
use smithay::backend::renderer::element::texture::{TextureBuffer, TextureRenderElement};
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::renderer::pixman::PixmanRenderer;
use smithay::backend::renderer::{Renderer, Texture};
use smithay::utils::{Physical, Point};

/// Errors at the renderer boundary.
#[derive(Debug)]
pub enum RenderError {
    /// EGL/GL initialization or context failure.
    Gl(String),
    /// Software renderer failure.
    Soft(String),
    /// Texture upload failure.
    Upload(String),
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Gl(e) => write!(f, "gl: {e}"),
            Self::Soft(e) => write!(f, "soft: {e}"),
            Self::Upload(e) => write!(f, "upload: {e}"),
        }
    }
}

impl std::error::Error for RenderError {}

/// The active renderer for a run (chosen by `--renderer`/`auto`).
pub enum DeRenderer {
    Gl(GlesRenderer),
    Soft(PixmanRenderer),
}

impl DeRenderer {
    #[must_use]
    pub fn kind(&self) -> RendererKind {
        match self {
            Self::Gl(_) => RendererKind::Gl,
            Self::Soft(_) => RendererKind::Soft,
        }
    }

    /// Renderer-supported dmabuf formats (for `DrmCompositor::new`'s
    /// `renderer_formats` argument; the compositor intersects them with the
    /// primary plane's formats).
    #[must_use]
    pub fn renderer_formats(&self) -> Vec<DrmFormat> {
        match self {
            Self::Gl(r) => r
                .egl_context()
                .dmabuf_render_formats()
                .iter()
                .copied()
                .collect(),
            Self::Soft(_) => [DrmFourcc::Argb8888, DrmFourcc::Xrgb8888]
                .into_iter()
                .map(|code| DrmFormat {
                    code,
                    modifier: DrmModifier::Linear,
                })
                .collect(),
        }
    }
}

/// Build a textured element at `location` from `buffer`.
pub fn texture_element<R>(
    buffer: &TextureBuffer<R::TextureId>,
    location: Point<f64, Physical>,
) -> TextureRenderElement<R::TextureId>
where
    R: Renderer,
    R::TextureId: Texture + Clone + 'static,
{
    TextureRenderElement::from_texture_buffer(location, buffer, None, None, None, Kind::Unspecified)
}
