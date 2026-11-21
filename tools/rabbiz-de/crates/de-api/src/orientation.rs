//! Screen orientation model: the rotation API shared by sensors, the
//! compositor, the shell and apps.
//!
//! Vocabulary (Android-compatible):
//! - *native* — the panel's scanout orientation (A21s: 720x1600 portrait).
//! - *logical* — what the shell and clients see: native rotated by
//!   [`Orientation`]. All `de-core`/`de-ui` geometry is logical.
//!
//! Pure data and math only (no sensors, no smithay) so every lane and every
//! host can test it. Sensors feed [`OrientationDetector`]; the result is
//! combined with the user's [`RotationPolicy`] and the foreground app's
//! [`OrientationRequest`] by [`resolve`].

use serde::{Deserialize, Serialize};

use crate::geom::Size;

/// Which way up the *device* is held, named for how the user turned it.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Orientation {
    /// Native portrait, top edge up.
    #[default]
    Portrait,
    /// Device turned 90 degrees counter-clockwise (top edge points left).
    LandscapeCcw,
    /// Portrait, upside down.
    PortraitFlipped,
    /// Device turned 90 degrees clockwise (top edge points right).
    LandscapeCw,
}

impl Orientation {
    pub const ALL: [Self; 4] = [
        Self::Portrait,
        Self::LandscapeCcw,
        Self::PortraitFlipped,
        Self::LandscapeCw,
    ];

    #[must_use]
    pub const fn is_landscape(self) -> bool {
        matches!(self, Self::LandscapeCcw | Self::LandscapeCw)
    }

    #[must_use]
    pub const fn is_portrait(self) -> bool {
        !self.is_landscape()
    }

    /// Logical size for a panel of `native` size.
    #[must_use]
    pub const fn logical_size(self, native: Size) -> Size {
        if self.is_landscape() {
            Size::new(native.h, native.w)
        } else {
            native
        }
    }

    /// Map a point in the panel's native space to logical space (touch input).
    #[must_use]
    pub fn native_to_logical(self, x: f64, y: f64, native: Size) -> (f64, f64) {
        let (w, h) = (f64::from(native.w), f64::from(native.h));
        match self {
            Self::Portrait => (x, y),
            Self::LandscapeCcw => (y, w - x),
            Self::PortraitFlipped => (w - x, h - y),
            Self::LandscapeCw => (h - y, x),
        }
    }

    /// Inverse of [`Self::native_to_logical`].
    #[must_use]
    pub fn logical_to_native(self, x: f64, y: f64, native: Size) -> (f64, f64) {
        let (w, h) = (f64::from(native.w), f64::from(native.h));
        match self {
            Self::Portrait => (x, y),
            Self::LandscapeCcw => (w - y, x),
            Self::PortraitFlipped => (w - x, h - y),
            Self::LandscapeCw => (y, h - x),
        }
    }

    /// Stable lowercase name (IPC, logs, settings).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Portrait => "portrait",
            Self::LandscapeCcw => "landscape_ccw",
            Self::PortraitFlipped => "portrait_flipped",
            Self::LandscapeCw => "landscape_cw",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|o| o.name() == s.trim())
    }
}

/// What a foreground app asks for (`X-Rabbiz-Orientation=` in its
/// `.desktop` file, or at runtime over the control socket).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrientationRequest {
    /// Follow the system (user policy + sensor).
    #[default]
    Unspecified,
    /// Always native portrait.
    Portrait,
    /// Always landscape (the sensor picks the side, falling back to CCW).
    Landscape,
    /// Portrait either way up, by sensor; ignores the user's rotation lock.
    SensorPortrait,
    /// Landscape either side, by sensor; ignores the user's rotation lock.
    SensorLandscape,
    /// Any of the four, by sensor; ignores the user's rotation lock.
    Sensor,
    /// Keep whatever is showing right now.
    Locked,
}

