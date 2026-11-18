//! Motion tokens from plan §7.17/§7.17a: spring presets, the fixed-duration
//! fallback ladder (safe mode / reduced motion), gesture and degrade rules.
//!
//! All numbers are starting hypotheses pending owner feel review on the A21s;
//! they are typed constants so a single edit retunes the whole system.

/// The property driven by a motion channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotionKind {
    /// Position, rotation, size or corner radius. Uses the preset's bounce.
    Spatial,
    /// Opacity, color or blur radius. Stays between the animation endpoints.
    Effects,
}

/// Session-wide spring tuning. Standard preserves the original presets.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum MotionScheme {
    #[default]
    Standard,
    Expressive,
}

impl MotionScheme {
    /// Expressive adds bounce to the small-control spatial preset. Soft
    /// already uses the system ceiling. Smooth covers apps and sheets.
    #[must_use]
    pub const fn spring_bounce(self, preset: SpringPreset, kind: MotionKind) -> f32 {
        match (self, preset, kind) {
            (Self::Expressive, SpringPreset::Snappy, MotionKind::Spatial) => 0.16,
            _ => preset.bounce_for(kind),
        }
    }
}

/// Spring preset names (§7.17a table). `duration` = settle time within 1 %,
/// `bounce` = fractional overshoot at first crossing (0 = critically damped).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpringPreset {
    /// App open/close, sheets, shade, drawer, page changes, recents — 450 ms,
    /// zero bounce.
    Smooth,
    /// Toggles, buttons on release, sliders, small elements — 350 ms, 12 %.
    Snappy,
    /// Charging pill and one-off delight moments — 500 ms, 25 % (max one
    /// per screen).
    Soft,
}

impl SpringPreset {
    /// Settle-within-1-% duration in seconds.
    #[must_use]
    pub const fn duration_s(self) -> f32 {
        match self {
            Self::Smooth => 0.450,
            Self::Snappy => 0.350,
            Self::Soft => 0.500,
        }
    }

    /// Fractional overshoot (0 = critically damped, no bounce).
    #[must_use]
    pub const fn bounce(self) -> f32 {
        match self {
            Self::Smooth => 0.0,
            Self::Snappy => 0.12,
            Self::Soft => 0.25,
        }
    }

    /// Effects are critically damped regardless of preset.
    #[must_use]
    pub const fn bounce_for(self, kind: MotionKind) -> f32 {
        match kind {
            MotionKind::Spatial => self.bounce(),
            MotionKind::Effects => 0.0,
        }
    }

    /// Maximum allowed bounce on app/sheet motion (§7.17a: app motion has
    /// zero bounce; small bounce only on small playful elements).
    pub const APP_MOTION_BOUNCE_MAX: f32 = 0.0;
    /// Maximum bounce anywhere in the system.
    pub const BOUNCE_MAX: f32 = 0.25;
}

/// Overscroll "rubber" response (§7.17a): displacement = `RUBBER_FACTOR` x
/// drag distance past the limit, then springs back with `Smooth`.
pub const RUBBER_FACTOR: f32 = 0.55;

/// Finger-tracking rules (§7.17a gesture rules 2 and 5).
/// On release the target is chosen from the projected end position
/// `pos + velocity * PROJECT_AHEAD_S`, not the position alone.
pub const PROJECT_AHEAD_S: f32 = 0.25;
/// Press feedback scale on touch-down.
pub const PRESS_SCALE: f32 = 0.96;
/// Press feedback animation duration.
pub const PRESS_SCALE_MS: u32 = 90;
/// Staggered-entrance step (drawer grid, notification stack).
pub const STAGGER_MS: u32 = 20;
/// Max staggered items.
pub const STAGGER_MAX: usize = 8;
/// Hard cap: nothing animates longer than this (§7.17a "Rhythm").
pub const DURATION_MAX_MS: u32 = 600;

/// Fixed-duration ladder in ms (fallback path and general durations).
pub const DURATIONS_MS: [u32; 5] = [90, 160, 240, 360, 520];

/// Speed by the fraction of the output covered by the moving surface.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum SpeedTier {
    Fast,
    #[default]
    Default,
    Slow,
}

impl SpeedTier {
    #[must_use]
    pub fn for_covered_fraction(fraction: f32) -> Self {
        if fraction < 0.10 {
            Self::Fast
        } else if fraction >= 0.75 {
            Self::Slow
        } else {
            Self::Default
        }
    }

