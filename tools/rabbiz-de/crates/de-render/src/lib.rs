//! `de-render` — one scene, two renderers (plan Decision 2).
//!
//! `Gl` = `GlesRenderer` on a GBM/EGL display; `Soft` = `PixmanRenderer`.
//! Both consume premultiplied RGBA8 [`Pixels`](de_api::Pixels) uploaded via
//! [`DeRenderer::upload`] and composite the same [`DeElement`]s.

pub mod blur;
pub mod gl;
pub mod soft;
pub mod target;
pub mod texture;

pub use target::{texture_element, DeRenderer, RenderError};
pub use texture::{upload, upload_rgba, DeTexture};
