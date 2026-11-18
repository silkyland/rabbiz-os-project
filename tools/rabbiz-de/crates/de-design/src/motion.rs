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

/// Evaluate a cubic-bezier easing at `t` (0..=1) via Newton–Raphson on x(t).
#[must_use]
pub fn bezier_eval(b: Bezier, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
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
    fn degrade_ladder_order() {
        let mut l = DegradeLevel::Full;
        for _ in 0..10 {
            l = l.degraded();
        }
        assert_eq!(l, DegradeLevel::Crossfade120);
    }
}
