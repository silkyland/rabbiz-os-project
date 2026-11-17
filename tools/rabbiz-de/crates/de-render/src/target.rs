use de_api::RendererKind;
use smithay::backend::allocator::gbm::GbmDevice;
use smithay::backend::drm::DrmDeviceFd;
use smithay::backend::renderer::element::texture::TextureBuffer;
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
use smithay::backend::renderer::pixman::{PixmanRenderer, PixmanTexture};
use smithay::backend::renderer::ImportDma;
use tiny_skia::Pixmap;

use crate::gl::create_gl_renderer;
use crate::soft::create_soft_renderer;
use crate::texture::upload_pixmap;

/// Active renderer for the session (chosen once at startup).
pub enum RenderTarget {
    Gl {
        renderer: GlesRenderer,
        scene_tex: Option<TextureBuffer<GlesTexture>>,
    },
    Soft {
        renderer: PixmanRenderer,
        scene_tex: Option<TextureBuffer<PixmanTexture>>,
    },
}

impl RenderTarget {
    pub fn new(kind: RendererKind, gbm: Option<GbmDevice<DrmDeviceFd>>) -> Result<Self, String> {
        match kind {
            RendererKind::Gl => {
                let gbm = gbm.ok_or_else(|| "GBM device required for GL renderer".to_string())?;
                let renderer = create_gl_renderer(gbm)?;
                Ok(Self::Gl {
                    renderer,
                    scene_tex: None,
                })
            }
            RendererKind::Soft => {
                let renderer = create_soft_renderer()?;
                Ok(Self::Soft {
                    renderer,
                    scene_tex: None,
                })
            }
        }
    }

    pub fn kind(&self) -> RendererKind {
        match self {
            Self::Gl { .. } => RendererKind::Gl,
            Self::Soft { .. } => RendererKind::Soft,
        }
    }

    pub fn dmabuf_formats(&self) -> Vec<smithay::backend::allocator::Format> {
        match self {
            Self::Gl { renderer, .. } => renderer.dmabuf_formats().into_iter().collect(),
            Self::Soft { renderer, .. } => renderer.dmabuf_formats().into_iter().collect(),
        }
    }

    pub fn upload_scene(&mut self, pixmap: &Pixmap) -> Result<u64, String> {
        let bytes = pixmap.data().len() as u64;
        match self {
            Self::Gl {
                renderer,
                scene_tex,
            } => {
                *scene_tex = Some(upload_pixmap(renderer, pixmap)?);
            }
            Self::Soft {
                renderer,
                scene_tex,
            } => {
                *scene_tex = Some(upload_pixmap(renderer, pixmap)?);
            }
        }
        Ok(bytes)
    }
}
