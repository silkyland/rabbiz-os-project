//! Recents snapshot cache (M3-T4): the last presented pixels of each
//! fullscreen app, keyed by toplevel app_id, so the recents rail can draw
//! real thumbnails instead of placeholders.
//!
//! Pure data — no smithay types cross this seam. The compositor calls
//! [`SnapshotCache::record`] wherever a surface snapshot is captured and
//! [`SnapshotCache::forget`] on `toplevel_destroyed`; `recents::render`
//! reads entries back in recency order.
//!
//! A 720×1600 RGBA frame is ~4.4 MB, so the cache is LRU-bounded: a new
//! record past `cap` evicts the least-recently-touched app.

use std::collections::HashMap;

use de_api::Pixels;

/// Default depth: 8 apps ≈ 35 MB worst case on the A21s' 2.8 GB.
pub const DEFAULT_CAP: usize = 8;

/// Last-shown frame per running app.
#[derive(Debug)]
pub struct SnapshotCache {
    cap: usize,
    /// Most-recent first; `map` keys are always a superset of `order`.
    order: Vec<String>,
    map: HashMap<String, Pixels>,
}

impl SnapshotCache {
    #[must_use]
    pub fn new(cap: usize) -> Self {
        Self {
            cap: cap.max(1),
            order: Vec::new(),
            map: HashMap::new(),
        }
    }

    /// Store `px` for `app_id`, promoting it to most-recent.
    pub fn record(&mut self, app_id: &str, px: Pixels) {
        self.order.retain(|id| id != app_id);
        self.order.insert(0, app_id.to_string());
        self.map.insert(app_id.to_string(), px);
        while self.order.len() > self.cap {
            if let Some(old) = self.order.pop() {
                self.map.remove(&old);
            }
        }
    }

    /// Latest snapshot for `app_id` (does not change recency — the card
    /// order must not shuffle just because the rail re-rendered).
    #[must_use]
    pub fn get(&self, app_id: &str) -> Option<&Pixels> {
        self.map.get(app_id)
    }

    /// App ids in recency order (most recent first).
    #[must_use]
    pub fn recent_ids(&self) -> &[String] {
        &self.order
    }

    /// Mark `app_id` most-recent without changing its snapshot (window
    /// raised to foreground but no new frame captured).
    pub fn touch(&mut self, app_id: &str) {
        if self.map.contains_key(app_id) {
            self.order.retain(|id| id != app_id);
            self.order.insert(0, app_id.to_string());
        }
    }

    /// Drop an app's snapshot (`toplevel_destroyed`, recents dismissal).
    pub fn forget(&mut self, app_id: &str) {
        self.order.retain(|id| id != app_id);
        self.map.remove(app_id);
    }

