//! Pure Heads-Up Notification (HUN) state machine: gating policy, 5 s
//! auto-dismiss timer with touch-hold pause, swipe gestures, and FIFO queue
//! promotion.

use std::collections::VecDeque;

/// Default auto-dismiss timeout for normal-priority HUN banners (5 seconds).
pub const DEFAULT_TIMEOUT_MS: u64 = 5_000;
const MAX_QUEUED: usize = 16;

/// Urgency level mapped into the HUN gating policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HunPriority {
    Low,
    Normal,
    Critical,
}

/// Candidate notification considered for Heads-Up display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HunCandidate {
    pub id: u32,
    pub priority: HunPriority,
    pub is_incoming_call: bool,
}

impl HunCandidate {
    #[must_use]
    const fn is_persistent(self) -> bool {
        matches!(self.priority, HunPriority::Critical) || self.is_incoming_call
    }
}

/// Shell state evaluated when deciding whether a HUN may be shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HunContext {
    pub dnd: bool,
    pub shade_open: bool,
    pub locked: bool,
}

/// Terminal outcome of the currently active HUN banner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HunOutcome {
    /// Swipe up: hides banner while keeping the notification in the shade.
    Snoozed(u32),
    /// Swipe left/right: dismisses the notification completely.
    Dismissed(u32),
    /// Auto-dismiss timeout elapsed.
    Expired(u32),
}

#[derive(Debug, Clone, Copy)]
struct ActiveHun {
    candidate: HunCandidate,
    remaining_ms: u64,
    last_ms: u64,
}

impl ActiveHun {
    const fn new(candidate: HunCandidate, now_ms: u64, timeout_ms: u64) -> Self {
        Self {
            candidate,
            remaining_ms: timeout_ms,
            last_ms: now_ms,
        }
    }
}

/// Heads-Up Notification banner queue and timer state machine.
#[derive(Debug, Clone)]
pub struct HunState {
    default_timeout_ms: u64,
    active: Option<ActiveHun>,
    queue: VecDeque<HunCandidate>,
    touch_held: bool,
}

impl Default for HunState {
    fn default() -> Self {
        Self::new(DEFAULT_TIMEOUT_MS)
    }
}

impl HunState {
    /// Create a new HUN state machine with the given default auto-dismiss timeout.
    #[must_use]
    pub fn new(default_timeout_ms: u64) -> Self {
        Self {
            default_timeout_ms,
            active: None,
            queue: VecDeque::new(),
            touch_held: false,
        }
    }

    /// Evaluate the HUN policy for `item` under `ctx`. Returns `true` if the
    /// candidate was accepted as active or queued, or `false` if suppressed.
    pub fn enqueue(&mut self, item: HunCandidate, ctx: HunContext, now_ms: u64) -> bool {
        if !should_show_hun(item, ctx) {
            return false;
        }
        if let Some(active) = &mut self.active {
            if active.candidate.id == item.id {
                *active = ActiveHun::new(item, now_ms, self.default_timeout_ms);
                return true;
            }
            if let Some(queued) = self.queue.iter_mut().find(|q| q.id == item.id) {
                *queued = item;
                return true;
            }
            if self.queue.len() >= MAX_QUEUED {
                self.queue.pop_front();
            }
            self.queue.push_back(item);
        } else {
            self.active = Some(ActiveHun::new(item, now_ms, self.default_timeout_ms));
            self.touch_held = false;
        }
        true
    }

    /// Currently visible HUN candidate, if any.
    #[must_use]
    pub fn active(&self) -> Option<&HunCandidate> {
        self.active.as_ref().map(|a| &a.candidate)
    }

