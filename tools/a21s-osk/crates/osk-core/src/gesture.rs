//! Touch gesture recognition, independent of geometry: the caller hit-tests points against
//! the layout and passes the result in. One entry per contact id, so two-thumb typing works.

use std::collections::BTreeMap;

use crate::layout::KeyPos;

/// Gboard shows the alternates popup after ~400 ms.
pub const DEFAULT_LONG_PRESS_MS: u64 = 400;
/// A key repeats (backspace) every 50 ms once held past the long-press threshold.
pub const DEFAULT_REPEAT_MS: u64 = 50;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TouchEvent {
    pub id: u64,
    pub x: f32,
    pub y: f32,
    pub at_ms: u64,
    pub kind: TouchKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TouchKind {
    Down,
    Move,
    Up,
    Cancel,
}

impl TouchEvent {
    pub fn down(id: u64, x: f32, y: f32, at_ms: u64) -> Self {
        TouchEvent {
            id,
            x,
            y,
            at_ms,
            kind: TouchKind::Down,
        }
    }
    pub fn mv(id: u64, x: f32, y: f32, at_ms: u64) -> Self {
        TouchEvent {
            id,
            x,
            y,
            at_ms,
            kind: TouchKind::Move,
        }
    }
    pub fn up(id: u64, x: f32, y: f32, at_ms: u64) -> Self {
        TouchEvent {
            id,
            x,
            y,
            at_ms,
            kind: TouchKind::Up,
        }
    }
    pub fn cancel(id: u64, at_ms: u64) -> Self {
        TouchEvent {
            id,
            x: 0.0,
            y: 0.0,
            at_ms,
            kind: TouchKind::Cancel,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum GestureEvent {
    /// Touch-down on a key: highlight it and show the preview bubble.
    Press(KeyPos),
    /// Held past the long-press threshold: open the alternates popup.
    LongPress(KeyPos),
    /// Held still longer: repeat the key (backspace, cursor keys).
    Repeat(KeyPos),
    /// Released after the finger left the surface of the key: cancel, no commit.
    Cancel(KeyPos),
    /// Released on the key it went down on without a long press: commit a tap.
    Tap(KeyPos),
    /// Released after sliding horizontally (space-bar cursor move, or a swipe gesture).
    Swipe { key: KeyPos, dx: f32, dy: f32 },
}

#[derive(Debug, Clone, Copy)]
struct Active {
    start: KeyPos,
    start_x: f32,
    start_y: f32,
    x: f32,
    y: f32,
    start_ms: u64,
    left: bool,
    long_fired: bool,
    last_repeat_ms: u64,
}

pub struct GestureDetector {
    pub long_press_ms: u64,
    pub repeat_ms: u64,
    pub move_tolerance_px: f32,
    /// Horizontal distance that turns a release into a swipe instead of a tap.
    pub swipe_threshold_px: f32,
    touches: BTreeMap<u64, Active>,
}

impl Default for GestureDetector {
    fn default() -> Self {
        GestureDetector {
            long_press_ms: DEFAULT_LONG_PRESS_MS,
            repeat_ms: DEFAULT_REPEAT_MS,
            move_tolerance_px: 8.0,
            swipe_threshold_px: 24.0,
            touches: BTreeMap::new(),
        }
    }
}

impl GestureDetector {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn active_touches(&self) -> usize {
        self.touches.len()
    }

    /// Feed one touch event. `hit` is the key under the pointer for this event's position.
    pub fn handle(&mut self, event: TouchEvent, hit: Option<KeyPos>) -> Vec<GestureEvent> {
        match event.kind {
            TouchKind::Down => {
                let Some(key) = hit else { return Vec::new() };
                self.touches.insert(
                    event.id,
                    Active {
                        start: key,
                        start_x: event.x,
                        start_y: event.y,
                        x: event.x,
                        y: event.y,
                        start_ms: event.at_ms,
                        left: false,
                        long_fired: false,
                        last_repeat_ms: event.at_ms,
                    },
                );
                vec![GestureEvent::Press(key)]
            }
            TouchKind::Move => {
                let Some(t) = self.touches.get_mut(&event.id) else {
                    return Vec::new();
                };
                t.x = event.x;
                t.y = event.y;
                let dx = t.x - t.start_x;
                let dy = t.y - t.start_y;
                let moved = (dx * dx + dy * dy).sqrt() > self.move_tolerance_px;
                if moved && hit != Some(t.start) && !t.left {
                    t.left = true;
                    return vec![GestureEvent::Cancel(t.start)];
                }
                Vec::new()
            }
            TouchKind::Up => {
                let Some(t) = self.touches.remove(&event.id) else {
                    return Vec::new();
                };
                let dx = event.x - t.start_x;
                let dy = event.y - t.start_y;
                if t.left || hit != Some(t.start) || t.long_fired {
                    return vec![GestureEvent::Cancel(t.start)];
                }
                if dx.abs() > self.swipe_threshold_px && dx.abs() > dy.abs() {
                    return vec![GestureEvent::Swipe {
                        key: t.start,
                        dx,
                        dy,
                    }];
                }
                vec![GestureEvent::Tap(t.start)]
            }
            TouchKind::Cancel => self
                .touches
                .remove(&event.id)
                .map(|t| vec![GestureEvent::Cancel(t.start)])
                .unwrap_or_default(),
        }
    }

    /// Drop every contact (the compositor sends `wl_touch.cancel` without ids).
    pub fn cancel_all(&mut self, at_ms: u64) -> Vec<GestureEvent> {
        let keys: Vec<u64> = self.touches.keys().copied().collect();
        let mut out = Vec::new();
        for id in keys {
            out.extend(self.handle(TouchEvent::cancel(id, at_ms), None));
        }
        out
    }

    /// Call from the render loop while any touch is active: fires long-press and repeat.
    pub fn tick(&mut self, now_ms: u64) -> Vec<GestureEvent> {
        let mut out = Vec::new();
        for t in self.touches.values_mut() {
            let held = now_ms.saturating_sub(t.start_ms);
            if t.left {
                continue;
            }
            if !t.long_fired && held >= self.long_press_ms {
                t.long_fired = true;
                t.last_repeat_ms = now_ms;
                out.push(GestureEvent::LongPress(t.start));
            } else if t.long_fired && now_ms.saturating_sub(t.last_repeat_ms) >= self.repeat_ms {
                t.last_repeat_ms = now_ms;
                out.push(GestureEvent::Repeat(t.start));
            }
        }
        out
    }
}
