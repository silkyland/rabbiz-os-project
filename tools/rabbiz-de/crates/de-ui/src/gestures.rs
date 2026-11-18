//! Gesture recognition (M3-T6): classifies the touch stream into the
//! shell's system gestures — edge-back, home swipe, swipe-up-hold recents,
//! shade pull, dock/home page switch — plus tap and long-press for icons.
//!
//! Fed `TouchEvent`s (libinput `time_ms`, so release velocity is real) and
//! emits [`GestureEvent`]s the compositor acts on. The tracker is
//! screen-agnostic: it reports *where* a gesture started and how it moved,
//! and `de-core` decides whether the current screen honours it (e.g. edge
//! swipe on Home = page switch, on an app = predictive back pill, §P13).

use de_api::input::{TouchEvent, TouchPhase};
use de_api::Point;

/// Screen-zone where the touch started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zone {
    /// Bottom gesture-bar strip (home swipe / recents hold).
    NavBar,
    /// Top strip (shade pull).
    TopEdge,
    /// Left/right 24 px edge (edge-back / predictive back).
    EdgeLeft,
    EdgeRight,
    /// Anything else (content: taps, scrolls, page swipes).
    Content,
}

/// A finished or in-progress gesture.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Gesture {
    /// Edge drag that qualifies as back (dx > 48 px, mostly horizontal).
    EdgeBack { from_left: bool },
    /// Bottom-strip upward swipe (fast, or past half the screen).
    Home,
    /// Bottom-strip upward swipe followed by a ≥ HOLD_MS stall.
    Recents,
    /// Top-edge downward drag (progress via [`GestureEvent::Progress`]).
    ShadePull,
    /// Horizontal swipe in content (home dock/grid paging: dir < 0 = next).
    PageSwitch { dir: i32 },
    /// Short touch with little movement.
    Tap { pos: Point },
    /// Touch held ≥ HOLD_MS without moving past the slop.
    LongPress { pos: Point },
}

/// Tracker output — one per consumed `TouchEvent` (or `tick`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GestureEvent {
    /// Contact started in `zone` at `pos` (first finger only).
    Started { zone: Zone, pos: Point },
    /// Finger moved in a recognised-or-candidate system gesture — the UI
    /// follows 1:1 (§7.17a finger-tracked motion). `dx`/`dy` from start.
    Progress { zone: Zone, dx: f32, dy: f32 },
    /// Gesture committed (release, or threshold+hold for recents/long-press).
    Recognized(Gesture),
    /// Contact ended before qualifying (or a second finger arrived).
    Cancelled,
}

/// Geometry + thresholds (physical px / ms).
#[derive(Debug, Clone, Copy)]
pub struct GestureConfig {
    pub output_w: i32,
    pub output_h: i32,
    /// Bottom nav strip height (matches `GESTURE_BAR_HEIGHT`).
    pub nav_h: i32,
    /// Top strip height (status bar + slop).
    pub top_h: i32,
    /// Side-edge width for edge-back.
    pub edge_w: i32,
    /// Movement below which a touch stays a tap/long-press.
    pub slop: f32,
    /// Edge-back horizontal commit distance.
    pub back_dx: f32,
    /// Swipe-up distance that commits "home" even at slow speed.
    pub home_dy: f32,
    /// Hold duration for recents and long-press.
    pub hold_ms: u64,
    /// Release speed (px/ms) that commits a swipe regardless of distance.
    pub fling_v: f32,
}

