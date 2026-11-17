//! Software rasterizer: scene -> premultiplied RGBA pixmap -> PNG.
//!
//! This is the scaffold API. A lane agent fills the bodies; the signatures
//! are frozen because `de-ui` and `de-sim` compile against them.

use std::path::Path;

use de_api::Scene;
use tiny_skia::Pixmap;

pub struct Renderer {
    _private: (),
}

impl Renderer {
    pub fn new() -> Self {
        todo!("de-paint lane: load fonts and create the swash cache")
    }

    pub fn render(&mut self, scene: &Scene) -> Pixmap {
        let _ = scene;
        todo!("de-paint lane: rasterize every layer and drawing op")
    }
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}

pub fn save_png(pixmap: &Pixmap, path: &Path) -> Result<(), String> {
    let _ = (pixmap, path);
    todo!("de-paint lane: create parent dirs and encode PNG")
}
