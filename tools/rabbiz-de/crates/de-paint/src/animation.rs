//! Animation clock + spring solver (M2-T4). Values animate as `f32` channels
//! driven by a closed-form damped-harmonic-oscillator step derived from each
//! `SpringPreset`'s duration/bounce; retargeting preserves velocity, which is
//! what makes gestures interruptible.

use de_design::motion::{self, DegradeLevel, FormFactor, MotionKind, MotionScheme, SpringPreset};

/// Clock feeding all animations; `now_s` is seconds since compositor start.
#[derive(Debug, Default)]
pub struct AnimationClock {
    pub now_s: f64,
    scheme: MotionScheme,
    no_bounce: bool,
    form_factor: FormFactor,
}

impl AnimationClock {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Configure the session's spring policy. The caller still selects the
    /// fallback animation when its motion or degradation policy requires it.
    #[must_use]
    pub fn with_motion(scheme: MotionScheme, reduced_motion: bool, safe_mode: bool) -> Self {
        let no_bounce = reduced_motion || safe_mode;
        Self {
            scheme: if no_bounce {
                MotionScheme::Standard
            } else {
                scheme
            },
            no_bounce,
            ..Self::default()
        }
    }

    /// The effective scheme for this clock.
    #[must_use]
    pub fn scheme(&self) -> MotionScheme {
        self.scheme
    }

    /// Whether reduced motion or safe mode takes priority over normal timing.
    #[must_use]
    pub fn motion_restricted(&self) -> bool {
        self.no_bounce
    }

    #[must_use]
    pub fn for_form_factor(self, form_factor: FormFactor) -> Self {
        Self {
            form_factor,
            ..self
        }
    }

    #[must_use]
    pub fn form_factor(&self) -> FormFactor {
        if self.motion_restricted() {
            FormFactor::Handset
        } else {
            self.form_factor
        }
    }

    fn spring_bounce(&self, preset: SpringPreset, kind: MotionKind) -> f32 {
        if self.no_bounce {
            0.0
        } else {
            self.scheme.spring_bounce(preset, kind)
        }
    }

    pub fn tick(&mut self, dt_s: f64) {
        self.now_s += dt_s;
    }
}

/// Derived oscillator constants for a preset: damping ratio `zeta` and
/// natural frequency `omega` chosen so the spring settles within ~1 % near
/// `duration_s` and overshoots by `bounce` on the first crossing.
#[derive(Debug, Clone, Copy)]
struct Osc {
    zeta: f32,
    omega: f32,
}

impl Osc {
    fn of(preset: SpringPreset, kind: MotionKind, clock: &AnimationClock) -> Self {
        let bounce = clock.spring_bounce(preset, kind);
        let bounce = bounce.max(0.0);
        // Overshoot = e^(-ζπ/√(1-ζ²)) → ζ = -ln(b)/√(π²+ln²b); bounce 0 → ζ=1.
        let zeta = if bounce <= 0.0 {
            1.0
        } else {
            let l = bounce.ln();
            (-l / (std::f32::consts::PI * std::f32::consts::PI + l * l).sqrt()).min(0.999)
        };
        let decay = if clock.motion_restricted() {
            4.6 // Preserve the previous restricted-motion spring evaluation.
        } else if zeta >= 1.0 {
            6.638_352 // (1 + decay) * exp(-decay) = 0.01 for a start at rest.
        } else {
            // Bound the rest-start response by exp(-decay)/sqrt(1-ζ²).
            100.0_f32.ln() - 0.5 * (1.0 - zeta * zeta).ln()
        };
        let duration = clock.form_factor().scale_s(preset.duration_s());
        let omega = decay / (zeta * duration.max(0.05));
        Self { zeta, omega }
    }