    /// Pause (`held = true`) or resume (`held = false`) the auto-dismiss timer
    /// while a finger is touching the banner.
    pub fn set_touch_held(&mut self, held: bool, now_ms: u64) {
        if self.touch_held == held {
            if let Some(active) = &mut self.active {
                if held {
                    active.last_ms = now_ms;
                }
            }
            return;
        }
        if let Some(active) = &mut self.active {
            if held {
                let elapsed = now_ms.saturating_sub(active.last_ms);
                active.remaining_ms = active.remaining_ms.saturating_sub(elapsed);
            }
            active.last_ms = now_ms;
        }
        self.touch_held = held;
    }

    /// Advance the timer to `now_ms`. Auto-expires normal HUNs once their
    /// timeout elapses (unless touch-held) and promotes the next queued HUN.
    /// Critical and incoming-call HUNs never auto-expire.
    pub fn tick(&mut self, now_ms: u64) -> Option<HunOutcome> {
        let active = self.active.as_mut()?;
        if active.candidate.is_persistent() {
            active.last_ms = now_ms;
            return None;
        }
        if self.touch_held {
            active.last_ms = now_ms;
            return None;
        }
        let elapsed = now_ms.saturating_sub(active.last_ms);
        active.remaining_ms = active.remaining_ms.saturating_sub(elapsed);
        active.last_ms = now_ms;
        if active.remaining_ms == 0 {
            let id = active.candidate.id;
            self.promote_next(now_ms);
            Some(HunOutcome::Expired(id))
        } else {
            None
        }
    }

    /// Swipe up on the banner: hides it while keeping the notification in the shade.
    pub fn swipe_up(&mut self, now_ms: u64) -> Option<HunOutcome> {
        let id = self.active.as_ref()?.candidate.id;
        self.promote_next(now_ms);
        Some(HunOutcome::Snoozed(id))
    }

    /// Swipe left or right on the banner: dismisses the notification completely.
    pub fn swipe_horizontal(&mut self, now_ms: u64) -> Option<HunOutcome> {
        let id = self.active.as_ref()?.candidate.id;
        self.promote_next(now_ms);
        Some(HunOutcome::Dismissed(id))
    }

    fn promote_next(&mut self, now_ms: u64) {
        self.touch_held = false;
        self.active = self
            .queue
            .pop_front()
            .map(|next| ActiveHun::new(next, now_ms, self.default_timeout_ms));
    }
}

const fn should_show_hun(item: HunCandidate, ctx: HunContext) -> bool {
    if ctx.locked {
        return false;
    }
    if item.is_persistent() {
        return true;
    }
    matches!(item.priority, HunPriority::Normal) && !ctx.dnd && !ctx.shade_open
}

#[cfg(test)]
mod tests {
    use super::*;

    fn normal(id: u32) -> HunCandidate {
        HunCandidate {
            id,
            priority: HunPriority::Normal,
            is_incoming_call: false,
        }
    }

    fn low(id: u32) -> HunCandidate {
        HunCandidate {
            id,
            priority: HunPriority::Low,
            is_incoming_call: false,
        }
    }

    fn critical(id: u32) -> HunCandidate {
        HunCandidate {
            id,
            priority: HunPriority::Critical,
            is_incoming_call: false,
        }
    }

    fn incoming_call(id: u32) -> HunCandidate {
        HunCandidate {
            id,
            priority: HunPriority::Normal,
            is_incoming_call: true,
        }
    }

    #[test]
    fn gating_respects_dnd_shade_lock_and_low_priority() {
        let mut hun = HunState::new(DEFAULT_TIMEOUT_MS);

        // Low priority never shows even when unlocked with DND off and shade closed.
        assert!(!hun.enqueue(low(1), HunContext::default(), 0));
        assert!(hun.active().is_none());

        // Normal is suppressed by DND, open shade, or lockscreen.
        assert!(!hun.enqueue(
            normal(2),
            HunContext {
                dnd: true,
                shade_open: false,
                locked: false,
            },
            0
        ));
        assert!(!hun.enqueue(
            normal(3),
            HunContext {
                dnd: false,
                shade_open: true,
                locked: false,
            },
            0
        ));
        assert!(!hun.enqueue(
            normal(4),
            HunContext {
                dnd: false,
                shade_open: false,
                locked: true,
            },
            0
        ));
        assert!(hun.active().is_none());

        // Normal shows when unlocked, DND off, and shade closed.
        assert!(hun.enqueue(normal(5), HunContext::default(), 100));
        assert_eq!(hun.active().map(|a| a.id), Some(5));
    }

