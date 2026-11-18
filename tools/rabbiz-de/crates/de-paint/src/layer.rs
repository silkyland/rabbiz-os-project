//! Retained layer cache (M2-T4): a `Layer` owns a rasterized `Pixels` texture
//! plus its placement state. Content is rasterized once (into `content`) and
//! frames only re-composite — never re-rasterize per frame.

use std::collections::HashMap;

use de_api::{Pixels, Rect};
use de_design::motion::{MotionKind, SpringPreset};

use crate::animation::SpringValue;

/// Unique layer id.
pub type LayerId = u64;

/// A retained layer: rasterized content + animated placement.
#[derive(Debug)]
pub struct Layer {
    pub id: LayerId,
    /// Rasterized pixels (premultiplied RGBA). Replaced only when content
    /// changes — the texture upload follows.
    pub content: Pixels,
    /// Animated x/y/opacity — position changes animate texture placement,
    /// never re-rasterize `content`.
    pub x: SpringValue,
    pub y: SpringValue,
    pub opacity: SpringValue,
    /// Corner radius hint for the renderer (rounded clip in shader or mask).
    pub radius: f32,
    pub visible: bool,
}

impl Layer {
    #[must_use]
    pub fn new(id: LayerId, content: Pixels, x: f32, y: f32, preset: SpringPreset) -> Self {
        Self {
            id,
            content,
            x: SpringValue::new(x, preset, MotionKind::Spatial),
            y: SpringValue::new(y, preset, MotionKind::Spatial),
            opacity: SpringValue::new(1.0, preset, MotionKind::Effects),
            radius: 0.0,
            visible: true,
        }
    }

    /// Current animated bounds.
    #[must_use]
    pub fn bounds(&self) -> Rect {
        Rect::new(
            self.x.value.round() as i32,
            self.y.value.round() as i32,
            self.content.size.w,
            self.content.size.h,
        )
    }

    /// True while any channel is still animating.
    #[must_use]
    pub fn animating(&self) -> bool {
        !self.x.settled() || !self.y.settled() || !self.opacity.settled()
    }

    /// Advance all channels by `dt_s`; returns damage union of old+new bounds
    /// when the layer moved (None when static and visible-content unchanged).
    pub fn advance(&mut self, dt_s: f32) -> Option<Rect> {
        let before = self.bounds();
        let a = self.x.advance(dt_s) | self.y.advance(dt_s) | self.opacity.advance(dt_s);
        if !a {
            return None;
        }
        let after = self.bounds();
        Some(crate::damage::DamageTracker::union_of(before, after))
    }

    /// Replace content; caller must damage `bounds()` ∪ new bounds.
    pub fn set_content(&mut self, content: Pixels) {
        self.content = content;
    }
}

/// Ordered stack of retained layers (bottom → top).
#[derive(Debug, Default)]
pub struct LayerTree {
    pub layers: Vec<Layer>,
    next_id: LayerId,
}

impl LayerTree {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a layer on top; returns its id.
    pub fn push(&mut self, content: Pixels, x: f32, y: f32, preset: SpringPreset) -> LayerId {
        self.next_id += 1;
        let id = self.next_id;
        self.layers.push(Layer::new(id, content, x, y, preset));
        id
    }

    #[must_use]
    pub fn get(&self, id: LayerId) -> Option<&Layer> {
        self.layers.iter().find(|l| l.id == id)
    }

    pub fn get_mut(&mut self, id: LayerId) -> Option<&mut Layer> {
        self.layers.iter_mut().find(|l| l.id == id)
    }

    /// Remove a layer; returns its bounds for damage.
    pub fn remove(&mut self, id: LayerId) -> Option<Rect> {
        let i = self.layers.iter().position(|l| l.id == id)?;
        let r = self.layers[i].bounds();
        self.layers.remove(i);
        Some(r)
    }
}

/// Texture cache: keyed by content fingerprint so identical raster content
/// (same label, same icon) shares one upload. Host-side this just dedups
/// `Pixels`; the GL backend maps ids to `TextureId`.
#[derive(Debug, Default)]
pub struct TextureCache {
    entries: HashMap<u64, Pixels>,
}

impl TextureCache {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or fetch by key (e.g. hash of content parameters).
    pub fn get_or_insert(&mut self, key: u64, make: impl FnOnce() -> Pixels) -> &Pixels {
        self.entries.entry(key).or_insert_with(make)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn evict(&mut self, key: u64) {
        self.entries.remove(&key);
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use de_api::Size;

    fn px(w: i32, h: i32) -> Pixels {
        Pixels::empty(Size::new(w, h))
    }

    #[test]
    fn layer_animation_damages_union() {
        let mut l = Layer::new(1, px(10, 10), 0.0, 0.0, SpringPreset::Smooth);
        l.x.retarget(50.0);
        let d = l.advance(1.0 / 60.0).expect("moving layer damages");
        assert!(d.x <= 0 && d.w > 10, "{d:?}");
        // Settled layer reports no damage.
        let mut l2 = Layer::new(2, px(10, 10), 0.0, 0.0, SpringPreset::Smooth);
        assert!(l2.advance(1.0 / 60.0).is_none());
    }

    #[test]
    fn layer_opacity_stays_in_range_with_a_bouncy_preset() {
        let mut layer = Layer::new(1, px(10, 10), 0.0, 0.0, SpringPreset::Soft);
        layer.opacity.retarget(0.0);
        for _ in 0..120 {
            layer.advance(1.0 / 60.0);
            assert!((0.0..=1.0).contains(&layer.opacity.value));
        }
        assert_eq!(layer.opacity.value, 0.0);
    }

    #[test]
    fn tree_push_remove() {
        let mut t = LayerTree::new();
        let id = t.push(px(8, 8), 4.0, 6.0, SpringPreset::Smooth);
        assert_eq!(t.get(id).unwrap().bounds(), Rect::new(4, 6, 8, 8));
        assert_eq!(t.remove(id), Some(Rect::new(4, 6, 8, 8)));
        assert!(t.get(id).is_none());
    }

    #[test]
    fn texture_cache_dedups() {
        let mut c = TextureCache::new();
        let mut calls = 0;
        c.get_or_insert(42, || {
            calls += 1;
            px(4, 4)
        });
        c.get_or_insert(42, || {
            calls += 1;
            px(4, 4)
        });
        assert_eq!(calls, 1);
        assert_eq!(c.len(), 1);
    }
}
