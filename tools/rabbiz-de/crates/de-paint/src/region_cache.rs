//! Keyed sub-region dirty tracking (`docs/DE-RETAINED-UI.md`, Tier B).
//!
//! Tracks `(RegionId, content_hash, bounds)` across frames so surfaces can
//! compute exact dirty rectangles for changed, moved, and removed regions and
//! feed them into [`DamageTracker`].

use std::collections::BTreeMap;

use de_api::Rect;

use crate::damage::DamageTracker;

/// Stable logical identifier for a widget/region within a surface.
pub type RegionId = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RegionEntry {
    hash: u64,
    bounds: Rect,
}

/// Retained per-surface cache of region hashes and bounds across frames.
///
/// Typical per-frame usage:
/// 1. Call [`RegionCache::begin_frame`].
/// 2. For each visible region, call [`RegionCache::record`].
/// 3. Call [`RegionCache::finish_frame`] to push damaged rects into a
///    [`DamageTracker`] and advance the cache to the new frame.
#[derive(Debug, Default, Clone)]
pub struct RegionCache {
    prev: BTreeMap<RegionId, RegionEntry>,
    curr: BTreeMap<RegionId, RegionEntry>,
    dirty_rects: Vec<Rect>,
}

impl RegionCache {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Start recording regions for a new frame.
    pub fn begin_frame(&mut self) {
        self.curr.clear();
        self.dirty_rects.clear();
    }

    /// Record a region's `(hash, bounds)` for the current frame.
    ///
    /// Returns `true` if the region is new or its hash or bounds changed since
    /// the previous frame, and `false` if it is completely unchanged.
    pub fn record(&mut self, id: RegionId, hash: u64, bounds: Rect) -> bool {
        let entry = RegionEntry { hash, bounds };
        let changed = match self.prev.get(&id) {
            Some(&old) if old == entry => false,
            Some(&old) => {
                self.dirty_rects.push(old.bounds);
                self.dirty_rects.push(bounds);
                true
            }
            None => {
                self.dirty_rects.push(bounds);
                true
            }
        };
        self.curr.insert(id, entry);
        changed
    }

    /// Finalize the current frame: emit old+new bounds for changed regions and
    /// old bounds for any regions present in the previous frame that were not
    /// recorded this frame, then commit the current frame as the baseline.
    pub fn finish_frame(&mut self, damage: &mut DamageTracker) {
        for (id, old) in &self.prev {
            if !self.curr.contains_key(id) {
                damage.add(old.bounds);
            }
        }
        for rect in self.dirty_rects.drain(..) {
            damage.add(rect);
        }
        std::mem::swap(&mut self.prev, &mut self.curr);
        self.curr.clear();
    }

    /// Drop all cached region state so the next frame treats every region as
    /// new (e.g. after a surface resize or theme change).
    pub fn invalidate_all(&mut self) {
        self.prev.clear();
        self.curr.clear();
        self.dirty_rects.clear();
    }

    /// Number of regions retained from the last completed frame.
    #[must_use]
    pub fn len(&self) -> usize {
        self.prev.len()
    }

    /// Whether no regions are currently retained.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.prev.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unchanged_frame_produces_empty_damage() {
        let mut cache = RegionCache::new();
        let mut damage = DamageTracker::new();

        // Frame 1: initial regions are new and produce damage.
        cache.begin_frame();
        assert!(cache.record(1, 100, Rect::new(0, 0, 50, 20)));
        assert!(cache.record(2, 200, Rect::new(100, 0, 60, 20)));
        cache.finish_frame(&mut damage);
        assert_eq!(damage.take().len(), 2);

        // Frame 2: identical hashes and bounds produce zero damage.
        cache.begin_frame();
        assert!(!cache.record(1, 100, Rect::new(0, 0, 50, 20)));
        assert!(!cache.record(2, 200, Rect::new(100, 0, 60, 20)));
        cache.finish_frame(&mut damage);
        assert!(damage.is_empty());
        assert_eq!(damage.take(), Vec::<Rect>::new());
    }

    #[test]
    fn modified_hash_damages_old_union_new() {
        let mut cache = RegionCache::new();
        let mut damage = DamageTracker::new();

        cache.begin_frame();
        assert!(cache.record(1, 11, Rect::new(10, 10, 40, 20)));
        assert!(cache.record(2, 22, Rect::new(200, 10, 40, 20)));
        cache.finish_frame(&mut damage);
        let _ = damage.take();

        // Region 1 changes hash and grows slightly (overlapping old bounds).
        cache.begin_frame();
        assert!(cache.record(1, 99, Rect::new(10, 10, 55, 25)));
        assert!(!cache.record(2, 22, Rect::new(200, 10, 40, 20)));
        cache.finish_frame(&mut damage);

        let rects = damage.take();
        assert_eq!(rects, vec![Rect::new(10, 10, 55, 25)]);
    }

    #[test]
    fn moved_region_damages_both_old_and_new_rects() {
        let mut cache = RegionCache::new();
        let mut damage = DamageTracker::new();

        let old_rect = Rect::new(10, 10, 30, 30);
        let new_rect = Rect::new(120, 80, 30, 30);

        cache.begin_frame();
        assert!(cache.record(7, 42, old_rect));
        cache.finish_frame(&mut damage);
        let _ = damage.take();

        // Same hash, disjoint new position.
        cache.begin_frame();
        assert!(cache.record(7, 42, new_rect));
        cache.finish_frame(&mut damage);

        let rects = damage.take();
        assert_eq!(rects.len(), 2);
        assert!(rects.contains(&old_rect));
        assert!(rects.contains(&new_rect));
    }

    #[test]
    fn removed_region_damages_its_old_rect() {
        let mut cache = RegionCache::new();
        let mut damage = DamageTracker::new();

        let kept = Rect::new(0, 0, 40, 20);
        let removed = Rect::new(100, 50, 80, 30);

        cache.begin_frame();
        cache.record(1, 1, kept);
        cache.record(2, 2, removed);
        cache.finish_frame(&mut damage);
        let _ = damage.take();

        // Region 2 disappears.
        cache.begin_frame();
        assert!(!cache.record(1, 1, kept));
        cache.finish_frame(&mut damage);

        assert_eq!(damage.take(), vec![removed]);
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn invalidate_all_marks_subsequent_regions_dirty() {
        let mut cache = RegionCache::new();
        let mut damage = DamageTracker::new();

        let r = Rect::new(5, 5, 20, 20);
        cache.begin_frame();
        cache.record(1, 1, r);
        cache.finish_frame(&mut damage);
        let _ = damage.take();

        cache.invalidate_all();
        assert!(cache.is_empty());

        cache.begin_frame();
        assert!(cache.record(1, 1, r));
        cache.finish_frame(&mut damage);
        assert_eq!(damage.take(), vec![r]);
    }
}