    #[test]
    fn critical_and_incoming_call_override_dnd_but_never_show_locked() {
        let mut hun = HunState::new(DEFAULT_TIMEOUT_MS);
        let dnd_ctx = HunContext {
            dnd: true,
            shade_open: false,
            locked: false,
        };
        let locked_ctx = HunContext {
            dnd: false,
            shade_open: false,
            locked: true,
        };

        assert!(!hun.enqueue(critical(1), locked_ctx, 0));
        assert!(!hun.enqueue(incoming_call(2), locked_ctx, 0));
        assert!(hun.active().is_none());

        assert!(hun.enqueue(critical(3), dnd_ctx, 0));
        assert_eq!(hun.active().map(|a| a.id), Some(3));
        // Critical never auto-expires.
        assert_eq!(hun.tick(60_000), None);
        assert_eq!(hun.active().map(|a| a.id), Some(3));

        // Incoming call also overrides DND and never auto-expires once promoted.
        assert!(hun.enqueue(incoming_call(4), dnd_ctx, 60_000));
        assert_eq!(hun.swipe_horizontal(60_000), Some(HunOutcome::Dismissed(3)));
        assert_eq!(hun.active().map(|a| a.id), Some(4));
        assert_eq!(hun.tick(120_000), None);
        assert_eq!(hun.active().map(|a| a.id), Some(4));
    }

    #[test]
    fn touch_hold_pauses_auto_dismiss_until_released() {
        let mut hun = HunState::new(5_000);
        assert!(hun.enqueue(normal(10), HunContext::default(), 1_000));

        // 2s elapsed -> 3s remaining.
        assert_eq!(hun.tick(3_000), None);
        hun.set_touch_held(true, 4_000); // 1s more elapsed -> 2s remaining.

        // Held for 20s: must not expire.
        assert_eq!(hun.tick(24_000), None);
        assert_eq!(hun.active().map(|a| a.id), Some(10));

        // Release at 24s -> expires at 26s.
        hun.set_touch_held(false, 24_000);
        assert_eq!(hun.tick(25_999), None);
        assert_eq!(hun.tick(26_000), Some(HunOutcome::Expired(10)));
        assert!(hun.active().is_none());
    }

    #[test]
    fn swipe_up_snoozes_swipe_horizontal_dismisses_and_promotes_queue() {
        let mut hun = HunState::new(5_000);
        let ctx = HunContext::default();
        assert!(hun.enqueue(normal(1), ctx, 0));
        assert!(hun.enqueue(normal(2), ctx, 100));
        assert!(hun.enqueue(normal(3), ctx, 200));

        assert_eq!(hun.active().map(|a| a.id), Some(1));
        assert_eq!(hun.swipe_up(500), Some(HunOutcome::Snoozed(1)));
        assert_eq!(hun.active().map(|a| a.id), Some(2));

        assert_eq!(hun.swipe_horizontal(1_000), Some(HunOutcome::Dismissed(2)));
        assert_eq!(hun.active().map(|a| a.id), Some(3));

        // Promoted item 3 gets a fresh 5_000 ms timeout starting at 1_000.
        assert_eq!(hun.tick(5_999), None);
        assert_eq!(hun.tick(6_000), Some(HunOutcome::Expired(3)));
        assert!(hun.active().is_none());
        assert_eq!(hun.swipe_up(6_000), None);
        assert_eq!(hun.swipe_horizontal(6_000), None);
    }
}
