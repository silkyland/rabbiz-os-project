//! Pixman (software) renderer — the fallback/diagnostic path.

use smithay::backend::renderer::pixman::{PixmanError, PixmanRenderer};

/// Create a `PixmanRenderer`.
pub fn create() -> Result<PixmanRenderer, PixmanError> {
    PixmanRenderer::new()
}
