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
    /// Bottom-strip upward swipe followed by a ≥ `HOLD_MS` stall.
    Recents,
    /// Short touch with little movement.
    Tap { pos: Point },
    /// Touch held ≥ `HOLD_MS` without moving past the slop.
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
    /// A content drag (past the slop) ended: displacement and release
    /// velocity (px/s, from the last ~50 ms of movement, not the average) so
    /// the UI can project where the content should settle.
    Released { dx: f32, dy: f32, vx: f32, vy: f32 },
    /// Bottom-strip drag released; velocity uses the same recent samples as content.
    NavReleased { dy: f32, vy: f32 },
    /// Finger moved after a long-press (item lifted for rearranging).
    HoldDrag { pos: Point },
    /// Finger lifted after a long-press drag.
    HoldDrop { pos: Point },
    /// Contact ended before qualifying (or a second finger arrived).
    Cancelled,
}

/// Destination of a bottom navigation drag. Distances scale with the output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavDestination {
    Home,
    Recents,
    Cancel,
}

#[must_use]
pub fn decide_navigation(dy: f32, vy: f32, height: f32) -> NavDestination {
    let rise = -dy / height.max(1.0);
    if rise >= 0.75 || (rise >= 0.007 && vy <= -500.0) {
        NavDestination::Home
    } else if rise >= 0.20 {
        NavDestination::Recents
    } else {
        NavDestination::Cancel
    }
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
            nav_h: 40,
            top_h: 64,
            edge_w: 44,
            slop: 12.0,
            back_dx: 40.0,
            hold_ms: 400,
            fling_v: 0.45,
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
    /// The commit was a long-press: later motion is a hold-drag.
    held: bool,
    /// Smoothed velocity in px/ms.
    vx: f32,
    vy: f32,
    /// Held position at `HOLD_MS` check (recents needs a stall, not a pa`rk`).
    stall_y: f32,
    stall_ms: u64,
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
    #[allow(clippy::too_many_lines)] // gesture state machine; splitting hurts clarity
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
                    held: false,
                    vx: 0.0,
                    vy: 0.0,
                    stall_y: 0.0,
                    stall_ms: ev.time_ms,
                });
                Some(GestureEvent::Started { zone, pos: ev.pos })
            }
            TouchPhase::Motion => {
                let tr = self.track.as_mut()?;
                if tr.held {
                    tr.last = ev.pos;
                    return Some(GestureEvent::HoldDrag { pos: ev.pos });
                }
                let dt = ev.time_ms.saturating_sub(tr.last_ms);
                if dt > 0 {
                    let a = 0.5;
                    tr.vx = a * ((ev.pos.x - tr.last.x) as f32 / dt as f32) + (1.0 - a) * tr.vx;
                    tr.vy = a * ((ev.pos.y - tr.last.y) as f32 / dt as f32) + (1.0 - a) * tr.vy;
                }
                tr.dx = (ev.pos.x - tr.start.x) as f32;
                tr.dy = (ev.pos.y - tr.start.y) as f32;
                tr.last = ev.pos;
                tr.last_ms = ev.time_ms;
                // A hold starts after rising 20% and measures actual time at
                // rest, rather than time since the initial contact.
                if tr.zone == Zone::NavBar && !tr.committed {
                    if tr.dy > -(self.cfg.output_h as f32 * 0.20)
                        || tr.stall_y == 0.0
                        || (tr.dy - tr.stall_y).abs() >= self.cfg.slop
                    {
                        tr.stall_y = tr.dy;
                        tr.stall_ms = ev.time_ms;
                    } else if ev.time_ms.saturating_sub(tr.stall_ms) >= self.cfg.hold_ms {
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
                    tr.held = true;
                    return Some(GestureEvent::Recognized(Gesture::LongPress {
                        pos: tr.start,
                    }));
                }
                let moved = tr.dx.abs() >= self.cfg.slop || tr.dy.abs() >= self.cfg.slop;
                if !tr.committed && (tr.zone != Zone::Content || moved) {
                    return Some(GestureEvent::Progress {
                        zone: tr.zone,
                        dx: tr.dx,
                        dy: tr.dy,
                    });
                }
                None
            }
            TouchPhase::Up | TouchPhase::Cancel => {
                let tr = self.track.take()?;
                if tr.held && ev.phase == TouchPhase::Up {
                    return Some(GestureEvent::HoldDrop { pos: ev.pos });
                }
                if ev.phase == TouchPhase::Cancel || tr.committed {
                    return Some(GestureEvent::Cancelled);
                }
                let dt = (ev.time_ms - tr.start_ms).max(1) as f32;
                let g = match tr.zone {
                    Zone::NavBar => {
                        if tr.dx.abs() >= self.cfg.slop || tr.dy.abs() >= self.cfg.slop {
                            let GestureEvent::Released { dy, vy, .. } =
                                Self::released(&tr, ev.time_ms)
                            else {
                                unreachable!()
                            };
                            return Some(GestureEvent::NavReleased { dy, vy });
                        }
                        Some(Gesture::Tap { pos: tr.start })
                    }
                    Zone::EdgeLeft | Zone::EdgeRight => {
                        let inward = if tr.zone == Zone::EdgeLeft {
                            tr.dx
                        } else {
                            -tr.dx
                        };
                        let stale = ev.time_ms.saturating_sub(tr.last_ms) >= 80;
                        // `tr.vx` is already in px/ms (matching `fling_v`).
                        let recent_vx = if stale { 0.0 } else { tr.vx };
                        let fallback_vx = tr.dx / dt;
                        let eff_vx = if recent_vx.abs() > fallback_vx.abs() {
                            recent_vx
                        } else {
                            fallback_vx
                        };
                        let inward_v = if tr.zone == Zone::EdgeLeft {
                            eff_vx
                        } else {
                            -eff_vx
                        };
                        if (inward >= self.cfg.back_dx || inward_v >= self.cfg.fling_v)
                            && inward.abs() > tr.dy.abs() * 1.1
                        {
                            Some(Gesture::EdgeBack {
                                from_left: tr.zone == Zone::EdgeLeft,
                            })
                        } else if tr.dx.abs() < self.cfg.slop && tr.dy.abs() < self.cfg.slop {
                            Some(Gesture::Tap { pos: tr.start })
                        } else {
                            None
                        }
                    }
                    // Content and the top strip: a tap, or a drag that ends with a velocity.
                    Zone::Content | Zone::TopEdge => {
                        if tr.dx.abs() < self.cfg.slop && tr.dy.abs() < self.cfg.slop {
                            Some(Gesture::Tap { pos: tr.start })
                        } else {
                            return Some(Self::released(&tr, ev.time_ms));
                        }
                    }
                };
                g.map_or(Some(GestureEvent::Cancelled), |g| {
                    Some(GestureEvent::Recognized(g))
                })
            }
        }
    }

    /// The release of a drag: displacement plus the velocity of the last
    /// moments. A stale velocity (the finger rested before lifting) is no fling.
    fn released(tr: &Track, now_ms: u64) -> GestureEvent {
        let fresh = now_ms.saturating_sub(tr.last_ms) < 80;
        let k = if fresh { 1000.0 } else { 0.0 };
        GestureEvent::Released {
            dx: tr.dx,
            dy: tr.dy,
            vx: tr.vx * k,
            vy: tr.vy * k,
        }
    }

    /// Deadline-driven holds also fire when a stationary touchscreen emits
    /// no Motion events. The caller uses the same monotonic clock as input.
    pub fn tick(&mut self, now_ms: u64) -> Option<GestureEvent> {
        let tr = self.track.as_mut()?;
        if tr.committed { return None; }
        if tr.zone == Zone::NavBar && tr.dy <= -(self.cfg.output_h as f32 * 0.15)
            && now_ms.saturating_sub(tr.stall_ms) >= self.cfg.hold_ms {
            tr.committed = true;
            return Some(GestureEvent::Recognized(Gesture::Recents));
        }
        if tr.dx.abs() < self.cfg.slop && tr.dy.abs() < self.cfg.slop
            && now_ms.saturating_sub(tr.start_ms) >= self.cfg.hold_ms {
            tr.committed = true;
            tr.held = true;
            return Some(GestureEvent::Recognized(Gesture::LongPress { pos: tr.start }));
        }
        None
    }

    /// Whether a finger is currently down.
    #[must_use]
    pub fn active(&self) -> bool {
        self.track.is_some()
    }
}