    fn enter_ms(self) -> u32 {
        let rung = match self {
            Self::Fast => 1,
            Self::Default => 3,
            Self::Slow => 4,
        };
        DURATIONS_MS[rung].min(DURATION_MAX_MS)
    }
}

/// Exit intent determines the endpoint velocity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitKind {
    /// The surface comes to rest at the edge so it can be pulled back.
    Temporary,
    /// A committed removal accelerates through the edge.
    Permanent,
}

/// Shell transitions and their reference tiers. Geometry may override the
/// reference tier, for example when an app window covers only part of an output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transition {
    Boot,
    Lock,
    Home,
    HomePage,
    Drawer,
    Recents,
    RecentPage,
    RecentCard,
    ClearRecents,
    Shade,
    Notification,
    QuickSettings,
    Osd,
    Power,
    Search,
    Screenshot,
    Error,
    FirstRun,
    Settings,
    AppWindow,
    AppToHome,
    Osk,
    Toggle,
    ChargingPill,
}

impl Transition {
    /// Reference table for shell surfaces and actions. The match is exhaustive
    /// so adding a transition requires a tier and exit policy.
    #[must_use]
    pub const fn timing(self) -> TransitionTiming {
        use ExitKind::{Permanent, Temporary};
        use SpeedTier::{Default, Fast, Slow};

        let (tier, exit_kind) = match self {
            Self::Boot
            | Self::Lock
            | Self::Home
            | Self::HomePage
            | Self::Recents
            | Self::AppToHome
            | Self::Search
            | Self::FirstRun
            | Self::Settings => (Slow, Temporary),
            Self::Drawer
            | Self::Shade
            | Self::QuickSettings
            | Self::Power
            | Self::Error
            | Self::RecentPage
            | Self::Osk => (Default, Temporary),
            Self::Osd | Self::Screenshot | Self::Toggle | Self::ChargingPill => (Fast, Temporary),
            Self::Notification => (Fast, Permanent),
            Self::RecentCard => (Default, Permanent),
            Self::AppWindow | Self::ClearRecents => (Slow, Permanent),
        };
        TransitionTiming { tier, exit_kind }
    }

    /// Resolve the tier from the visible area of the animated surface divided
    /// by the output area. Static backdrops are outside the covered area.
    #[must_use]
    pub fn timing_for_covered_fraction(self, fraction: f32) -> TransitionTiming {
        TransitionTiming {
            tier: SpeedTier::for_covered_fraction(fraction),
            ..self.timing()
        }
    }
}

/// Normal transition timing. Reduced motion and the degrade ladder select
/// their own existing fallback timings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransitionTiming {
    pub tier: SpeedTier,
    pub exit_kind: ExitKind,
}

impl TransitionTiming {
    #[must_use]
    pub fn enter(self) -> TimedMotion {
        TimedMotion {
            duration_ms: self.tier.enter_ms(),
            curve: BEZIER_ENTER,
        }
    }

    #[must_use]
    pub fn exit(self) -> TimedMotion {
        TimedMotion {
            duration_ms: self.tier.enter_ms() * 45 / 100,
            curve: match self.exit_kind {
                ExitKind::Temporary => BEZIER_ENTER,
                ExitKind::Permanent => BEZIER_EXIT,
            },
        }
    }
}

/// A bounded, fixed-duration motion segment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimedMotion {
    pub duration_ms: u32,
    pub curve: Bezier,
}

impl TimedMotion {
    #[must_use]
    pub fn progress_at(self, elapsed_ms: f32) -> f32 {
        bezier_eval(self.curve, elapsed_ms / self.duration_ms as f32)
    }
}

/// Cubic-bezier easing curve for the fallback ladder.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bezier {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
}

/// `standard (.2,0,0,1)` — general movement fallback.
pub const BEZIER_STANDARD: Bezier = Bezier {
    x1: 0.2,
    y1: 0.0,
    x2: 0.0,
    y2: 1.0,
};
/// `enter (.05,.7,.1,1)` — elements appearing/expanding.
pub const BEZIER_ENTER: Bezier = Bezier {
    x1: 0.05,
    y1: 0.7,
    x2: 0.1,
    y2: 1.0,
};
/// `exit (.3,0,.8,.15)` — elements leaving/collapsing.
pub const BEZIER_EXIT: Bezier = Bezier {
    x1: 0.3,
    y1: 0.0,
    x2: 0.8,
    y2: 0.15,
};

