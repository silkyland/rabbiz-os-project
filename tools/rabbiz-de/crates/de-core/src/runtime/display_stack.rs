//! Smithay front-to-back render stack backed by [`de_api::DisplayStack`].
//!
//! Smithay's `DamageTrackedRenderer::render_frame` expects elements in
//! **front-to-back** order (index 0 = topmost). This adapter maps semantic
//! [`DisplayArea`] bands directly to that order and enforces Keyguard
//! isolation whenever the screen is locked.

use de_api::{DisplayArea, DisplayStack};

/// Collects render elements into [`DisplayArea`] bands and emits them in
/// Smithay's front-to-back order.
pub(crate) struct RenderStack<E> {
    inner: DisplayStack<E>,
}

impl<E> RenderStack<E> {
    pub(crate) const fn new() -> Self {
        Self {
            inner: DisplayStack::new(),
        }
    }

    /// Push a single element into `area`. Later pushes within the same `area`
    /// appear in front of earlier pushes.
    pub(crate) fn push(&mut self, area: DisplayArea, element: E) {
        self.inner.push(area, element);
    }

    /// Push a slice of elements that are already ordered front-to-back
    /// (e.g. from Smithay's `window.render_elements`) into `area`.
    pub(crate) fn extend_front_to_back(&mut self, area: DisplayArea, elements: Vec<E>) {
        for element in elements.into_iter().rev() {
            self.inner.push(area, element);
        }
    }

    /// Emit elements in Smithay's front-to-back order (topmost at index 0).
    /// When `locked` is true, everything below [`DisplayArea::Keyguard`] is dropped.
    pub(crate) fn into_smithay_elements(self, locked: bool) -> Vec<E> {
        self.inner.into_hit_test_order(locked)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smithay_order_is_front_to_back_and_respects_front_to_back_batches() {
        let mut stack = RenderStack::new();
        stack.push(DisplayArea::Wallpaper, "wall");
        // Front-to-back batch: popup is in front of app window.
        stack.extend_front_to_back(DisplayArea::App, vec!["popup", "app"]);
        stack.push(DisplayArea::SystemBars, "gesture-bar");
        stack.push(DisplayArea::Critical, "cursor");

        assert_eq!(
            stack.into_smithay_elements(false),
            vec!["cursor", "gesture-bar", "popup", "app", "wall"]
        );
    }

    #[test]
    fn strictly_locked_first_frame_contains_only_keyguard_and_above() {
        let mut stack = RenderStack::new();
        stack.push(DisplayArea::Wallpaper, "wall");
        stack.push(DisplayArea::Home, "home");
        stack.extend_front_to_back(DisplayArea::App, vec!["app"]);
        stack.push(DisplayArea::Shade, "shade");
        stack.push(DisplayArea::Keyguard, "lock");

        assert_eq!(stack.into_smithay_elements(true), vec!["lock"]);
    }
}