impl OrientationRequest {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Unspecified => "unspecified",
            Self::Portrait => "portrait",
            Self::Landscape => "landscape",
            Self::SensorPortrait => "sensor_portrait",
            Self::SensorLandscape => "sensor_landscape",
            Self::Sensor => "sensor",
            Self::Locked => "locked",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        [
            Self::Unspecified,
            Self::Portrait,
            Self::Landscape,
            Self::SensorPortrait,
            Self::SensorLandscape,
            Self::Sensor,
            Self::Locked,
        ]
        .into_iter()
        .find(|r| r.name() == s.trim().to_ascii_lowercase().replace('-', "_"))
    }
}

/// The user's rotation setting (Quick Settings "Auto-rotate" tile).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RotationPolicy {
    /// Follow the sensor for unconstrained apps and the shell.
    pub auto: bool,
    /// Orientation used while `auto` is off.
    pub locked: Orientation,
}

impl Default for RotationPolicy {
    fn default() -> Self {
        Self {
            auto: true,
            locked: Orientation::Portrait,
        }
    }
}

/// Combine sensor, user policy and app request into the orientation to show.
///
/// `current` is what is on screen now (used by `Locked` and as the fallback
/// when the sensor has no opinion, e.g. device lying flat).
#[must_use]
pub fn resolve(
    policy: RotationPolicy,
    request: OrientationRequest,
    sensed: Option<Orientation>,
    current: Orientation,
) -> Orientation {
    let sensor = sensed.unwrap_or(current);
    let pick = |allowed: &[Orientation], fallback: Orientation| {
        if allowed.contains(&sensor) {
            sensor
        } else if allowed.contains(&current) {
            current
        } else {
            fallback
        }
    };
    use Orientation::{LandscapeCcw, LandscapeCw, Portrait, PortraitFlipped};
    match request {
        OrientationRequest::Unspecified => {
            if policy.auto {
                sensor
            } else {
                policy.locked
            }
        }
        OrientationRequest::Portrait => Portrait,
        OrientationRequest::Landscape => pick(&[LandscapeCcw, LandscapeCw], LandscapeCcw),
        OrientationRequest::SensorPortrait => pick(&[Portrait, PortraitFlipped], Portrait),
        OrientationRequest::SensorLandscape => pick(&[LandscapeCcw, LandscapeCw], LandscapeCcw),
        OrientationRequest::Sensor => sensor,
        OrientationRequest::Locked => current,
    }
}

/// One accelerometer reading in the device frame, m/s^2.
///
/// Convention (Android / iio-sensor-proxy): x toward the right edge, y toward
/// the top edge, z out of the screen; at rest the vector points *up*, so a
/// phone standing upright reads `y = +9.8`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AccelSample {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl AccelSample {
    pub const G: f32 = 9.806_65;

    #[must_use]
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    /// Reading of a device held still at `o` (used by the emulator and tests).
    #[must_use]
    pub fn at_rest(o: Orientation) -> Self {
        let g = Self::G;
        match o {
            Orientation::Portrait => Self::new(0.0, g, 0.0),
            Orientation::LandscapeCcw => Self::new(g, 0.0, 0.0),
            Orientation::PortraitFlipped => Self::new(0.0, -g, 0.0),
            Orientation::LandscapeCw => Self::new(-g, 0.0, 0.0),
        }
    }
}

/// Sensor debouncing: turns a stream of [`AccelSample`]s into stable
/// orientation changes.
///
/// A candidate must sit within `ENTER_DEG` of its axis and stay there for
/// `DWELL_MS` before it replaces the current one; readings with the device
/// near flat (|z| > `FLAT_Z`) or shaken (|a| far from g) are ignored.
#[derive(Debug, Clone, Copy)]
pub struct OrientationDetector {
    current: Option<Orientation>,
    candidate: Option<(Orientation, u64)>,
}

impl Default for OrientationDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl OrientationDetector {
    /// Half-width of the band around each axis that counts as "that way up".
    pub const ENTER_DEG: f32 = 35.0;
    /// How long the candidate must persist.
    pub const DWELL_MS: u64 = 400;
    /// |z| / g above which the device is considered lying flat.
    pub const FLAT_Z: f32 = 0.8;

