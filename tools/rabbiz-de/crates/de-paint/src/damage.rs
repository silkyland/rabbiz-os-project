//! Damage tracking (M2-T4): the compositor only repaints where pixels could
//! have changed. A `DamageTracker` accumulates `Rect`s in surface space and
//! hands the renderer a compact list per frame.

use de_api::Rect;

/// Accumulates damaged rects; `take()` drains them for the frame.
#[derive(Debug, Default)]
pub struct DamageTracker {
    rects: Vec<Rect>,
}

impl DamageTracker {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Mark `r` damaged. Rects touching existing damage are merged to keep the
    /// list small.
    pub fn add(&mut self, r: Rect) {
        if r.w <= 0 || r.h <= 0 {
            return;
        }
        // Merge if the new rect intersects or abuts an existing one.
        for i in 0..self.rects.len() {
            if intersects_or_abuts(self.rects[i], r) {
                self.rects[i] = union(self.rects[i], r);
                return;
            }
        }
        self.rects.push(r);
    }

    /// Mark the whole `r` region (e.g. full screen on resize).
    pub fn damage_all(&mut self, r: Rect) {
        self.rects.clear();
        self.rects.push(r);
    }

    /// Drain and return the damage list for this frame.
    pub fn take(&mut self) -> Vec<Rect> {
        // Second-pass merge so overlapping unions collapse transitively.
        let mut out: Vec<Rect> = Vec::with_capacity(self.rects.len());
        for r in self.rects.drain(..) {
            let mut merged = false;
            for o in &mut out {
                if intersects_or_abuts(*o, r) {
                    *o = union(*o, r);
                    merged = true;
                    break;
                }
            }
            if !merged {
                out.push(r);
            }
        }
        out
    }

    /// Peek at current damage without draining.
    #[must_use]
    pub fn pending(&self) -> &[Rect] {
        &self.rects
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rects.is_empty()
    }
}

fn intersects_or_abuts(a: Rect, b: Rect) -> bool {
    a.x <= b.x + b.w && b.x <= a.x + a.w && a.y <= b.y + b.h && b.y <= a.y + a.h
}

impl DamageTracker {
    /// Union of two rects (public for callers that merge layer bounds).
    #[must_use]
    pub fn union_of(a: Rect, b: Rect) -> Rect {
        union(a, b)
    }
}

fn union(a: Rect, b: Rect) -> Rect {
    let x = a.x.min(b.x);
    let y = a.y.min(b.y);
    Rect::new(
        x,
        y,
        (a.x + a.w).max(b.x + b.w) - x,
        (a.y + a.h).max(b.y + b.h) - y,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disjoint_rects_stay_separate() {
        let mut d = DamageTracker::new();
        d.add(Rect::new(0, 0, 10, 10));
        d.add(Rect::new(100, 100, 10, 10));
        assert_eq!(d.take().len(), 2);
    }

    #[test]
    fn overlapping_rects_merge() {
        let mut d = DamageTracker::new();
        d.add(Rect::new(0, 0, 50, 50));
        d.add(Rect::new(25, 25, 50, 50));
        let dr = d.take();
        assert_eq!(dr.len(), 1);
        assert_eq!(dr[0], Rect::new(0, 0, 75, 75));
    }

    #[test]
    fn damage_all_replaces() {
        let mut d = DamageTracker::new();
        d.add(Rect::new(0, 0, 5, 5));
        d.damage_all(Rect::new(0, 0, 720, 1600));
        let dr = d.take();
        assert_eq!(dr, vec![Rect::new(0, 0, 720, 1600)]);
        assert!(d.is_empty());
    }
}