    pub fn clear(&mut self) {
        self.order.clear();
        self.map.clear();
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

impl Default for SnapshotCache {
    fn default() -> Self {
        Self::new(DEFAULT_CAP)
    }
}

// --- capture seam (smithay) ---

use smithay::desktop::Window;
use smithay::reexports::wayland_server::protocol::wl_shm;
use smithay::wayland::compositor::{with_states, BufferAssignment, SurfaceAttributes};
use smithay::wayland::seat::WaylandFocus;
use smithay::wayland::shell::xdg::{ToplevelSurface, XdgToplevelSurfaceData};
use smithay::wayland::shm::with_buffer_contents;

use de_api::Size;

/// A toplevel's `app_id` from its xdg role state ("" when unset).
#[must_use]
pub fn window_app_id(toplevel: &ToplevelSurface) -> String {
    with_states(toplevel.wl_surface(), |states| {
        states
            .data_map
            .get::<XdgToplevelSurfaceData>()
            .and_then(|d| d.lock().ok().and_then(|r| r.app_id.clone()))
            .unwrap_or_default()
    })
}

/// Copy the window's committed `wl_shm` buffer into `Pixels` (RGBA bytes).
///
/// Returns `None` for dmabuf/EGL clients — the A21s forces software
/// toolkits for shell-launched apps (`A21S_FORCE_SOFTWARE=1`), so the
/// common path *is* shm; GPU clients keep the icon fallback in the rail.
#[allow(unsafe_code)] // smithay hands us a raw pool pointer, scoped to the closure.
pub fn capture_window(window: &Window) -> Option<Pixels> {
    let surface = window.wl_surface()?.into_owned();
    let buffer = with_states(&surface, |states| {
        // `BufferAssignment` isn't Clone; take a clone of the WlBuffer only.
        match states
            .cached_state
            .get::<SurfaceAttributes>()
            .current()
            .buffer
        {
            Some(BufferAssignment::NewBuffer(ref b)) => Some(b.clone()),
            _ => None,
        }
    });
    let buffer = buffer?;
    let out = with_buffer_contents(&buffer, |ptr, _len, data| {
        let (w, h, stride, off) = (data.width, data.height, data.stride, data.offset);
        if w <= 0 || h <= 0 || stride < w * 4 {
            return None;
        }
        // wl_shm formats are little-endian words: Argb/Xrgb store B,G,R,A
        // bytes; Abgr/Xbgr store R,G,B,A — already `Pixels` order.
        let (swap_rb, opaque) = match data.format {
            wl_shm::Format::Argb8888 => (true, false),
            wl_shm::Format::Xrgb8888 => (true, true),
            wl_shm::Format::Abgr8888 => (false, false),
            wl_shm::Format::Xbgr8888 => (false, true),
            _ => return None,
        };
        let mut px = Pixels::empty(Size::new(w, h));
        for y in 0..h {
            // SAFETY: ptr is valid for the pool for the closure's lifetime;
            // offset + h*stride bytes of this buffer are inside it (client-
            // validated by wl_shm), and we never read past row `off+stride`.
            let row = unsafe { ptr.add((off + y * stride) as usize) };
            for x in 0..w {
                let (b0, b1, b2, b3) = unsafe {
                    let s = row.add((x * 4) as usize);
                    (*s, *s.add(1), *s.add(2), *s.add(3))
                };
                let i = ((y * w + x) * 4) as usize;
                px.data[i] = if swap_rb { b2 } else { b0 };
                px.data[i + 1] = b1;
                px.data[i + 2] = if swap_rb { b0 } else { b2 };
                px.data[i + 3] = if opaque { 0xFF } else { b3 };
            }
        }
        Some(px)
    });
    out.ok().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use de_api::Size;

    fn px(w: i32, h: i32, v: u8) -> Pixels {
        Pixels::new(Size::new(w, h), vec![v; (w * h * 4) as usize])
    }

    #[test]
    fn record_and_recency_order() {
        let mut c = SnapshotCache::new(3);
        c.record("a", px(2, 2, 1));
        c.record("b", px(2, 2, 2));
        c.record("a", px(2, 2, 3)); // promote a
        assert_eq!(c.recent_ids(), &["a", "b"]);
        assert_eq!(c.get("a").unwrap().data[0], 3);
    }

    #[test]
    fn lru_evicts_oldest() {
        let mut c = SnapshotCache::new(2);
        c.record("a", px(2, 2, 1));
        c.record("b", px(2, 2, 2));
        c.record("c", px(2, 2, 3)); // evicts a
        assert_eq!(c.recent_ids(), &["c", "b"]);
        assert!(c.get("a").is_none());
        assert_eq!(c.len(), 2);
    }

    #[test]
    fn touch_promotes_without_new_pixels() {
        let mut c = SnapshotCache::new(3);
        c.record("a", px(2, 2, 1));
        c.record("b", px(2, 2, 2));
        c.touch("a");
        assert_eq!(c.recent_ids(), &["a", "b"]);
        assert_eq!(c.get("a").unwrap().data[0], 1);
        c.touch("ghost");
        assert_eq!(c.recent_ids(), &["a", "b"]);
    }

    #[test]
    fn forget_and_clear() {
        let mut c = SnapshotCache::default();
        c.record("a", px(2, 2, 1));
        c.record("b", px(2, 2, 2));
        c.forget("a");
        assert_eq!(c.recent_ids(), &["b"]);
        c.clear();
        assert!(c.is_empty());
    }
}
