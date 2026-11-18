//! GLES2 renderer bring-up on the GBM/EGL display.

use smithay::backend::egl::{EGLContext, EGLDisplay};
use smithay::backend::renderer::gles::GlesRenderer;

/// Errors creating the GL renderer.
#[derive(Debug)]
pub struct GlError(pub String);

impl std::fmt::Display for GlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for GlError {}

/// Create a `GlesRenderer` on `display` (default config + priority).
#[allow(unsafe_code)] // smithay `GlesRenderer::new` is an unsafe FFI ctor
pub fn create(display: &EGLDisplay) -> Result<GlesRenderer, GlError> {
    let ctx = EGLContext::new_with_priority(
        display,
        smithay::backend::egl::context::ContextPriority::Medium,
    )
    .map_err(|e| GlError(format!("egl context: {e}")))?;
    // SAFETY: `ctx` outlives the renderer inside `DeRenderer::Gl`.
    unsafe { GlesRenderer::new(ctx) }.map_err(|e| GlError(format!("gles: {e}")))
}