    /// Advance (x, v) toward `target` by `dt` using the closed-form solution.
    fn step(self, x: f32, v: f32, target: f32, dt: f32) -> (f32, f32) {
        let a = x - target;
        if self.zeta >= 1.0 {
            // Critically damped: x(t) = (A + (v0+ωA)t)·e^(-ωt).
            let w = self.omega;
            let b = v + w * a;
            let e = (-w * dt).exp();
            let nx = (a + b * dt) * e;
            let nv = (b - w * (a + b * dt)) * e;
            (nx + target, nv)
        } else {
            // Underdamped: x(t) = e^(-ζωt)(A cos ωd t + B sin ωd t).
            let (z, w) = (self.zeta, self.omega);
            let wd = w * (1.0 - z * z).sqrt();
            let b = (v + z * w * a) / wd;
            let e = (-z * w * dt).exp();
            let (c, s) = ((wd * dt).cos(), (wd * dt).sin());
            let nx = e * (a * c + b * s);
            // dx/dt = e^(-ζωt)[(B·ωd - ζωA)cos + (-A·ωd - ζωB)sin]
            let nv = e * ((b * wd - z * w * a) * c + (-a * wd - z * w * b) * s);
            (nx + target, nv)
        }
    }
}

/// One animated scalar channel.
#[derive(Debug, Clone)]
pub struct SpringValue {
    pub value: f32,
    pub target: f32,
    pub velocity: f32,
    pub preset: SpringPreset,
    kind: MotionKind,
    /// Settled when within this epsilon of target and velocity is negligible.
    pub epsilon: f32,
}

impl SpringValue {
    #[must_use]
    pub fn new(initial: f32, preset: SpringPreset, kind: MotionKind) -> Self {
        Self {
            value: initial,
            target: initial,
            velocity: 0.0,
            preset,
            kind,
            epsilon: 0.001,
        }
    }

    /// Retarget keeping current position + velocity (interruptible).
    pub fn retarget(&mut self, target: f32) {
        self.target = target;
    }

    /// Retarget with an incoming gesture velocity.
    pub fn retarget_with_velocity(&mut self, target: f32, velocity: f32) {
        self.target = target;
        self.velocity = velocity;
    }

    /// Project where a gesture release will land: pos + v·0.25 s (§7.17).
    #[must_use]
    pub fn projected_release(&self) -> f32 {
        self.value + self.velocity * motion::PROJECT_AHEAD_S
    }

    /// Rubber-band overscroll displacement: 0.55 × drag past the limit.
    #[must_use]
    pub fn rubber(drag_past_limit: f32) -> f32 {
        drag_past_limit * motion::RUBBER_FACTOR
    }

    /// Advance the spring by `dt_s` (closed-form, no allocation).
    /// Returns `true` if still animating.
    pub fn advance(&mut self, dt_s: f32, clock: &AnimationClock) -> bool {
        if self.settled() {
            self.snap();
            return false;
        }
        let (value, velocity) = Osc::of(self.preset, self.kind, clock).step(
            self.value,
            self.velocity,
            self.target,
            dt_s,
        );
        // Critical damping can still overshoot with inherited velocity.
        let (value, velocity) = if clock.no_bounce || self.kind == MotionKind::Effects {
            let range = self.value.min(self.target)..=self.value.max(self.target);
            let bounded = value.clamp(*range.start(), *range.end());
            (
                bounded,
                if range.contains(&value) {
                    velocity
                } else {
                    0.0
                },
            )
        } else {
            (value, velocity)
        };
        self.value = value;
        self.velocity = velocity;
        if self.settled() {
            self.snap();
            return false;
        }
        true
    }

    /// Snap immediately (degrade ≥ crossfade / reduced motion).
    pub fn snap(&mut self) {
        self.value = self.target;
        self.velocity = 0.0;
    }

    #[must_use]
    pub fn settled(&self) -> bool {
        (self.value - self.target).abs() < self.epsilon && self.velocity.abs() < self.epsilon
    }
}

/// Frame pacing + degradation ladder driver. `level` rises when frames blow
/// the budget; recovers after sustained good frames.
#[derive(Debug)]
pub struct FrameGovernor {
    pub level: DegradeLevel,
    slow_streak: u32,
    good_streak: u32,
}

impl Default for FrameGovernor {
    fn default() -> Self {
        Self::new()
    }
}

