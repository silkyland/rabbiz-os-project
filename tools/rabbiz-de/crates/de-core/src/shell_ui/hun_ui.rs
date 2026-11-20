//! Heads-Up Notification (HUN) compositor overlay controller: wraps
//! [`de_ui::hun::HunState`] with owned banner data, cached GPU texture slot,
//! and on-demand pixel rasterization for `DisplayArea::Overlay`.

use std::any::Any;
use std::time::Instant;

use de_api::{Pixels, Point, Rect, Size};
use de_design::tokens::{self, Theme};
use de_ui::hun::{HunCandidate, HunContext, HunOutcome, HunPriority, HunState};
use de_ui::screens::hun_banner::{self, HunBannerHit, HunBannerModel};

use crate::state::De;

/// Vertical drag delta (logical px) that snoozes the active HUN banner.
const SWIPE_UP_THRESHOLD: i32 = -32;
/// Horizontal drag distance (logical px) that dismisses the active HUN banner.
const SWIPE_HORIZONTAL_THRESHOLD: i32 = 72;

/// Active touch contact tracked on the floating HUN banner.
#[derive(Debug, Clone, Copy, Default)]
struct HunTouchDrag {
    active: bool,
    slot: Option<i32>,
    start: Point,
    last: Point,
}

/// Outcome of releasing a tracked touch contact on the HUN banner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HunTouchAction {
    /// Swiped up: banner snoozed (notification remains in the shade).
    Snoozed(u32),
    /// Swiped left/right: notification dismissed completely.
    Dismissed(u32),
    /// Tapped the banner card body (banner snoozed).
    TappedBody(u32),
    /// Tapped an action pill button (banner dismissed).
    TappedAction { id: u32, action_index: usize },
    /// Released without a qualifying swipe or in-banner tap.
    Released,
}

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
    drag: HunTouchDrag,
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
            drag: HunTouchDrag::default(),
        }
    }

    /// Evaluate the HUN gating policy and enqueue `data` if accepted.
    ///
    /// Returns `true` if the notification became active or was queued.
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
    pub fn set_touch_held(&mut self, held: bool, now_ms: u64) {
        self.state.set_touch_held(held, now_ms);
    }

    /// Swipe up on the active banner: snoozes the banner while keeping it in the shade.
    pub fn swipe_up(&mut self, now_ms: u64) -> Option<HunOutcome> {
        let outcome = self.state.swipe_up(now_ms);
        if outcome.is_some() {
            self.sync_active_after_transition();
        }
        outcome
    }

    /// Swipe left or right on the active banner: dismisses the notification completely.
    pub fn swipe_horizontal(&mut self, now_ms: u64) -> Option<HunOutcome> {
        let outcome = self.state.swipe_horizontal(now_ms);
        if outcome.is_some() {
            self.sync_active_after_transition();
        }
        outcome
    }

    /// Capture a touch-down landing on the visible HUN banner and pause its timer.
    pub fn on_touch_down(&mut self, slot: i32, pt: Point, output: Size, now_ms: u64) -> bool {
        if self.drag.active || !self.is_visible() {
            return false;
        }
        let Some(rect) = self.banner_rect(output) else {
            return false;
        };
        if !rect.contains(pt) {
            return false;
        }
        self.drag = HunTouchDrag {
            active: true,
            slot: Some(slot),
            start: pt,
            last: pt,
        };
        self.set_touch_held(true, now_ms);
        true
    }

    /// Track motion for the captured HUN touch slot.
    pub fn on_touch_motion(&mut self, slot: i32, pt: Point) -> bool {
        if !self.drag.active || self.drag.slot != Some(slot) {
            return false;
        }
        self.drag.last = pt;
        true
    }

    /// Resolve a tracked HUN touch release into a swipe, tap, or plain release.
    pub fn on_touch_up(
        &mut self,
        slot: i32,
        pt: Point,
        output: Size,
        now_ms: u64,
    ) -> Option<HunTouchAction> {
        if !self.drag.active || self.drag.slot != Some(slot) {
            return None;
        }
        self.drag.last = pt;
        let drag = std::mem::take(&mut self.drag);
        self.set_touch_held(false, now_ms);

        let dx = drag.last.x - drag.start.x;
        let dy = drag.last.y - drag.start.y;
        if dy <= SWIPE_UP_THRESHOLD {
            return match self.swipe_up(now_ms) {
                Some(HunOutcome::Snoozed(id)) => Some(HunTouchAction::Snoozed(id)),
                _ => Some(HunTouchAction::Released),
            };
        }
        if dx.abs() >= SWIPE_HORIZONTAL_THRESHOLD {
            return match self.swipe_horizontal(now_ms) {
                Some(HunOutcome::Dismissed(id)) => Some(HunTouchAction::Dismissed(id)),
                _ => Some(HunTouchAction::Released),
            };
        }

        let hit_info = self.banner_rect(output).and_then(|rect| {
            let data = self.active_model.as_ref()?;
            let hit = hun_banner::hit(rect, drag.last, &data.as_model())?;
            Some((data.id, hit))
        });
        match hit_info {
            Some((id, HunBannerHit::Action(action_index))) => {
                self.swipe_horizontal(now_ms);
                Some(HunTouchAction::TappedAction { id, action_index })
            }
            Some((id, HunBannerHit::Body)) => {
                self.swipe_up(now_ms);
                Some(HunTouchAction::TappedBody(id))
            }
            None => Some(HunTouchAction::Released),
        }
    }

    /// Cancel an in-progress HUN touch drag and resume the auto-dismiss timer.
    pub fn on_touch_cancel(&mut self, slot: i32, now_ms: u64) -> bool {
        if !self.drag.active || self.drag.slot != Some(slot) {
            return false;
        }
        self.drag = HunTouchDrag::default();
        self.set_touch_held(false, now_ms);
        true
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

/// Convert a D-Bus notification entry into HUN policy candidate + banner payload.
pub(crate) fn entry_to_hun(
    entry: &de_system::notifications::Entry,
) -> (HunCandidate, HunBannerData) {
    let priority = match entry.urgency {
        de_system::notifications::Urgency::Low => HunPriority::Low,
        de_system::notifications::Urgency::Normal => HunPriority::Normal,
        de_system::notifications::Urgency::Critical => HunPriority::Critical,
    };
    let is_incoming_call = entry
        .category
        .as_deref()
        .is_some_and(|c| c == "call.incoming" || c == "call");
    let candidate = HunCandidate {
        id: entry.id,
        priority,
        is_incoming_call,
    };
    let data = HunBannerData {
        id: entry.id,
        app: entry.app.clone(),
        title: entry.title.clone(),
        body: entry.body.clone(),
        actions: entry
            .actions
            .iter()
            .map(|a| (a.key.clone(), a.label.clone()))
            .collect(),
        critical: matches!(entry.urgency, de_system::notifications::Urgency::Critical),
    };
    (candidate, data)
}

impl De {
    /// Enqueue a newly arrived notification entry into the HUN overlay controller.
    pub(crate) fn hun_enqueue_entry(
        &mut self,
        entry: &de_system::notifications::Entry,
        now: Instant,
    ) -> bool {
        let (candidate, data) = entry_to_hun(entry);
        let ctx = HunContext {
            dnd: self.app_prefs.prefs().do_not_disturb,
            shade_open: self.panels.shown(),
            locked: self.locked() || self.is_session_locked(),
        };
        let now_ms = self.uptime_ms(now);
        self.hun_fx.enqueue(candidate, data, ctx, now_ms)
    }

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
    use de_ui::hun::DEFAULT_TIMEOUT_MS;

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

    #[test]
    fn touch_down_capture_holds_timer_and_ignores_outside_or_wrong_slot() {
        let mut fx = HunFx::new();
        let output = Size::new(720, 1600);
        let (cand, data) = normal(40);
        assert!(fx.enqueue(cand, data, HunContext::default(), 1_000));
        let rect = fx.banner_rect(output).expect("visible banner");

        // Outside the banner is not captured.
        assert!(!fx.on_touch_down(0, Point::new(rect.x, rect.y + rect.h + 40), output, 1_500));

        // Inside the banner is captured and pauses expiry.
        let start = Point::new(rect.x + rect.w / 2, rect.y + 30);
        assert!(fx.on_touch_down(1, start, output, 2_000));
        assert_eq!(fx.tick(20_000), None);
        assert!(fx.is_visible());

        // Different slot is ignored; matching slot is tracked.
        assert!(!fx.on_touch_motion(2, Point::new(start.x + 10, start.y)));
        assert_eq!(fx.on_touch_up(2, start, output, 20_000), None);
        assert!(fx.on_touch_motion(1, Point::new(start.x, rect.y + rect.h + 20)));

        // Releasing outside banner below swipe thresholds returns Released and resumes timer.
        assert_eq!(
            fx.on_touch_up(1, Point::new(start.x, rect.y + rect.h + 20), output, 20_000),
            Some(HunTouchAction::Released)
        );
        assert!(fx.is_visible());
        assert_eq!(
            fx.tick(20_000 + DEFAULT_TIMEOUT_MS - 1_000),
            Some(HunOutcome::Expired(40))
        );
    }

    #[test]
    fn touch_swipe_up_snoozes_and_swipe_horizontal_dismisses() {
        let mut fx = HunFx::new();
        let output = Size::new(720, 1600);
        let (c1, d1) = normal(50);
        let (c2, d2) = normal(51);
        assert!(fx.enqueue(c1, d1, HunContext::default(), 0));
        assert!(fx.enqueue(c2, d2, HunContext::default(), 50));

        let rect = fx.banner_rect(output).expect("first banner");
        let start = Point::new(rect.x + rect.w / 2, rect.y + 60);
        assert!(fx.on_touch_down(0, start, output, 100));
        assert!(fx.on_touch_motion(0, Point::new(start.x, start.y - 40)));
        assert_eq!(
            fx.on_touch_up(0, Point::new(start.x, start.y - 40), output, 150),
            Some(HunTouchAction::Snoozed(50))
        );
        // Queued banner 51 is promoted.
        assert!(fx.is_visible());
        assert_eq!(fx.active_model.as_ref().map(|m| m.id), Some(51));

        let rect2 = fx.banner_rect(output).expect("second banner");
        let start2 = Point::new(rect2.x + rect2.w / 2, rect2.y + 60);
        assert!(fx.on_touch_down(1, start2, output, 200));
        assert!(fx.on_touch_motion(1, Point::new(start2.x + 80, start2.y)));
        assert_eq!(
            fx.on_touch_up(1, Point::new(start2.x + 80, start2.y), output, 250),
            Some(HunTouchAction::Dismissed(51))
        );
        assert!(!fx.is_visible());
    }

    #[test]
    fn touch_tap_body_and_tap_action_pill() {
        let mut fx = HunFx::new();
        let output = Size::new(720, 1600);
        let (c1, d1) = normal(60);
        let (c2, mut d2) = normal(61);
        d2.actions = vec![
            ("reply".to_string(), "Reply".to_string()),
            ("mute".to_string(), "Mute".to_string()),
        ];
        assert!(fx.enqueue(c1, d1, HunContext::default(), 0));
        assert!(fx.enqueue(c2, d2, HunContext::default(), 10));

        // Tap upper body area -> TappedBody(60), snoozes banner and promotes 61.
        let rect1 = fx.banner_rect(output).expect("banner 60");
        let body_pt = Point::new(rect1.x + rect1.w / 2, rect1.y + 30);
        assert!(fx.on_touch_down(0, body_pt, output, 100));
        assert_eq!(
            fx.on_touch_up(0, body_pt, output, 140),
            Some(HunTouchAction::TappedBody(60))
        );
        assert_eq!(fx.active_model.as_ref().map(|m| m.id), Some(61));

        // Tap second action pill (right half of the bottom action row) -> TappedAction { id: 61, action_index: 1 }.
        let rect2 = fx.banner_rect(output).expect("banner 61");
        let action_pt = Point::new(rect2.x + (rect2.w * 3) / 4, rect2.y + rect2.h - 32);
        assert!(fx.on_touch_down(0, action_pt, output, 200));
        assert_eq!(
            fx.on_touch_up(0, action_pt, output, 240),
            Some(HunTouchAction::TappedAction {
                id: 61,
                action_index: 1,
            })
        );
        assert!(!fx.is_visible());
    }
}