impl GestureConfig {
    /// A21s panel defaults (720×1600).
    #[must_use]
    pub fn a21s() -> Self {
        Self {
            output_w: 720,
            output_h: 1600,
            nav_h: 32,
            top_h: 64,
            edge_w: 24,
            slop: 12.0,
            back_dx: 48.0,
            home_dy: 96.0,
            hold_ms: 400,
            fling_v: 0.5,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Track {
    zone: Zone,
    start: Point,
    last: Point,
    start_ms: u64,
    last_ms: u64,
    /// Accumulated displacement at last event.
    dx: f32,
    dy: f32,
    /// Already committed mid-gesture (recents/long-press fired early).
    committed: bool,
    /// Held position at HOLD_MS check (recents needs a stall, not a park).
    stall_y: f32,
}

/// Single-finger tracker — the first slot owns the gesture; extra fingers
/// cancel it (multi-touch system gestures are a later milestone).
#[derive(Debug)]
pub struct GestureTracker {
    cfg: GestureConfig,
    track: Option<Track>,
}

impl GestureTracker {
    #[must_use]
    pub fn new(cfg: GestureConfig) -> Self {
        Self { cfg, track: None }
    }

    /// Which zone `pos` falls in.
    #[must_use]
    pub fn zone(&self, pos: Point) -> Zone {
        if pos.y >= self.cfg.output_h - self.cfg.nav_h {
            Zone::NavBar
        } else if pos.y < self.cfg.top_h {
            Zone::TopEdge
        } else if pos.x < self.cfg.edge_w {
            Zone::EdgeLeft
        } else if pos.x >= self.cfg.output_w - self.cfg.edge_w {
            Zone::EdgeRight
        } else {
            Zone::Content
        }
    }

    /// Feed a touch event; at most one event out per call.
    pub fn feed(&mut self, ev: &TouchEvent) -> Option<GestureEvent> {
        match ev.phase {
            TouchPhase::Down => {
                if self.track.is_some() {
                    // Second finger — kill the in-flight gesture.
                    self.track = None;
                    return Some(GestureEvent::Cancelled);
                }
                let zone = self.zone(ev.pos);
                self.track = Some(Track {
                    zone,
                    start: ev.pos,
                    last: ev.pos,
                    start_ms: ev.time_ms,
                    last_ms: ev.time_ms,
                    dx: 0.0,
                    dy: 0.0,
                    committed: false,
                    stall_y: 0.0,
                });
                Some(GestureEvent::Started { zone, pos: ev.pos })
            }
            TouchPhase::Motion => {
                let tr = self.track.as_mut()?;
                tr.dx = (ev.pos.x - tr.start.x) as f32;
                tr.dy = (ev.pos.y - tr.start.y) as f32;
                tr.last = ev.pos;
                tr.last_ms = ev.time_ms;
                // Recents: rose past home_dy then stalled ≥ hold_ms.
                if tr.zone == Zone::NavBar && !tr.committed {
                    if tr.dy <= -self.cfg.home_dy && tr.stall_y == 0.0 {
                        tr.stall_y = tr.dy;
                    }
                    if tr.stall_y != 0.0
                        && ev.time_ms - tr.start_ms >= self.cfg.hold_ms
                        && (tr.dy - tr.stall_y).abs() < self.cfg.slop
                    {
                        tr.committed = true;
                        return Some(GestureEvent::Recognized(Gesture::Recents));
                    }
                }
                // Long-press: still inside the slop, time crossed hold_ms.
                if !tr.committed
                    && tr.dx.abs() < self.cfg.slop
                    && tr.dy.abs() < self.cfg.slop
                    && ev.time_ms - tr.start_ms >= self.cfg.hold_ms
                {
                    tr.committed = true;
                    return Some(GestureEvent::Recognized(Gesture::LongPress { pos: tr.start }));
                }
                if tr.zone != Zone::Content && !tr.committed {
                    return Some(GestureEvent::Progress { zone: tr.zone, dx: tr.dx, dy: tr.dy });
                }
                None
            }
            TouchPhase::Up | TouchPhase::Cancel => {
                let tr = self.track.take()?;
                if ev.phase == TouchPhase::Cancel || tr.committed {
                    return Some(GestureEvent::Cancelled);
                }
                let dt = (ev.time_ms - tr.start_ms).max(1) as f32;
                let vx = tr.dx / dt;
                let vy = tr.dy / dt;
                let g = match tr.zone {
                    Zone::NavBar => {
                        if tr.dy <= -self.cfg.home_dy || vy <= -self.cfg.fling_v {
                            Some(Gesture::Home)
                        } else {
                            Some(Gesture::Tap { pos: tr.start })
                        }
                    }
                    Zone::TopEdge => {
                        if tr.dy >= self.cfg.home_dy || vy >= self.cfg.fling_v {
                            Some(Gesture::ShadePull)
                        } else {
                            Some(Gesture::Tap { pos: tr.start })
                        }
                    }
                    Zone::EdgeLeft | Zone::EdgeRight => {
                        let inward = if tr.zone == Zone::EdgeLeft { tr.dx } else { -tr.dx };
                        let inward_v = if tr.zone == Zone::EdgeLeft { vx } else { -vx };
                        if (inward >= self.cfg.back_dx || inward_v >= self.cfg.fling_v)
                            && inward.abs() > tr.dy.abs() * 1.4
                        {
                            Some(Gesture::EdgeBack { from_left: tr.zone == Zone::EdgeLeft })
                        } else {
                            None
                        }
                    }
                    Zone::Content => {
                        if tr.dx.abs() < self.cfg.slop && tr.dy.abs() < self.cfg.slop {
                            Some(Gesture::Tap { pos: tr.start })
                        } else if tr.dx.abs() > tr.dy.abs()
                            && (tr.dx.abs() >= self.cfg.back_dx || vx.abs() >= self.cfg.fling_v)
                        {
                            Some(Gesture::PageSwitch { dir: if tr.dx < 0.0 { -1 } else { 1 } })
                        } else {
                            None
                        }
                    }
                };
                g.map_or(Some(GestureEvent::Cancelled), |g| {
                    Some(GestureEvent::Recognized(g))
                })
            }
        }
    }

    /// Whether a finger is currently down.
    #[must_use]
    pub fn active(&self) -> bool {
        self.track.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tr() -> GestureTracker {
        GestureTracker::new(GestureConfig::a21s())
    }

    fn ev(phase: TouchPhase, x: i32, y: i32, ms: u64) -> TouchEvent {
        TouchEvent { slot: 0, phase, pos: Point::new(x, y), time_ms: ms }
    }

    #[test]
    fn home_swipe_from_nav_strip() {
        let mut t = tr();
        assert_eq!(
            t.feed(&ev(TouchPhase::Down, 360, 1590, 0)),
            Some(GestureEvent::Started { zone: Zone::NavBar, pos: Point::new(360, 1590) })
        );
        t.feed(&ev(TouchPhase::Motion, 360, 1400, 100));
        assert_eq!(
            t.feed(&ev(TouchPhase::Up, 360, 1400, 200)),
            Some(GestureEvent::Recognized(Gesture::Home))
        );
    }

    #[test]
    fn swipe_up_hold_is_recents() {
        let mut t = tr();
        t.feed(&ev(TouchPhase::Down, 360, 1590, 0));
        t.feed(&ev(TouchPhase::Motion, 360, 1480, 100)); // rose >96 px
        assert_eq!(
            t.feed(&ev(TouchPhase::Motion, 360, 1478, 420)), // stalled ≥400 ms
            Some(GestureEvent::Recognized(Gesture::Recents))
        );
        assert_eq!(t.feed(&ev(TouchPhase::Up, 360, 1478, 440)), Some(GestureEvent::Cancelled));
    }

    #[test]
    fn edge_back_left_only_horizontal() {
        let mut t = tr();
        t.feed(&ev(TouchPhase::Down, 8, 800, 0));
        t.feed(&ev(TouchPhase::Motion, 80, 810, 100));
        assert_eq!(
            t.feed(&ev(TouchPhase::Up, 80, 810, 200)),
            Some(GestureEvent::Recognized(Gesture::EdgeBack { from_left: true }))
        );
        // Mostly-vertical edge swipe is not back.
        let mut t = tr();
        t.feed(&ev(TouchPhase::Down, 8, 800, 0));
        t.feed(&ev(TouchPhase::Motion, 40, 1000, 150));
        assert_eq!(t.feed(&ev(TouchPhase::Up, 40, 1000, 250)), Some(GestureEvent::Cancelled));
    }

    #[test]
    fn right_edge_back() {
        let mut t = tr();
        t.feed(&ev(TouchPhase::Down, 712, 700, 0));
        t.feed(&ev(TouchPhase::Motion, 640, 705, 100));
        assert_eq!(
            t.feed(&ev(TouchPhase::Up, 640, 705, 180)),
            Some(GestureEvent::Recognized(Gesture::EdgeBack { from_left: false }))
        );
    }

    #[test]
    fn top_edge_pull_is_shade() {
        let mut t = tr();
        t.feed(&ev(TouchPhase::Down, 360, 20, 0));
        t.feed(&ev(TouchPhase::Motion, 360, 300, 120));
        assert_eq!(
            t.feed(&ev(TouchPhase::Up, 360, 300, 240)),
            Some(GestureEvent::Recognized(Gesture::ShadePull))
        );
    }

    #[test]
    fn content_swipe_is_page_switch_tap_is_tap() {
        let mut t = tr();
        t.feed(&ev(TouchPhase::Down, 300, 900, 0));
        t.feed(&ev(TouchPhase::Motion, 180, 905, 100));
        assert_eq!(
            t.feed(&ev(TouchPhase::Up, 180, 905, 200)),
            Some(GestureEvent::Recognized(Gesture::PageSwitch { dir: -1 }))
        );
        let mut t = tr();
        t.feed(&ev(TouchPhase::Down, 300, 900, 0));
        assert_eq!(
            t.feed(&ev(TouchPhase::Up, 302, 902, 120)),
            Some(GestureEvent::Recognized(Gesture::Tap { pos: Point::new(300, 900) }))
        );
    }

    #[test]
    fn hold_without_move_is_long_press() {
        let mut t = tr();
        t.feed(&ev(TouchPhase::Down, 300, 900, 0));
        assert_eq!(
            t.feed(&ev(TouchPhase::Motion, 302, 901, 450)),
            Some(GestureEvent::Recognized(Gesture::LongPress { pos: Point::new(300, 900) }))
        );
    }

    #[test]
    fn second_finger_cancels() {
        let mut t = tr();
        t.feed(&ev(TouchPhase::Down, 360, 1590, 0));
        assert_eq!(
            t.feed(&ev(TouchPhase::Down, 100, 800, 10)),
            Some(GestureEvent::Cancelled)
        );
        assert!(!t.active());
    }
}