/// One rung up the ladder (toward `Full`).
fn recovered(l: DegradeLevel) -> DegradeLevel {
    match l {
        DegradeLevel::Full | DegradeLevel::NoStagger => DegradeLevel::Full,
        DegradeLevel::NoEffects => DegradeLevel::NoStagger,
        DegradeLevel::Bezier160 => DegradeLevel::NoEffects,
        DegradeLevel::Crossfade120 => DegradeLevel::Bezier160,
    }
}

impl FrameGovernor {
    #[must_use]
    pub fn new() -> Self {
        Self {
            level: DegradeLevel::Full,
            slow_streak: 0,
            good_streak: 0,
        }
    }

    /// Feed one frame's composite time (ms). Returns current level.
    pub fn observe(&mut self, frame_ms: f64) -> DegradeLevel {
        if frame_ms > DegradeLevel::SLOW_FRAME_MS {
            self.slow_streak += 1;
            self.good_streak = 0;
            if self.slow_streak >= DegradeLevel::SLOW_STREAK {
                self.level = self.level.degraded();
                self.slow_streak = 0;
            }
        } else {
            self.good_streak += 1;
            self.slow_streak = 0;
            // Recover one rung after ~2 s of healthy frames.
            if self.good_streak >= 120 {
                self.level = recovered(self.level);
                self.good_streak = 0;
            }
        }
        self.level
    }

