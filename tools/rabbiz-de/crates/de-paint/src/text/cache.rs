//! Label raster cache — the "no per-frame raster" contract from §7.17a.
//! Shaped text is expensive; entries are keyed by content+style and evicted
//! LRU beyond a byte budget. Hit rate is observable via [`CacheStats`].

use std::collections::HashMap;
use std::sync::Mutex;

use de_api::{Pixels, Size};
use tiny_skia::Pixmap;

/// Key for one cached label raster.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LabelKey {
    pub text: String,
    pub family: &'static str,
    pub size_px: u32,
    pub line_height_px: u32,
    pub weight: u16,
    pub color: [u8; 4],
    pub max_width_px: Option<u32>,
    pub max_lines: usize,
}

/// Byte budget for cached label pixmaps (~4 MiB — a full-screen text page
/// is ~4.6 MB at 720x1600; labels are far smaller).
pub const LABEL_CACHE_BUDGET: usize = 4 * 1024 * 1024;
/// Hard cap on entry count regardless of bytes.
pub const LABEL_CACHE_MAX_ENTRIES: usize = 1024;

struct Entry {
    px: Pixmap,
    bytes: usize,
    /// Last-use counter (LRU).
    stamp: u64,
}

/// Process-global label cache.
pub struct LabelCache {
    map: HashMap<LabelKey, Entry>,
    bytes: usize,
    clock: u64,
    /// (hits, misses) for `de-stats`.
    pub stats: (u64, u64),
}

impl LabelCache {
    fn new() -> Self {
        Self {
            map: HashMap::new(),
            bytes: 0,
            clock: 0,
            stats: (0, 0),
        }
    }

    /// Look up `key`; on miss, call `produce` and cache the result.
    /// Returns the pixmap clone (tiny-skia `Pixmap` is ref-counted? No — it
    /// is an owned buffer; we clone the pixel data, cheap enough at label
    /// sizes).
    pub fn get_or_insert_with(
        &mut self,
        key: LabelKey,
        produce: impl FnOnce() -> Option<Pixmap>,
    ) -> Option<Pixmap> {
        self.clock += 1;
        if let Some(e) = self.map.get_mut(&key) {
            e.stamp = self.clock;
            self.stats.0 += 1;
            return Some(clone_pixmap(&e.px));
        }
        self.stats.1 += 1;
        let px = produce()?;
        let bytes = px.data().len();
        self.evict_for(bytes);
        self.map.insert(
            key,
            Entry {
                px: clone_pixmap(&px),
                bytes,
                stamp: self.clock,
            },
        );
        self.bytes += bytes;
        Some(px)
    }

    /// Evict least-recently-used entries until `incoming` fits the budget.
    fn evict_for(&mut self, incoming: usize) {
        while self.bytes + incoming > LABEL_CACHE_BUDGET
            || self.map.len() + 1 > LABEL_CACHE_MAX_ENTRIES
        {
            let Some((k, _)) = self.map.iter().min_by_key(|(_, e)| e.stamp) else {
                break;
            };
            let k = k.clone();
            if let Some(e) = self.map.remove(&k) {
                self.bytes = self.bytes.saturating_sub(e.bytes);
            }
        }
    }

    /// Current cache size in bytes (diagnostics).
    #[must_use]
    pub fn bytes(&self) -> usize {
        self.bytes
    }
}

fn clone_pixmap(p: &Pixmap) -> Pixmap {
    let mut q = Pixmap::new(p.width(), p.height()).expect("same size");
    q.data_mut().copy_from_slice(p.data());
    q
}

static CACHE: Mutex<Option<LabelCache>> = Mutex::new(None);

/// Access the shared cache. `Mutex::lock().unwrap()` is fine here: label
/// rasterization is already serialized behind the `FontSystem` mutex.
pub fn with_cache<R>(f: impl FnOnce(&mut LabelCache) -> R) -> R {
    let mut g = CACHE.lock().unwrap();
    f(g.get_or_insert_with(LabelCache::new))
}

/// Convert a cached pixmap to `de-api` pixels.
#[must_use]
pub fn to_pixels(px: &Pixmap) -> Pixels {
    Pixels {
        size: Size::new(px.width() as i32, px.height() as i32),
        data: px.data().to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evicts_lru_over_budget() {
        let mut c = LabelCache::new();
        let key = |i: usize| LabelKey {
            text: format!("t{i}"),
            family: "x",
            size_px: 10,
            line_height_px: 10,
            weight: 400,
            color: [0; 4],
            max_width_px: None,
            max_lines: 1,
        };
        for i in 0..LABEL_CACHE_MAX_ENTRIES + 8 {
            c.get_or_insert_with(key(i), || Pixmap::new(8, 8));
        }
        assert!(c.map.len() <= LABEL_CACHE_MAX_ENTRIES);
        assert!(c.bytes() <= LABEL_CACHE_BUDGET);
        // earliest entries evicted.
        assert!(!c.map.contains_key(&key(0)));
        assert!(c.map.contains_key(&key(LABEL_CACHE_MAX_ENTRIES + 7)));
    }

    #[test]
    fn hit_and_miss_counted() {
        let mut c = LabelCache::new();
        let k = LabelKey {
            text: "x".into(),
            family: "x",
            size_px: 10,
            line_height_px: 10,
            weight: 400,
            color: [0; 4],
            max_width_px: None,
            max_lines: 1,
        };
        assert!(c
            .get_or_insert_with(k.clone(), || Pixmap::new(4, 4))
            .is_some());
        assert!(c.get_or_insert_with(k, || Pixmap::new(4, 4)).is_some());
        assert_eq!(c.stats, (1, 1));
    }
}