    #[must_use]
    pub const fn new() -> Self {
        Self {
            current: None,
            candidate: None,
        }
    }

    /// Last stable orientation, `None` until the first confident reading.
    #[must_use]
    pub const fn current(&self) -> Option<Orientation> {
        self.current
    }

    /// Instantaneous classification of one sample, `None` when ambiguous.
    #[must_use]
    pub fn classify(s: AccelSample) -> Option<Orientation> {
        let mag = (s.x * s.x + s.y * s.y + s.z * s.z).sqrt();
        if mag < AccelSample::G * 0.7 || mag > AccelSample::G * 1.3 {
            return None; // free-fall or shaking
        }
        if (s.z / mag).abs() > Self::FLAT_Z {
            return None;
        }
        // 0 = upright, +90 = turned CCW, +-180 = flipped, -90 = turned CW.
        let angle = s.x.atan2(s.y).to_degrees();
        let near = |center: f32| {
            let mut d = (angle - center).abs();
            if d > 180.0 {
                d = 360.0 - d;
            }
            d <= Self::ENTER_DEG
        };
        if near(0.0) {
            Some(Orientation::Portrait)
        } else if near(90.0) {
            Some(Orientation::LandscapeCcw)
        } else if near(-90.0) {
            Some(Orientation::LandscapeCw)
        } else if near(180.0) {
            Some(Orientation::PortraitFlipped)
        } else {
            None
        }
    }

