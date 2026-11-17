use smithay::backend::renderer::pixman::PixmanRenderer;

/// Create a `PixmanRenderer` for software compositing.
pub fn create_soft_renderer() -> Result<PixmanRenderer, String> {
    PixmanRenderer::new().map_err(|err| format!("PixmanRenderer::new failed: {err}"))
}