/// Standard decelerate — utility entrances that settle into place.
pub const BEZIER_STANDARD_DECELERATE: Bezier = Bezier {
    x1: 0.0,
    y1: 0.0,
    x2: 0.0,
    y2: 1.0,
};
/// Standard accelerate — utility exits that pick up speed.
pub const BEZIER_STANDARD_ACCELERATE: Bezier = Bezier {
    x1: 0.3,
    y1: 0.0,
    x2: 1.0,
    y2: 1.0,
};

const EMPHASIZED_JOIN_X: f32 = 0.166_666;
const EMPHASIZED_JOIN_Y: f32 = 0.4;
// Normalize each reference segment to a unit square for `bezier_eval`.
const EMPHASIZED_FIRST: Bezier = Bezier {
    x1: 0.05 / EMPHASIZED_JOIN_X,
    y1: 0.0,
    x2: 0.133_333 / EMPHASIZED_JOIN_X,
    y2: 0.06 / EMPHASIZED_JOIN_Y,
};
const EMPHASIZED_SECOND: Bezier = Bezier {
    x1: (0.208_333 - EMPHASIZED_JOIN_X) / (1.0 - EMPHASIZED_JOIN_X),
    y1: (0.82 - EMPHASIZED_JOIN_Y) / (1.0 - EMPHASIZED_JOIN_Y),
    x2: (0.25 - EMPHASIZED_JOIN_X) / (1.0 - EMPHASIZED_JOIN_X),
    y2: 1.0,
};

/// Evaluate the two-segment emphasized easing for on-screen movement.
#[must_use]
pub fn emphasized_eval(t: f32) -> f32 {
    if t <= EMPHASIZED_JOIN_X {
        EMPHASIZED_JOIN_Y * bezier_eval(EMPHASIZED_FIRST, t / EMPHASIZED_JOIN_X)
    } else {
        let local_t = (t - EMPHASIZED_JOIN_X) / (1.0 - EMPHASIZED_JOIN_X);
        EMPHASIZED_JOIN_Y + (1.0 - EMPHASIZED_JOIN_Y) * bezier_eval(EMPHASIZED_SECOND, local_t)
    }
}

/// Evaluate a cubic-bezier easing at `t` (0..=1). Invert x(t) with
/// Newton–Raphson, or with bisection when an endpoint has zero x slope.
#[must_use]
pub fn bezier_eval(b: Bezier, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if b.x1 == 0.0 || b.x2 == 1.0 {
        // At a flat x endpoint Newton's starting derivative can vanish even
        // for an interior t. Bracketing covers both standard edge curves.
        if t == 0.0 || t == 1.0 || t.is_nan() {
            return t;
        }
        let (mut low, mut high) = (0.0, 1.0);
        for _ in 0..24 {
            let mid = (low + high) * 0.5;
            if cubic(b.x1, b.x2, mid) < t {
                low = mid;
            } else {
                high = mid;
            }
        }
        return cubic(b.y1, b.y2, (low + high) * 0.5);
    }
    // Solve x(u) = t for u, then return y(u).
    let mut u = t;
    for _ in 0..8 {
        let x = cubic(b.x1, b.x2, u) - t;
        let dx = cubic_d(b.x1, b.x2, u);
        if dx.abs() < 1e-6 {
            break;
        }
        u = (u - x / dx).clamp(0.0, 1.0);
    }
    cubic(b.y1, b.y2, u)
}

#[inline]
fn cubic(a: f32, b: f32, t: f32) -> f32 {
    let it = 1.0 - t;
    3.0 * it * it * t * a + 3.0 * it * t * t * b + t * t * t
}

#[inline]
fn cubic_d(a: f32, b: f32, t: f32) -> f32 {
    let it = 1.0 - t;
    3.0 * it * it * a + 6.0 * it * t * (b - a) + 3.0 * t * t * (1.0 - b)
}

