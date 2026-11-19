//! Retained-scene types shared by `de-paint`, `de-render` and `de-core`.
//!
//! A [`Scene`] is an ordered list of [`Layer`]s. `de-paint` rasterizes visual
//! units into [`Pixels`] (premultiplied RGBA8) when content changes; the
//! renderer uploads them as [`Texture`]s and composites layers every frame.
//! Animations only move textures — they never re-rasterize.

use serde::{Deserialize, Serialize};

use crate::geom::{Point, Rect, Size};

/// Canonical Z-order bands shared by compositing (bottom -> top) and input
/// hit-testing (top -> bottom). See `docs/DE-SCENE-INSETS.md`.
#[repr(u8)]
#[derive(
    Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub enum DisplayArea {
    /// Wallpaper and ambient background surfaces.
    Wallpaper = 0,
    /// Home screen / launcher grid / dock / desktop surface.
    Home = 10,
    /// Normal application toplevels (fullscreen, split, or tiled).
    #[default]
    App = 20,
    /// Floating windows, Picture-in-Picture, and app-owned modal dialogs.
    AppOverlay = 30,
    /// Persistent system chrome: status bar, navigation/gesture bar.
    SystemBars = 40,
    /// On-screen keyboard (IME) and input-method popups.
    Ime = 50,
    /// Transient banners: heads-up notifications, volume/brightness OSD, screenshot toast.
    Overlay = 60,
    /// Pull-down notification shade, quick settings, app drawer, recents, search.
    Shade = 70,
    /// Shell-owned menus and system dialogs (context menu, Polkit, Portal permissions).
    SystemDialog = 80,
    /// Lock screen / Keyguard (strictly isolates lower areas when active).
    Keyguard = 90,
    /// Full-screen intents allowed above Keyguard (incoming call, alarm).
    KeyguardOverlay = 100,
    /// Critical system overlays: power menu, screen-off fade, screenshot flash, cursor.
    Critical = 110,
}

impl DisplayArea {
    /// Whether this area is permitted to render or receive input while Keyguard is active.
    #[must_use]
    pub const fn allowed_while_locked(self) -> bool {
        (self as u8) >= (Self::Keyguard as u8)
    }
}

/// Opaque identifier of a layer inside a [`Scene`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct LayerId(pub u64);

/// Opaque identifier of a rasterized texture inside the renderer's cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TextureId(pub u64);

/// Premultiplied RGBA8 pixels, row-major, `4 * w * h` bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pixels {
    pub size: Size,
    pub data: Vec<u8>,
}

impl Pixels {
    #[must_use]
    pub fn new(size: Size, data: Vec<u8>) -> Self {
        debug_assert_eq!(data.len() as i64, size.area() * 4);
        Self { size, data }
    }

    #[must_use]
    pub fn empty(size: Size) -> Self {
        Self {
            size,
            data: vec![0; (size.area().max(0) * 4) as usize],
        }
    }
}

/// A rasterized image handed to the renderer for upload.
#[derive(Debug, Clone)]
pub struct Texture {
    pub id: TextureId,
    pub pixels: Pixels,
    /// Regions that changed since the last upload of this texture.
    /// Empty means "upload everything".
    pub damage: Vec<Rect>,
}

/// One composited quad in the scene.
#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    pub id: LayerId,
    /// Texture to sample; `None` = solid `color` fill (still composited as a layer).
    pub texture: Option<TextureId>,
    /// Destination rect in output space (physical pixels).
    pub dst: Rect,
    /// Global opacity multiplier, `0.0..=1.0`.
    pub opacity: f32,
    /// Rounded-corner radius in px, applied by the renderer.
    pub corner_radius: f32,
    /// Fallback/fill color when `texture` is `None` (premultiplied RGBA).
    pub color: [u8; 4],
    /// Damage within `dst` that must be repainted this frame.
    pub damage: Vec<Rect>,
    /// Whether this layer is visible.
    pub visible: bool,
}

impl Layer {
    #[must_use]
    pub fn textured(id: LayerId, texture: TextureId, dst: Rect) -> Self {
        Self {
            id,
            texture: Some(texture),
            dst,
            opacity: 1.0,
            corner_radius: 0.0,
            color: [0, 0, 0, 0],
            damage: Vec::new(),
            visible: true,
        }
    }

    #[must_use]
    pub fn solid(id: LayerId, dst: Rect, color: [u8; 4]) -> Self {
        Self {
            id,
            texture: None,
            dst,
            opacity: 1.0,
            corner_radius: 0.0,
            color,
            damage: Vec::new(),
            visible: true,
        }
    }
}

/// A full frame's worth of layers, bottom to top.
#[derive(Debug, Default, Clone)]
pub struct Scene {
    /// Physical output size this scene targets.
    pub output: Size,
    /// Clear color under everything (premultiplied RGBA).
    pub clear: [u8; 4],
    /// Layers in bottom-to-top order.
    pub layers: Vec<Layer>,
    /// Cursor position hint for compositor use (not a layer).
    pub cursor: Option<Point>,
}

impl Scene {
    #[must_use]
    pub fn new(output: Size, clear: [u8; 4]) -> Self {
        Self {
            output,
            clear,
            layers: Vec::new(),
            cursor: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_area_z_order_is_strictly_monotonic() {
        let ordered = [
            DisplayArea::Wallpaper,
            DisplayArea::Home,
            DisplayArea::App,
            DisplayArea::AppOverlay,
            DisplayArea::SystemBars,
            DisplayArea::Ime,
            DisplayArea::Overlay,
            DisplayArea::Shade,
            DisplayArea::SystemDialog,
            DisplayArea::Keyguard,
            DisplayArea::KeyguardOverlay,
            DisplayArea::Critical,
        ];
        for pair in ordered.windows(2) {
            assert!(
                pair[0] < pair[1],
                "{:?} must be below {:?}",
                pair[0],
                pair[1]
            );
        }
        assert!(!DisplayArea::App.allowed_while_locked());
        assert!(!DisplayArea::Shade.allowed_while_locked());
        assert!(DisplayArea::Keyguard.allowed_while_locked());
        assert!(DisplayArea::KeyguardOverlay.allowed_while_locked());
        assert!(DisplayArea::Critical.allowed_while_locked());
    }
}
