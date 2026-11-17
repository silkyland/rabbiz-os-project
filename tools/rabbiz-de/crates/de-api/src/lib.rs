//! Platform-neutral scene types for `rabbiz-de`.
//!
//! This crate has no renderer, windowing or OS dependency. Everything the UI
//! wants to draw is expressed here, so the same scene can be rasterized by the
//! GPU renderer, the pixman renderer, or the host simulator.

mod geom;
mod scene;

pub use geom::{Point, Rect, Size};
pub use scene::{Color, DrawOp, Layer, Scene, TextAlign, TextStyle, Weight};

/// Which compositor renderer is presenting this scene.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RendererKind {
    Gl,
    Soft,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_contains_its_edges_and_rejects_outside() {
        let r = Rect::new(10.0, 20.0, 100.0, 50.0);
        assert!(r.contains(Point::new(10.0, 20.0)));
        assert!(r.contains(Point::new(109.9, 69.9)));
        assert!(!r.contains(Point::new(110.0, 69.9)));
        assert!(!r.contains(Point::new(9.9, 20.0)));
    }

    #[test]
    fn scene_keeps_layer_order() {
        let mut scene = Scene::new(Size::new(720, 1600));
        scene.push(Layer::new("background"));
        scene.push(Layer::new("foreground"));
        assert_eq!(scene.layers.len(), 2);
        assert_eq!(scene.layers[1].name, "foreground");
    }
}
