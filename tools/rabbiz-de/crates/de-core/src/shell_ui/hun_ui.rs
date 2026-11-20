//! Heads-Up Notification (HUN) compositor overlay controller: wraps
//! [`de_ui::hun::HunState`] with owned banner data, cached GPU texture slot,
//! and on-demand pixel rasterization for `DisplayArea::Overlay`.

use std::any::Any;

use de_api::{Pixels, Rect, Size};
use de_design::tokens::{self, Theme};
use de_ui::hun::{HunCandidate, HunContext, HunOutcome, HunState};
use de_ui::screens::hun_banner::{self, HunBannerModel};

use crate::state::De;

/// Owned notification payload displayed on a HUN banner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HunBannerData {
    pub id: u32,
    pub app: String,
    pub title: String,
    pub body: String,
    pub actions: Vec<(String, String)>,
    pub critical: bool,
}

impl HunBannerData {
    #[must_use]
    pub fn as_model(&self) -> HunBannerModel<'_> {
        HunBannerModel {
            id: self.id,
            app: &self.app,
            title: &self.title,
            body: &self.body,
            actions: &self.actions,
            critical: self.critical,
        }
    }
}

/// Compositor-side HUN overlay state, mirroring [`super::osd_ui::OsdFx`].
pub(crate) struct HunFx {
    state: HunState,
    active_model: Option<HunBannerData>,
    queued_models: Vec<HunBannerData>,
    pub px: Option<Pixels>,
    pub tex: Option<Box<dyn Any>>,
    /// Bumped on every re-raster — the frame loop uploads on change.
    pub generation: u64,
    pub tex_generation: u64,
    dirty: bool,
    pub top_inset: i32,
}

impl Default for HunFx {
    fn default() -> Self {
        Self::new()
    }
}

impl HunFx {
    /// Create a new HUN overlay controller with default timeout and status-bar inset.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: HunState::default(),
            active_model: None,
            queued_models: Vec::new(),
            px: None,
            tex: None,
            generation: 0,
            tex_generation: u64::MAX,
            dirty: false,
            top_inset: tokens::STATUS_BAR_HEIGHT,
        }
    }

    /// Evaluate the HUN gating policy and enqueue `data` if accepted.
    ///
    /// Returns `true` if the notification became active or was queued.
    #[allow(dead_code)] // Wired from notification D-Bus daemon in a follow-up
    pub fn enqueue(
        &mut self,
        candidate: HunCandidate,
        data: HunBannerData,
        ctx: HunContext,
        now_ms: u64,
    ) -> bool {
        if !self.state.enqueue(candidate, ctx, now_ms) {
            return false;
        }
        if self.state.active().map(|a| a.id) == Some(candidate.id) {
            self.active_model = Some(data);
            self.dirty = true;
            self.px = None;
            self.tex = None;
        } else if let Some(slot) = self.queued_models.iter_mut().find(|m| m.id == data.id) {
            *slot = data;
        } else {
            self.queued_models.push(data);
        }
        true
    }

    /// Advance the HUN auto-dismiss timer to `now_ms`.
    pub fn tick(&mut self, now_ms: u64) -> Option<HunOutcome> {
        let outcome = self.state.tick(now_ms);
        if outcome.is_some() {
            self.sync_active_after_transition();
        }
        outcome
    }

    /// Pause (`held = true`) or resume (`held = false`) the auto-dismiss timer.
    #[allow(dead_code)] // Wired from input/touch.rs in a follow-up
    pub fn set_touch_held(&mut self, held: bool, now_ms: u64) {
        self.state.set_touch_held(held, now_ms);
    }

    /// Swipe up on the active banner: snoozes the banner while keeping it in the shade.
    #[allow(dead_code)] // Wired from input/touch.rs in a follow-up
    pub fn swipe_up(&mut self, now_ms: u64) -> Option<HunOutcome> {
        let outcome = self.state.swipe_up(now_ms);
        if outcome.is_some() {
            self.sync_active_after_transition();
        }
        outcome
    }

    /// Swipe left or right on the active banner: dismisses the notification completely.
    #[allow(dead_code)] // Wired from input/touch.rs in a follow-up
    pub fn swipe_horizontal(&mut self, now_ms: u64) -> Option<HunOutcome> {
        let outcome = self.state.swipe_horizontal(now_ms);
        if outcome.is_some() {
            self.sync_active_after_transition();
        }
        outcome
    }

    /// Whether a HUN banner is currently visible.
    #[must_use]
    pub fn is_visible(&self) -> bool {
        self.state.active().is_some() && self.active_model.is_some()
    }

    /// Force a re-raster on the next frame if currently visible (e.g. theme reload).
    pub fn mark_stale(&mut self) {
        if self.is_visible() {
            self.dirty = true;
            self.tex = None;
        }
    }

    /// Screen-space rectangle of the active HUN banner on `output`, if visible.
    #[must_use]
    pub fn banner_rect(&self, output: Size) -> Option<Rect> {
        if !self.is_visible() {
            return None;
        }
        let data = self.active_model.as_ref()?;
        Some(hun_banner::banner_rect(
            output,
            self.top_inset,
            !data.actions.is_empty(),
        ))
    }

    /// Rasterize the active banner when dirty and return `(pixels, screen_rect)`.
    pub fn take_render_pixels(&mut self, theme: &Theme, output: Size) -> Option<(Pixels, Rect)> {
        if !self.dirty {
            return None;
        }
        let rect = self.banner_rect(output)?;
        let data = self.active_model.as_ref()?;
        let px = hun_banner::render(theme, &data.as_model(), rect.size());
        self.dirty = false;
        Some((px, rect))
    }

    fn sync_active_after_transition(&mut self) {
        self.px = None;
        self.tex = None;
        if let Some(next_id) = self.state.active().map(|a| a.id) {
            if let Some(pos) = self.queued_models.iter().position(|m| m.id == next_id) {
                self.active_model = Some(self.queued_models.remove(pos));
                self.dirty = true;
            } else {
                self.active_model = None;
                self.dirty = false;
            }
        } else {
            self.active_model = None;
            self.dirty = false;
        }
    }
}