    /// Whether springs may run at this level (≥ Bezier160 uses fixed
    /// durations instead).
    #[must_use]
    pub fn springs_allowed(&self) -> bool {
        self.level < DegradeLevel::Bezier160
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRESETS: [(SpringPreset, f32); 3] = [
        (SpringPreset::Smooth, 0.0),
        (SpringPreset::Snappy, 0.12),
        (SpringPreset::Soft, 0.25),
    ];

    #[test]
    fn restricted_motion_overrides_expressive() {
        for (reduced_motion, safe_mode) in [(true, false), (false, true), (true, true)] {
            let clock =
                AnimationClock::with_motion(MotionScheme::Expressive, reduced_motion, safe_mode);
            assert_eq!(clock.scheme(), MotionScheme::Standard);
            for (preset, _) in PRESETS {
                assert_eq!(
                    clock.spring_bounce(preset, MotionKind::Spatial).to_bits(),
                    0
                );
                let mut spring = SpringValue::new(0.0, preset, MotionKind::Spatial);
                spring.retarget_with_velocity(1.0, 100.0);
                for _ in 0..2000 {
                    spring.advance(0.001, &clock);
                    assert!(
                        (0.0..=1.0).contains(&spring.value),
                        "{preset:?}: {spring:?}"
                    );
                }
                assert_eq!(spring.value, 1.0);
            }
        }
    }

    #[test]
    fn effects_stay_between_endpoints() {
        let clock = AnimationClock::new();
        for (preset, _) in PRESETS {
            for (from, to) in [(0.0_f32, 1.0_f32), (1.0, 0.0), (0.25, 0.75)] {
                for velocity in [-100.0, 0.0, 100.0] {
                    let bounds = from.min(to)..=from.max(to);
                    let mut spring = SpringValue::new(from, preset, MotionKind::Effects);
                    spring.retarget_with_velocity(to, velocity);
                    assert!(bounds.contains(&spring.value));
                    for _ in 0..2000 {
                        spring.advance(0.001, &clock);
                        assert!(bounds.contains(&spring.value), "{preset:?}: {spring:?}");
                    }
                    assert_eq!(spring.value, to);
                    assert!(spring.settled());
                }
            }
        }
    }

    #[test]
    fn spatial_bounce_respects_each_preset_limit() {
        let clock = AnimationClock::new();
        for (preset, bounce) in PRESETS {
            let mut spring = SpringValue::new(0.0, preset, MotionKind::Spatial);
            spring.retarget(1.0);
            let mut peak = spring.value;
            for _ in 0..2000 {
                spring.advance(0.001, &clock);
                peak = peak.max(spring.value);
                assert!(spring.value >= 0.0, "{preset:?}: {spring:?}");
            }
            assert!(peak <= 1.0 + bounce + 1e-5, "{preset:?}: peak={peak}");
            assert!(
                (peak - (1.0 + bounce)).abs() < 1e-4,
                "{preset:?}: peak={peak}"
            );
        }
    }

    #[test]
    fn springs_stay_in_the_one_percent_band_by_the_scaled_duration() {
        for form_factor in [FormFactor::Handset, FormFactor::Tablet, FormFactor::Desktop] {
            for scheme in [MotionScheme::Standard, MotionScheme::Expressive] {
                let clock =
                    AnimationClock::with_motion(scheme, false, false).for_form_factor(form_factor);
                for (preset, _) in PRESETS {
                    for kind in [MotionKind::Spatial, MotionKind::Effects] {
                        let duration = form_factor.scale_s(preset.duration_s());
                        let bounce = clock.spring_bounce(preset, kind);
                        let mut spring = SpringValue::new(0.0, preset, kind);
                        spring.retarget(1.0);
                        spring.advance(duration, &clock);
                        for _ in 0..600 {
                            assert!(
                                (spring.value - 1.0).abs() <= 0.010_01,
                                "{form_factor:?} {scheme:?} {spring:?}"
                            );
                            spring.advance(0.001, &clock);
                        }
                        let mut spring = SpringValue::new(0.0, preset, kind);
                        spring.retarget(1.0);
                        for _ in 0..600 {
                            spring.advance(0.001, &clock);
                            assert!((0.0..=1.0 + bounce + 1e-5).contains(&spring.value));
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn spring_converges() {
        let clock = AnimationClock::new();
        let mut s = SpringValue::new(0.0, SpringPreset::Smooth, MotionKind::Spatial);
        s.retarget(100.0);
        for _ in 0..240 {
            s.advance(1.0 / 60.0, &clock);
        }
        assert!((s.value - 100.0).abs() < 0.5, "v={}", s.value);
    }

    #[test]
    fn snappy_overshoots() {
        let clock = AnimationClock::new();
        let mut s = SpringValue::new(0.0, SpringPreset::Snappy, MotionKind::Spatial);
        s.retarget(100.0);
        let mut max_v = 0.0f32;
        for _ in 0..240 {
            s.advance(1.0 / 60.0, &clock);
            max_v = max_v.max(s.value);
        }
        // 12 % bounce → peak ≥ ~104 past the target.
        assert!(max_v > 103.0, "peak={max_v}");
        assert!((s.value - 100.0).abs() < 0.5);
    }

    #[test]
    fn retarget_preserves_velocity() {
        let mut s = SpringValue::new(0.0, SpringPreset::Snappy, MotionKind::Spatial);
        s.retarget_with_velocity(50.0, 400.0);
        let v0 = s.velocity;
        s.retarget(10.0);
        assert_eq!(s.velocity, v0, "retarget must preserve velocity");
    }

    #[test]
    fn projection_uses_quarter_second() {
        let mut s = SpringValue::new(100.0, SpringPreset::Smooth, MotionKind::Spatial);
        s.velocity = -800.0;
        assert_eq!(s.projected_release(), -100.0);
    }

    #[test]
    fn rubber_is_055x() {
        assert_eq!(SpringValue::rubber(100.0), 55.0);
    }

    #[test]
    fn governor_degrades_on_slow_frames() {
        let mut g = FrameGovernor::new();
        for _ in 0..3 {
            g.observe(40.0);
        }
        assert_eq!(g.level, DegradeLevel::NoStagger);
        for _ in 0..3 {
            g.observe(40.0);
        }
        assert_eq!(g.level, DegradeLevel::NoEffects);
    }

    #[test]
    fn governor_recovers() {
        let mut g = FrameGovernor::new();
        g.level = DegradeLevel::NoEffects;
        for _ in 0..120 {
            g.observe(10.0);
        }
        assert_eq!(g.level, DegradeLevel::NoStagger);
    }
}
