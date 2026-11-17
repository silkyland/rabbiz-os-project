use smithay::backend::allocator::gbm::GbmDevice;
use smithay::backend::drm::DrmDeviceFd;
use smithay::backend::egl::{EGLContext, EGLDisplay};
use smithay::backend::renderer::gles::GlesRenderer;

/// Create a `GlesRenderer` bound to the given GBM device.
pub fn create_gl_renderer(gbm: GbmDevice<DrmDeviceFd>) -> Result<GlesRenderer, String> {
    let display =
        unsafe { EGLDisplay::new(gbm) }.map_err(|err| format!("EGLDisplay::new failed: {err}"))?;
    let context =
        EGLContext::new(&display).map_err(|err| format!("EGLContext::new failed: {err}"))?;
    unsafe { GlesRenderer::new(context) }.map_err(|err| format!("GlesRenderer::new failed: {err}"))
}