/// Degrade ladder (§7.17a performance contract): when p95 frame time stays
/// above 20 ms for 3 consecutive transitions, step down one rung. Safe mode
/// starts at the last rung; reduced-motion pref forces bounce 0 and caps
/// durations at 160 ms with no parallax/scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DegradeLevel {
    /// Springs + stagger + effects.
    Full = 0,
    /// No stagger.
    NoStagger,
    /// No blur/scale effects.
    NoEffects,
    /// `smooth` becomes the 160 ms standard bezier.
    Bezier160,
    /// Everything becomes a 120 ms crossfade (safe mode rung).
    Crossfade120,
}

impl DegradeLevel {
    /// One rung down, saturating at `Crossfade120`.
    #[must_use]
    pub fn degraded(self) -> Self {
        match self {
            Self::Full => Self::NoStagger,
            Self::NoStagger => Self::NoEffects,
            Self::NoEffects => Self::Bezier160,
            _ => Self::Crossfade120,
        }
    }

    /// Frame-time threshold (ms) that trips the ladder after 3 consecutive
    /// slow transitions.
    pub const SLOW_FRAME_MS: f64 = 20.0;
    /// Consecutive slow transitions before stepping down.
    pub const SLOW_STREAK: u32 = 3;
}

/// Reduced-motion preference: 120 ms crossfade, bounce 0, no parallax/scale.
pub const REDUCED_MOTION_MS: u32 = 120;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn covered_area_selects_ladder_tiers_and_shorter_exits() {
        for (area, tier, enter, exit) in [
            (0.0, SpeedTier::Fast, 160, 72),
            (0.099, SpeedTier::Fast, 160, 72),
            (0.10, SpeedTier::Default, 360, 162),
            (0.749, SpeedTier::Default, 360, 162),
            (0.75, SpeedTier::Slow, 520, 234),
            (1.0, SpeedTier::Slow, 520, 234),
        ] {
            let timing = Transition::AppWindow.timing_for_covered_fraction(area);
            assert_eq!(timing.tier, tier);
            assert_eq!(timing.enter().duration_ms, enter);
            assert_eq!(timing.exit().duration_ms, exit);
            assert!(timing.enter().duration_ms <= DURATION_MAX_MS);
        }
    }

    #[test]
    fn every_non_screen_transition_has_a_reference_tier() {
        use SpeedTier::{Default, Fast, Slow};

        for (transition, tier) in [
            (Transition::HomePage, Slow),
            (Transition::RecentPage, Default),
            (Transition::RecentCard, Default),
            (Transition::ClearRecents, Slow),
            (Transition::Notification, Fast),
            (Transition::AppWindow, Slow),
            (Transition::AppToHome, Slow),
            (Transition::Osk, Default),
            (Transition::Toggle, Fast),
            (Transition::ChargingPill, Fast),
        ] {
            assert_eq!(transition.timing().tier, tier, "{transition:?}");
        }
    }

    #[test]
    fn exit_curves_match_edge_behavior() {
        for transition in [
            Transition::AppWindow,
            Transition::Notification,
            Transition::RecentCard,
            Transition::ClearRecents,
        ] {
            assert_eq!(transition.timing().exit().curve, BEZIER_EXIT);
        }
        for transition in [Transition::Shade, Transition::Drawer] {
            let curve = transition.timing().exit().curve;
            assert_eq!(bezier_eval(curve, 1.0), 1.0);
            // dy/dx at the edge is (1 - y2) / (1 - x2).
            assert_eq!(curve.y2, 1.0);
            assert!(curve.x2 < 1.0);
        }
    }

    #[test]
    fn standard_reproduces_every_preset() {
        assert_eq!(MotionScheme::default(), MotionScheme::Standard);
        for (preset, duration, bounce) in [
            (SpringPreset::Smooth, 0.450_f32, 0.0_f32),
            (SpringPreset::Snappy, 0.350, 0.12),
            (SpringPreset::Soft, 0.500, 0.25),
        ] {
            assert_eq!(preset.duration_s().to_bits(), duration.to_bits());
            assert_eq!(
                MotionScheme::Standard
                    .spring_bounce(preset, MotionKind::Spatial)
                    .to_bits(),
                bounce.to_bits()
            );
            assert_eq!(
                MotionScheme::Standard
                    .spring_bounce(preset, MotionKind::Effects)
                    .to_bits(),
                0
            );
        }
    }

    #[test]
    fn expressive_only_raises_small_spatial_bounce() {
        let expressive = MotionScheme::Expressive;
        let snappy = expressive.spring_bounce(SpringPreset::Snappy, MotionKind::Spatial);
        assert!(snappy > SpringPreset::Snappy.bounce() && snappy <= SpringPreset::BOUNCE_MAX);
        assert_eq!(
            expressive.spring_bounce(SpringPreset::Smooth, MotionKind::Spatial),
            SpringPreset::APP_MOTION_BOUNCE_MAX
        );
        assert_eq!(
            expressive.spring_bounce(SpringPreset::Soft, MotionKind::Spatial),
            SpringPreset::Soft.bounce()
        );
        for preset in [
            SpringPreset::Smooth,
            SpringPreset::Snappy,
            SpringPreset::Soft,
        ] {
            assert_eq!(expressive.spring_bounce(preset, MotionKind::Effects), 0.0);
        }
    }

    #[test]
    fn beziers_hit_endpoints() {
        for b in [BEZIER_STANDARD, BEZIER_ENTER, BEZIER_EXIT] {
            assert!((bezier_eval(b, 0.0)).abs() < 1e-4);
            assert!((bezier_eval(b, 1.0) - 1.0).abs() < 1e-4);
        }
        // standard curve is monotonic and mid-ish at t=0.5.
        let m = bezier_eval(BEZIER_STANDARD, 0.5);
        assert!(m > 0.4 && m < 0.9, "mid {m}");
    }

    #[test]
    fn added_easings_match_the_reference_paths() {
        // Reference coordinates in output space, before segment normalization.
        let decelerate = [[[0.0, 0.0], [0.0, 0.0], [0.0, 1.0], [1.0, 1.0]]];
        let accelerate = [[[0.0, 0.0], [0.3, 0.0], [1.0, 1.0], [1.0, 1.0]]];
        let emphasized = [
            [[0.0, 0.0], [0.05, 0.0], [0.133333, 0.06], [0.166666, 0.4]],
            [[0.166666, 0.4], [0.208333, 0.82], [0.25, 1.0], [1.0, 1.0]],
        ];

        for (name, evaluate, path) in [
            (
                "standard decelerate",
                (|t| bezier_eval(BEZIER_STANDARD_DECELERATE, t)) as fn(f32) -> f32,
                decelerate.as_slice(),
            ),
            (
                "standard accelerate",
                |t| bezier_eval(BEZIER_STANDARD_ACCELERATE, t),
                accelerate.as_slice(),
            ),
            ("emphasized", emphasized_eval, emphasized.as_slice()),
        ] {
            assert_eq!(evaluate(0.0), 0.0, "{name}");
            assert_eq!(evaluate(1.0), 1.0, "{name}");
            let samples = [0.0, 1e-8, 1e-6]
                .into_iter()
                .chain((1..4096).map(|step| step as f32 / 4096.0))
                .chain([0.999_999, 1.0]);
            let mut previous = 0.0;
            for t in samples {
                let actual = evaluate(t);
                let expected = reference_progress(path, f64::from(t));
                assert!(actual >= previous, "{name} decreases at {t}");
                assert!(
                    (f64::from(actual) - expected).abs() <= 1e-5,
                    "{name} at {t}: {actual} vs {expected}"
                );
                previous = actual;
            }
        }

        assert_eq!(emphasized_eval(0.166666), 0.4);
    }

    // Evaluate the unscaled reference with de Casteljau subdivision in f64.
    fn reference_progress(path: &[[[f64; 2]; 4]], t: f64) -> f64 {
        let segment = path.iter().find(|segment| t <= segment[3][0]).unwrap();
        let coordinate = |axis: usize, u: f64| {
            let mut points = segment.map(|point| point[axis]);
            for remaining in (1..4).rev() {
                for i in 0..remaining {
                    points[i] += (points[i + 1] - points[i]) * u;
                }
            }
            points[0]
        };
        let (mut low, mut high) = (0.0, 1.0);
        for _ in 0..50 {
            let mid = (low + high) * 0.5;
            if coordinate(0, mid) < t {
                low = mid;
            } else {
                high = mid;
            }
        }
        coordinate(1, (low + high) * 0.5)
    }

    #[test]
    fn degrade_ladder_order() {
        let mut l = DegradeLevel::Full;
        for _ in 0..10 {
            l = l.degraded();
        }
        assert_eq!(l, DegradeLevel::Crossfade120);
    }
}