/// Predictive Back feedback: a curved water-drop bell attached flush to the
/// touched edge, growing smoothly inward with a `<` / `>` chevron inside.
#[must_use]
pub fn back_feedback(size: de_api::Size, left: bool, y: i32, inward: f32) -> (de_api::Pixels, Point) {
    use de_api::Size;
    use de_design::tokens;
    use de_paint::Canvas;
    let t = tokens::DEFAULT;
    let progress = (inward / 96.0).clamp(0.0, 1.0);
    let w = (16.0 + 44.0 * progress).round() as i32;
    let h = 260;
    let mut c = Canvas::new(Size::new(w, h));
    let fill = if inward >= 48.0 {
        [t.accent[0], t.accent[1], t.accent[2], 230]
    } else {
        [28, 24, 32, 200]
    };
    let width = w as usize;
    let data = c.pixmap_mut().data_mut();
    // Premultiply fill color.
    let fa = u32::from(fill[3]);
    let pr = (u32::from(fill[0]) * fa + 127) / 255;
    let pg = (u32::from(fill[1]) * fa + 127) / 255;
    let pb = (u32::from(fill[2]) * fa + 127) / 255;
    for row in 0..h {
        let u = (row as f32 + 0.5) / h as f32;
        // Raised-cosine bell: smooth S-curve tangent at top/bottom, rounded belly.
        let bell = (0.5 * (1.0 - (std::f32::consts::TAU * u).cos())).powf(1.6);
        let extent = w as f32 * bell;
        for col in 0..w {
            let dist_from_edge = if left {
                col as f32 + 0.5
            } else {
                (w - 1 - col) as f32 + 0.5
            };
            let cov = (extent - dist_from_edge + 0.5).clamp(0.0, 1.0);
            if cov <= 0.0 {
                continue;
            }
            let m = (cov * 255.0).round() as u32;
            let i = (row as usize * width + col as usize) * 4;
            data[i] = ((pr * m + 127) / 255) as u8;
            data[i + 1] = ((pg * m + 127) / 255) as u8;
            data[i + 2] = ((pb * m + 127) / 255) as u8;
            data[i + 3] = ((fa * m + 127) / 255) as u8;
        }
    }
    if w >= 28 {
        let glyph = if left { "\u{e5cb}" } else { "\u{e5cc}" };
        let color = if inward >= 48.0 {
            t.text_on_accent
        } else {
            [0xFF, 0xF8, 0xF0, 0xFF]
        };
        let gx = if left { 2 } else { w - 34 };
        crate::chrome::draw_ink_centered(
            &mut c,
            glyph,
            &de_paint::text::LabelStyle::icon(32.0, color),
            de_api::Rect::new(gx, (h - 32) / 2, 32, 32),
        );
    }
    let at = Point::new(
        if left { 0 } else { size.w - w },
        (y - h / 2).clamp(0, (size.h - h).max(0)),
    );
    (c.into_pixels(), at)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tr() -> GestureTracker {
        GestureTracker::new(GestureConfig::a21s())
    }

    fn ev(phase: TouchPhase, x: i32, y: i32, ms: u64) -> TouchEvent {
        TouchEvent {
            slot: 0,
            phase,
            pos: Point::new(x, y),
            time_ms: ms,
        }
    }

    #[test]
    fn home_swipe_from_nav_strip() {
        let mut t = tr();
        assert_eq!(
            t.feed(&ev(TouchPhase::Down, 360, 1590, 0)),
            Some(GestureEvent::Started {
                zone: Zone::NavBar,
                pos: Point::new(360, 1590)
            })
        );
        t.feed(&ev(TouchPhase::Motion, 360, 1400, 100));
        assert_eq!(
            t.feed(&ev(TouchPhase::Up, 360, 1400, 200)),
            Some(GestureEvent::NavReleased {
                dy: -190.0,
                vy: 0.0
            })
        );
    }

    #[test]
    fn swipe_up_hold_is_recents() {
        let mut t = tr();
        t.feed(&ev(TouchPhase::Down, 360, 1590, 0));
        t.feed(&ev(TouchPhase::Motion, 360, 1200, 100)); // rose >20%
        assert_eq!(
            t.feed(&ev(TouchPhase::Motion, 360, 1198, 520)), // stalled ≥400 ms
            Some(GestureEvent::Recognized(Gesture::Recents))
        );
        assert_eq!(
            t.feed(&ev(TouchPhase::Up, 360, 1198, 540)),
            Some(GestureEvent::Cancelled)
        );
    }

    #[test]
    fn navigation_destinations_follow_distance_and_recent_velocity() {
        use NavDestination::{Cancel, Home, Recents};
        assert_eq!(decide_navigation(-100.0, -900.0, 1600.0), Home);
        assert_eq!(decide_navigation(-800.0, 0.0, 1600.0), Recents);
        assert_eq!(decide_navigation(-1200.0, 0.0, 1600.0), Home);
        assert_eq!(decide_navigation(-200.0, 0.0, 1600.0), Cancel);
        assert_eq!(decide_navigation(100.0, 900.0, 1600.0), Cancel);
        assert_eq!(decide_navigation(-480.0, 0.0, 2400.0), Recents);
    }

    #[test]
    fn shallow_hold_and_slow_continuous_rise_do_not_open_recents() {
        let mut t = tr();
        t.feed(&ev(TouchPhase::Down, 360, 1590, 0));
        t.feed(&ev(TouchPhase::Motion, 360, 1480, 100));
        assert!(matches!(
            t.feed(&ev(TouchPhase::Motion, 360, 1480, 600)),
            Some(GestureEvent::Progress { .. })
        ));
        t.feed(&ev(TouchPhase::Motion, 360, 1200, 700));
        assert!(matches!(
            t.feed(&ev(TouchPhase::Motion, 360, 1100, 1100)),
            Some(GestureEvent::Progress { .. })
        ));
    }

    #[test]
    fn nav_rest_before_release_discards_old_fling() {
        let mut t = tr();
        t.feed(&ev(TouchPhase::Down, 360, 1590, 0));
        t.feed(&ev(TouchPhase::Motion, 360, 1200, 100));
        assert_eq!(
            t.feed(&ev(TouchPhase::Up, 360, 1200, 300)),
            Some(GestureEvent::NavReleased {
                dy: -390.0,
                vy: 0.0
            })
        );
    }

    #[test]
    fn hold_fires_without_another_motion_event() {
        let mut t = tr();
        t.feed(&ev(TouchPhase::Down, 360, 1590, 1000));
        t.feed(&ev(TouchPhase::Motion, 360, 1200, 1100));
        assert_eq!(t.tick(1499), None);
        assert_eq!(t.tick(1500), Some(GestureEvent::Recognized(Gesture::Recents)));
        assert_eq!(t.tick(1600), None);
    }

    #[test]
    fn edge_back_left_only_horizontal() {
        let mut t = tr();
        t.feed(&ev(TouchPhase::Down, 8, 800, 0));
        t.feed(&ev(TouchPhase::Motion, 80, 810, 100));
        assert_eq!(
            t.feed(&ev(TouchPhase::Up, 80, 810, 200)),
            Some(GestureEvent::Recognized(Gesture::EdgeBack {
                from_left: true
            }))
        );
        // Mostly-vertical edge swipe is not back.
        let mut t = tr();
        t.feed(&ev(TouchPhase::Down, 8, 800, 0));
        t.feed(&ev(TouchPhase::Motion, 40, 1000, 150));
        assert_eq!(
            t.feed(&ev(TouchPhase::Up, 40, 1000, 250)),
            Some(GestureEvent::Cancelled)
        );
    }

    #[test]
    fn right_edge_back() {
        let mut t = tr();
        t.feed(&ev(TouchPhase::Down, 712, 700, 0));
        t.feed(&ev(TouchPhase::Motion, 640, 705, 100));
        assert_eq!(
            t.feed(&ev(TouchPhase::Up, 640, 705, 180)),
            Some(GestureEvent::Recognized(Gesture::EdgeBack {
                from_left: false
            }))
        );
    }

    #[test]
    fn top_edge_pull_reports_progress_then_release_with_velocity() {
        let mut t = tr();
        t.feed(&ev(TouchPhase::Down, 360, 20, 0));
        assert!(matches!(
            t.feed(&ev(TouchPhase::Motion, 360, 200, 100)),
            Some(GestureEvent::Progress {
                zone: Zone::TopEdge,
                ..
            })
        ));
        t.feed(&ev(TouchPhase::Motion, 360, 300, 120));
        match t.feed(&ev(TouchPhase::Up, 360, 300, 130)) {
            Some(GestureEvent::Released { dy, vy, .. }) => {
                assert_eq!(dy, 280.0);
                assert!(vy > 500.0);
            }
            other => panic!("{other:?}"),
        }
        // A tap in the top strip stays a tap.
        let mut t = tr();
        t.feed(&ev(TouchPhase::Down, 360, 20, 0));
        assert!(matches!(
            t.feed(&ev(TouchPhase::Up, 361, 21, 80)),
            Some(GestureEvent::Recognized(Gesture::Tap { .. }))
        ));
    }

    #[test]
    fn content_drag_follows_then_releases_with_velocity() {
        let mut t = tr();
        t.feed(&ev(TouchPhase::Down, 300, 900, 0));
        // Inside the slop: nothing yet.
        assert_eq!(t.feed(&ev(TouchPhase::Motion, 296, 901, 20)), None);
        assert_eq!(
            t.feed(&ev(TouchPhase::Motion, 240, 905, 60)),
            Some(GestureEvent::Progress {
                zone: Zone::Content,
                dx: -60.0,
                dy: 5.0
            })
        );
        t.feed(&ev(TouchPhase::Motion, 180, 905, 100));
        match t.feed(&ev(TouchPhase::Up, 180, 905, 110)) {
            Some(GestureEvent::Released { dx, vx, .. }) => {
                assert_eq!(dx, -120.0);
                assert!(vx < -500.0, "fast leftward release, got {vx}");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn resting_before_lift_is_not_a_fling() {
        let mut t = tr();
        t.feed(&ev(TouchPhase::Down, 300, 900, 0));
        t.feed(&ev(TouchPhase::Motion, 200, 900, 50));
        match t.feed(&ev(TouchPhase::Up, 200, 900, 400)) {
            Some(GestureEvent::Released { vx, .. }) => assert_eq!(vx, 0.0),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn long_press_then_drag_is_a_hold_drag() {
        let mut t = tr();
        t.feed(&ev(TouchPhase::Down, 300, 900, 0));
        assert!(matches!(
            t.feed(&ev(TouchPhase::Motion, 301, 900, 450)),
            Some(GestureEvent::Recognized(Gesture::LongPress { .. }))
        ));
        assert_eq!(
            t.feed(&ev(TouchPhase::Motion, 340, 1000, 500)),
            Some(GestureEvent::HoldDrag {
                pos: Point::new(340, 1000)
            })
        );
        assert_eq!(
            t.feed(&ev(TouchPhase::Up, 340, 1400, 600)),
            Some(GestureEvent::HoldDrop {
                pos: Point::new(340, 1400)
            })
        );
    }

    #[test]
    fn tap_is_tap() {
        let mut t = tr();
        t.feed(&ev(TouchPhase::Down, 300, 900, 0));
        assert_eq!(
            t.feed(&ev(TouchPhase::Up, 302, 902, 120)),
            Some(GestureEvent::Recognized(Gesture::Tap {
                pos: Point::new(300, 900)
            }))
        );
    }

    #[test]
    fn hold_without_move_is_long_press() {
        let mut t = tr();
        t.feed(&ev(TouchPhase::Down, 300, 900, 0));
        assert_eq!(
            t.feed(&ev(TouchPhase::Motion, 302, 901, 450)),
            Some(GestureEvent::Recognized(Gesture::LongPress {
                pos: Point::new(300, 900)
            }))
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
