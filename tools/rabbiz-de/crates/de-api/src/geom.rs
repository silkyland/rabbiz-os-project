//! Geometry primitives in physical pixels.
//!
//! These are deliberately crate-local (not `euclid`, not smithay types) so that
//! `de-api` stays renderer-agnostic and builds on every host.

use serde::{Deserialize, Serialize};

/// A point in physical pixels.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    pub const ZERO: Self = Self { x: 0, y: 0 };

    #[must_use]
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

impl From<(i32, i32)> for Point {
    fn from((x, y): (i32, i32)) -> Self {
        Self { x, y }
    }
}

/// A size in physical pixels.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Size {
    pub w: i32,
    pub h: i32,
}

impl Size {
    pub const ZERO: Self = Self { w: 0, h: 0 };

    #[must_use]
    pub const fn new(w: i32, h: i32) -> Self {
        Self { w, h }
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.w <= 0 || self.h <= 0
    }

    #[must_use]
    pub fn area(self) -> i64 {
        i64::from(self.w.max(0)) * i64::from(self.h.max(0))
    }
}

impl From<(i32, i32)> for Size {
    fn from((w, h): (i32, i32)) -> Self {
        Self { w, h }
    }
}

/// An axis-aligned rectangle in physical pixels.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const ZERO: Self = Self {
        x: 0,
        y: 0,
        w: 0,
        h: 0,
    };

    #[must_use]
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    #[must_use]
    pub const fn from_origin_size(origin: Point, size: Size) -> Self {
        Self {
            x: origin.x,
            y: origin.y,
            w: size.w,
            h: size.h,
        }
    }

    #[must_use]
    pub const fn size(self) -> Size {
        Size::new(self.w, self.h)
    }

    #[must_use]
    pub const fn origin(self) -> Point {
        Point::new(self.x, self.y)
    }

    #[must_use]
    pub const fn right(self) -> i32 {
        self.x + self.w
    }

    #[must_use]
    pub const fn bottom(self) -> i32 {
        self.y + self.h
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.w <= 0 || self.h <= 0
    }

    #[must_use]
    pub const fn center(self) -> Point {
        Point::new(self.x + self.w / 2, self.y + self.h / 2)
    }

    #[must_use]
    pub const fn contains(self, p: Point) -> bool {
        p.x >= self.x && p.x < self.right() && p.y >= self.y && p.y < self.bottom()
    }

    #[must_use]
    pub const fn translate(self, dx: i32, dy: i32) -> Self {
        Self {
            x: self.x + dx,
            y: self.y + dy,
            w: self.w,
            h: self.h,
        }
    }

    #[must_use]
    pub fn intersect(self, other: Self) -> Option<Self> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let r = self.right().min(other.right());
        let b = self.bottom().min(other.bottom());
        (r > x && b > y).then(|| Self::new(x, y, r - x, b - y))
    }

    #[must_use]
    pub fn union(self, other: Self) -> Self {
        if self.is_empty() {
            return other;
        }
        if other.is_empty() {
            return self;
        }
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let r = self.right().max(other.right());
        let b = self.bottom().max(other.bottom());
        Self::new(x, y, r - x, b - y)
    }

    #[must_use]
    pub const fn inset(self, amount: i32) -> Self {
        Self {
            x: self.x + amount,
            y: self.y + amount,
            w: self.w - amount * 2,
            h: self.h - amount * 2,
        }
    }
}

/// Physical-pixel insets from the four edges of an output or rect.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EdgeInsets {
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    pub left: i32,
}

impl EdgeInsets {
    pub const ZERO: Self = Self {
        top: 0,
        right: 0,
        bottom: 0,
        left: 0,
    };

    #[must_use]
    pub const fn new(top: i32, right: i32, bottom: i32, left: i32) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }

    /// Component-wise maximum of two inset sets.
    #[must_use]
    pub const fn max(self, other: Self) -> Self {
        Self {
            top: if self.top > other.top {
                self.top
            } else {
                other.top
            },
            right: if self.right > other.right {
                self.right
            } else {
                other.right
            },
            bottom: if self.bottom > other.bottom {
                self.bottom
            } else {
                other.bottom
            },
            left: if self.left > other.left {
                self.left
            } else {
                other.left
            },
        }
    }

    /// Apply these insets to `rect`, clamping width and height at 0.
    #[must_use]
    pub const fn deflate(self, rect: Rect) -> Rect {
        let w = rect.w - self.left - self.right;
        let h = rect.h - self.top - self.bottom;
        Rect::new(
            rect.x + self.left,
            rect.y + self.top,
            if w > 0 { w } else { 0 },
            if h > 0 { h } else { 0 },
        )
    }
}

/// Complete inset breakdown for a single output (status/nav bars, cutout, IME).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WindowInsets {
    /// Status bar + navigation/gesture bar.
    pub system_bars: EdgeInsets,
    /// Display cutout / notch / punch-hole safe area.
    pub cutout: EdgeInsets,
    /// Active on-screen keyboard height (typically `bottom > 0`).
    pub ime: EdgeInsets,
}

impl WindowInsets {
    /// Usable rect for an app that respects system bars, cutouts, and IME.
    #[must_use]
    pub const fn app_frame(&self, output: Size) -> Rect {
        let combined = self.system_bars.max(self.cutout).max(self.ime);
        combined.deflate(Rect::new(0, 0, output.w, output.h))
    }

    /// Usable rect for an edge-to-edge/immersive app (ignores bars, still respects IME).
    #[must_use]
    pub const fn immersive_frame(&self, output: Size) -> Rect {
        self.ime.deflate(Rect::new(0, 0, output.w, output.h))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_intersect_union() {
        let a = Rect::new(0, 0, 10, 10);
        let b = Rect::new(5, 5, 10, 10);
        assert_eq!(a.intersect(b), Some(Rect::new(5, 5, 5, 5)));
        assert_eq!(a.union(b), Rect::new(0, 0, 15, 15));
        assert_eq!(a.intersect(Rect::new(20, 20, 5, 5)), None);
    }

    #[test]
    fn rect_contains() {
        let r = Rect::new(10, 10, 20, 20);
        assert!(r.contains(Point::new(10, 10)));
        assert!(!r.contains(Point::new(30, 10)));
    }

    #[test]
    fn window_insets_compute_app_and_immersive_frames() {
        let out = Size::new(720, 1600);
        let insets = WindowInsets {
            system_bars: EdgeInsets::new(56, 0, 48, 0),
            cutout: EdgeInsets::new(64, 0, 0, 0),
            ime: EdgeInsets::new(0, 0, 520, 0),
        };
        assert_eq!(insets.app_frame(out), Rect::new(0, 64, 720, 1016));
        assert_eq!(insets.immersive_frame(out), Rect::new(0, 0, 720, 1080));
    }

    #[test]
    fn edge_insets_deflate_clamps_non_negative() {
        let r = Rect::new(0, 0, 100, 100);
        let huge = EdgeInsets::new(80, 60, 80, 60);
        assert_eq!(huge.deflate(r), Rect::new(60, 80, 0, 0));
    }
}