    /// Feed one sample. Returns the new orientation when it changed.
    pub fn update(&mut self, s: AccelSample, now_ms: u64) -> Option<Orientation> {
        let Some(c) = Self::classify(s) else {
            self.candidate = None;
            return None;
        };
        if Some(c) == self.current {
            self.candidate = None;
            return None;
        }
        match self.candidate {
            Some((o, since)) if o == c => {
                // First reading ever needs no dwell: boot straight into the held pose.
                if self.current.is_none() || now_ms.saturating_sub(since) >= Self::DWELL_MS {
                    self.current = Some(c);
                    self.candidate = None;
                    return Some(c);
                }
            }
            _ => self.candidate = Some((c, now_ms)),
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NATIVE: Size = Size::new(720, 1600);

    #[test]
    fn logical_size_swaps_in_landscape() {
        assert_eq!(Orientation::Portrait.logical_size(NATIVE), NATIVE);
        assert_eq!(Orientation::PortraitFlipped.logical_size(NATIVE), NATIVE);
        assert_eq!(Orientation::LandscapeCcw.logical_size(NATIVE), Size::new(1600, 720));
        assert_eq!(Orientation::LandscapeCw.logical_size(NATIVE), Size::new(1600, 720));
    }

    #[test]
    fn point_mapping_round_trips_and_corners_match_physics() {
        for o in Orientation::ALL {
            let l = o.logical_size(NATIVE);
            for (x, y) in [(0.0, 0.0), (100.0, 250.0), (f64::from(l.w), f64::from(l.h))] {
                let (nx, ny) = o.logical_to_native(x, y, NATIVE);
                let (bx, by) = o.native_to_logical(nx, ny, NATIVE);
                assert!((bx - x).abs() < 1e-9 && (by - y).abs() < 1e-9, "{o:?}");
            }
        }
        // Turned CCW: the panel's right edge is up, so logical top (y=0) is
        // the native right edge (x=w).
        let (nx, _) = Orientation::LandscapeCcw.logical_to_native(0.0, 0.0, NATIVE);
        assert!((nx - 720.0).abs() < 1e-9);
        // Turned CW: logical top -> native left edge (x=0).
        let (nx, _) = Orientation::LandscapeCw.logical_to_native(0.0, 0.0, NATIVE);
        assert!(nx.abs() < 1e-9);
    }

    #[test]
    fn classify_each_rest_pose() {
        for o in Orientation::ALL {
            assert_eq!(OrientationDetector::classify(AccelSample::at_rest(o)), Some(o));
        }
    }

    #[test]
    fn flat_and_shaken_are_ignored() {
        assert_eq!(OrientationDetector::classify(AccelSample::new(0.0, 0.0, 9.8)), None);
        assert_eq!(OrientationDetector::classify(AccelSample::new(30.0, 0.0, 0.0)), None);
        assert_eq!(OrientationDetector::classify(AccelSample::new(0.0, 0.5, 0.2)), None);
    }

    #[test]
    fn diagonal_hold_keeps_current() {
        // 45 degrees between portrait and landscape: outside both bands.
        let g = AccelSample::G;
        let s = AccelSample::new(g * 0.7071, g * 0.7071, 0.0);
        assert_eq!(OrientationDetector::classify(s), None);
    }

    #[test]
    fn detector_needs_dwell_after_first_reading() {
        let mut d = OrientationDetector::new();
        assert_eq!(d.update(AccelSample::at_rest(Orientation::Portrait), 0), None);
        assert_eq!(
            d.update(AccelSample::at_rest(Orientation::Portrait), 10),
            Some(Orientation::Portrait)
        );
        let land = AccelSample::at_rest(Orientation::LandscapeCcw);
        assert_eq!(d.update(land, 1000), None);
        assert_eq!(d.update(land, 1200), None);
        assert_eq!(d.update(land, 1400), Some(Orientation::LandscapeCcw));
        assert_eq!(d.update(land, 1500), None);
    }

    #[test]
    fn jitter_resets_dwell() {
        let mut d = OrientationDetector::new();
        d.update(AccelSample::at_rest(Orientation::Portrait), 0);
        d.update(AccelSample::at_rest(Orientation::Portrait), 1);
        let land = AccelSample::at_rest(Orientation::LandscapeCcw);
        d.update(land, 1000);
        d.update(AccelSample::at_rest(Orientation::Portrait), 1200);
        d.update(land, 1300);
        assert_eq!(d.update(land, 1500), None);
        assert_eq!(d.update(land, 1700), Some(Orientation::LandscapeCcw));
    }

    #[test]
    fn resolve_follows_policy_and_request() {
        use Orientation::{LandscapeCcw, LandscapeCw, Portrait};
        let auto = RotationPolicy::default();
        let lock = RotationPolicy {
            auto: false,
            locked: Portrait,
        };
        let r = |p, q, s, c| resolve(p, q, s, c);
        assert_eq!(r(auto, OrientationRequest::Unspecified, Some(LandscapeCw), Portrait), LandscapeCw);
        assert_eq!(r(lock, OrientationRequest::Unspecified, Some(LandscapeCw), Portrait), Portrait);
        // Sensor-style requests ignore the user lock.
        assert_eq!(r(lock, OrientationRequest::Sensor, Some(LandscapeCw), Portrait), LandscapeCw);
        assert_eq!(r(lock, OrientationRequest::SensorLandscape, Some(LandscapeCw), Portrait), LandscapeCw);
        // Landscape request with the device held portrait picks a landscape side.
        assert_eq!(r(auto, OrientationRequest::Landscape, Some(Portrait), Portrait), LandscapeCcw);
        assert_eq!(r(auto, OrientationRequest::Landscape, Some(Portrait), LandscapeCw), LandscapeCw);
        assert_eq!(r(auto, OrientationRequest::Portrait, Some(LandscapeCcw), LandscapeCcw), Portrait);
        assert_eq!(r(auto, OrientationRequest::Locked, Some(Portrait), LandscapeCcw), LandscapeCcw);
        // No sensor opinion: stay put.
        assert_eq!(r(auto, OrientationRequest::Unspecified, None, LandscapeCcw), LandscapeCcw);
    }

    #[test]
    fn names_round_trip() {
        for o in Orientation::ALL {
            assert_eq!(Orientation::parse(o.name()), Some(o));
        }
        assert_eq!(OrientationRequest::parse("sensor-landscape"), Some(OrientationRequest::SensorLandscape));
        assert_eq!(OrientationRequest::parse("nope"), None);
    }
}
