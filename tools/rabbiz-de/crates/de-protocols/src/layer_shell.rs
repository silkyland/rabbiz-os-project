//! Ordering and coordinates shared by layer rendering and hit testing.

use smithay::desktop::{LayerMap, LayerSurface};
use smithay::utils::{Logical, Point};
use smithay::wayland::shell::wlr_layer::Layer;

pub const ABOVE_WINDOWS: [Layer; 2] = [Layer::Overlay, Layer::Top];
pub const BELOW_WINDOWS: [Layer; 2] = [Layer::Bottom, Layer::Background];

/// The map's geometry includes the surface tree's bounding-box offset.
#[must_use]
pub fn origin(map: &LayerMap, surface: &LayerSurface) -> Option<Point<i32, Logical>> {
    map.layer_geometry(surface)
        .map(|geometry| geometry.loc - surface.bbox().loc)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rendering and hit-testing both iterate these lists, so they must cover all
    /// four layers exactly once and keep the top-most layer first.
    #[test]
    fn levels_cover_every_layer_once_top_first() {
        let all: Vec<Layer> = ABOVE_WINDOWS.into_iter().chain(BELOW_WINDOWS).collect();
        for layer in [Layer::Overlay, Layer::Top, Layer::Bottom, Layer::Background] {
            assert_eq!(all.iter().filter(|l| **l == layer).count(), 1, "{layer:?}");
        }
        assert_eq!(ABOVE_WINDOWS[0], Layer::Overlay);
        assert_eq!(BELOW_WINDOWS[0], Layer::Bottom);
    }
}