impl De {
    /// Rasterize the active HUN banner when dirty.
    pub(crate) fn hun_prepare(&mut self, output: Size) {
        if let Some((px, _rect)) = self.hun_fx.take_render_pixels(&self.theme, output) {
            self.hun_fx.px = Some(px);
            self.hun_fx.generation = self.hun_fx.generation.wrapping_add(1);
        } else if !self.hun_fx.is_visible() {
            self.hun_fx.px = None;
            self.hun_fx.tex = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use de_ui::hun::{HunPriority, DEFAULT_TIMEOUT_MS};

    fn normal(id: u32) -> (HunCandidate, HunBannerData) {
        (
            HunCandidate {
                id,
                priority: HunPriority::Normal,
                is_incoming_call: false,
            },
            HunBannerData {
                id,
                app: "Chat".to_string(),
                title: format!("Message {id}"),
                body: "Hello".to_string(),
                actions: vec![("reply".to_string(), "Reply".to_string())],
                critical: false,
            },
        )
    }

    #[test]
    fn enqueue_visibility_and_render_dirty_cycle() {
        let mut fx = HunFx::new();
        let output = Size::new(720, 1600);
        assert!(!fx.is_visible());
        assert!(fx.banner_rect(output).is_none());
        assert!(fx.take_render_pixels(&tokens::DARK, output).is_none());

        // Suppressed under DND.
        let (cand, data) = normal(1);
        let dnd_ctx = HunContext {
            dnd: true,
            shade_open: false,
            locked: false,
        };
        assert!(!fx.enqueue(cand, data, dnd_ctx, 0));
        assert!(!fx.is_visible());

        // Accepted when unlocked and DND off.
        let (cand, data) = normal(2);
        assert!(fx.enqueue(cand, data, HunContext::default(), 100));
        assert!(fx.is_visible());
        let rect = fx.banner_rect(output).expect("visible banner rect");
        assert_eq!(
            rect.y,
            tokens::STATUS_BAR_HEIGHT + hun_banner::BANNER_TOP_GAP
        );
        assert_eq!(rect.h, hun_banner::BANNER_H_ACTIONS);

        // First render produces pixels and clears dirty flag.
        let (px, drawn_rect) = fx
            .take_render_pixels(&tokens::DARK, output)
            .expect("dirty banner renders");
        assert_eq!(drawn_rect, rect);
        assert_eq!(px.size, rect.size());
        assert!(fx.take_render_pixels(&tokens::DARK, output).is_none());

        // mark_stale re-arms rasterization.
        fx.mark_stale();
        assert!(fx.take_render_pixels(&tokens::LIGHT, output).is_some());
    }

    #[test]
    fn swipe_up_snooze_and_swipe_horizontal_dismiss_promote_queue() {
        let mut fx = HunFx::new();
        let ctx = HunContext::default();
        let output = Size::new(720, 1600);

        let (c1, d1) = normal(10);
        let (c2, d2) = normal(20);
        assert!(fx.enqueue(c1, d1, ctx, 0));
        assert!(fx.enqueue(c2, d2, ctx, 50));

        assert_eq!(fx.swipe_up(100), Some(HunOutcome::Snoozed(10)));
        assert!(fx.is_visible());
        assert_eq!(fx.active_model.as_ref().map(|m| m.id), Some(20));
        assert!(fx.take_render_pixels(&tokens::DARK, output).is_some());

        assert_eq!(fx.swipe_horizontal(200), Some(HunOutcome::Dismissed(20)));
        assert!(!fx.is_visible());
        assert!(fx.banner_rect(output).is_none());

        // mark_stale is a no-op when hidden.
        fx.mark_stale();
        assert!(fx.take_render_pixels(&tokens::DARK, output).is_none());
    }

    #[test]
    fn tick_expiry_respects_touch_hold() {
        let mut fx = HunFx::new();
        let (cand, data) = normal(30);
        assert!(fx.enqueue(cand, data, HunContext::default(), 1_000));

        fx.set_touch_held(true, 3_000);
        assert_eq!(fx.tick(20_000), None);
        assert!(fx.is_visible());

        fx.set_touch_held(false, 20_000);
        assert_eq!(fx.tick(20_000 + DEFAULT_TIMEOUT_MS - 2_001), None);
        assert_eq!(
            fx.tick(20_000 + DEFAULT_TIMEOUT_MS - 2_000),
            Some(HunOutcome::Expired(30))
        );
        assert!(!fx.is_visible());
    }
}
