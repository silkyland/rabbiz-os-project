//! Screen rotation: sensor -> orientation policy -> output transform.
//!
//! `De::output.width/height` are *logical* (post-rotation); the whole shell,
//! every client configure and all input live in logical space. Only two
//! places know the panel is physically portrait: the scanout (DRM output
//! transform) and raw libinput touch mapping ([`Rotation::map_hw_point`]).
//! On `--backend=virtual` (the emulator) there is no panel: the emulated
//! screen itself changes shape, so the frame the viewer receives is already
//! landscape and injected touches are already logical.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use de_api::{
    resolve_orientation, Orientation, OrientationDetector, OrientationRequest, RotationPolicy,
    Size as ApiSize,
};
use de_system::accel::AccelSource;
use smithay::desktop::layer_map_for_output;
use smithay::output::Mode;
use smithay::utils::{Logical, Point, Transform};

use crate::state::{De, Foreground};

const POLL_EVERY: Duration = Duration::from_millis(200);

/// Rotation state owned by [`De`].
pub(crate) struct Rotation {
    /// Panel size in its native (scanout) orientation.
    pub native: ApiSize,
    /// Orientation currently applied to the output.
    pub current: Orientation,
    pub policy: RotationPolicy,
    /// Last stable sensor reading (`None` until the first confident one).
    pub sensed: Option<Orientation>,
    /// Runtime per-app requests (`ctl app-orientation`), over the catalog's.
    pub app_requests: HashMap<String, OrientationRequest>,
    detector: OrientationDetector,
    source: AccelSource,
    next_poll: Instant,
}

impl Rotation {
    pub(crate) fn new(native: ApiSize, policy: RotationPolicy) -> Self {
        Self {
            native,
            current: Orientation::Portrait,
            policy,
            sensed: None,
            app_requests: HashMap::new(),
            detector: OrientationDetector::new(),
            source: AccelSource::new(),
            next_poll: Instant::now(),
        }
    }

    /// Map a raw panel-space point (touch/absolute pointer) to logical space.
    pub(crate) fn map_hw_point(&self, p: Point<f64, Logical>) -> Point<f64, Logical> {
        let (x, y) = self.current.native_to_logical(p.x, p.y, self.native);
        (x, y).into()
    }

    /// Panel size the hardware layer should normalise raw coordinates to.
    pub(crate) fn native_size(&self) -> smithay::utils::Size<i32, Logical> {
        (self.native.w, self.native.h).into()
    }
}

/// `wl_output`-style transform that displays an upright logical frame on a
/// panel turned to `o`.
#[must_use]
pub(crate) fn transform_for(o: Orientation) -> Transform {
    match o {
        Orientation::Portrait => Transform::Normal,
        Orientation::LandscapeCw => Transform::_90,
        Orientation::PortraitFlipped => Transform::_180,
        Orientation::LandscapeCcw => Transform::_270,
    }
}

/// Rotation policy as stored in prefs.
#[must_use]
pub(crate) fn policy_from_prefs(p: &de_config::Prefs) -> RotationPolicy {
    RotationPolicy {
        auto: p.auto_rotate,
        locked: Orientation::parse(&p.rotation_lock).unwrap_or_default(),
    }
}

impl De {
    /// Hardware raw point -> logical.
    pub(crate) fn hw_point(&self, p: Point<f64, Logical>) -> Point<f64, Logical> {
        self.rotation.map_hw_point(p)
    }

    /// Orientation the foreground wants (shell screens follow the system).
    fn foreground_orientation_request(&self) -> OrientationRequest {
        if self.foreground != Foreground::App {
            return OrientationRequest::Unspecified;
        }
        let Some(app_id) = self.foreground_app_id() else {
            return OrientationRequest::Unspecified;
        };
        self.rotation
            .app_requests
            .get(&app_id)
            .copied()
            .or_else(|| self.catalog.find(&app_id).map(|e| e.orientation))
            .unwrap_or_default()
    }

    fn foreground_app_id(&self) -> Option<String> {
        let window = self.recent_windows().into_iter().next()?;
        let toplevel = window.toplevel()?;
        smithay::wayland::compositor::with_states(toplevel.wl_surface(), |states| {
            states
                .data_map
                .get::<smithay::wayland::shell::xdg::XdgToplevelSurfaceData>()
                .and_then(|d| d.lock().ok()?.app_id.clone())
        })
    }

    /// Timer entry (every ~100 ms): sample the accelerometer, debounce, and
    /// rotate when the resolved orientation changed.
    pub(crate) fn poll_orientation(&mut self) {
        let now = Instant::now();
        if now < self.rotation.next_poll || self.sleeping() {
            return;
        }
        self.rotation.next_poll = now + POLL_EVERY;
        if let Some(sample) = self.rotation.source.poll() {
            let ms = self.uptime_ms(now);
            if let Some(o) = self.rotation.detector.update(sample, ms) {
                self.rotation.sensed = Some(o);
            }
        }
        self.reevaluate_orientation();
    }

    /// Re-run the sensor/policy/app resolution and rotate if it moved.
    pub(crate) fn reevaluate_orientation(&mut self) {
        let target = resolve_orientation(
            self.rotation.policy,
            self.foreground_orientation_request(),
            self.rotation.sensed,
            self.rotation.current,
        );
        if target != self.rotation.current {
            self.apply_orientation(target);
        }
    }

    /// Auto-rotate tile / `ctl rotate`: `auto` follows the sensor, otherwise
    /// lock to the given orientation.
    pub(crate) fn set_rotation_policy(&mut self, policy: RotationPolicy) {
        self.rotation.policy = policy;
        let _ = self
            .app_prefs
            .set_rotation(policy.auto, policy.locked.name());
        self.reevaluate_orientation();
    }

    /// Android semantics: switching auto-rotate off freezes what is showing.
    pub(crate) fn toggle_auto_rotate(&mut self) {
        let policy = if self.rotation.policy.auto {
            RotationPolicy {
                auto: false,
                locked: self.rotation.current,
            }
        } else {
            RotationPolicy {
                auto: true,
                ..self.rotation.policy
            }
        };
        self.set_rotation_policy(policy);
    }

    /// `ctl rotate <mode>`.
    pub(crate) fn set_rotation_mode(&mut self, mode: &str) -> Result<(), String> {
        let policy = if mode == "auto" {
            RotationPolicy {
                auto: true,
                ..self.rotation.policy
            }
        } else {
            let o = Orientation::parse(mode).ok_or_else(|| format!("unknown rotation '{mode}'"))?;
            RotationPolicy {
                auto: false,
                locked: o,
            }
        };
        self.set_rotation_policy(policy);
        Ok(())
    }

    /// Rotate the output to `o`: resize the logical output, push the new
    /// transform to scanout and wl_output, reconfigure every surface and
    /// drop every size-dependent cache.
    pub(crate) fn apply_orientation(&mut self, o: Orientation) {
        let native = self.rotation.native;
        let logical = o.logical_size(native);
        eprintln!(
            "rabbiz-de: rotate {} -> {} ({}x{})",
            self.rotation.current.name(),
            o.name(),
            logical.w,
            logical.h
        );
        self.rotation.current = o;
        self.output.width = logical.w as u32;
        self.output.height = logical.h as u32;

        let refresh = (self.output.refresh_hz * 1000.0) as i32;
        let transform = transform_for(o);
        let (mode_size, out_transform) = if self.compositor.is_some() {
            (native, transform)
        } else {
            (logical, Transform::Normal)
        };
        self.output_obj.change_current_state(
            Some(Mode {
                size: (mode_size.w, mode_size.h).into(),
                refresh,
            }),
            Some(out_transform),
            None,
            None,
        );
        if let Some(compositor) = self.compositor.as_mut() {
            compositor.set_output_mode_source(smithay::output::OutputModeSource::Static {
                size: (native.w, native.h).into(),
                scale: smithay::utils::Scale::from(1.0),
                transform: out_transform,
            });
        }
        layer_map_for_output(&self.output_obj).arrange();

        // Gesture zones, pull reach.
        let mut cfg = de_ui::gestures::GestureConfig::a21s();
        cfg.output_w = logical.w;
        cfg.output_h = logical.h;
        self.gestures = de_ui::gestures::GestureTracker::new(cfg);
        self.touch_claimed = false;

        // Clients: reconfigure for Desktop Mode (floating) or Handset Mode (fullscreen).
        self.reconfigure_windows_for_mode();
        self.sync_layer_insets();

        // Every raster is size-bound.
        self.shell_px = None;
        self.shell_tex = None;
        self.gesture_bar = None;
        self.wallpaper = None;
        self.meta_cache = None;
        self.brand_px = de_ui::boot::brand_frame(logical, 0.5);
        self.panels.on_resize(logical.h);
        self.home_ui.on_resize();
        self.lock.on_resize();
        self.power.on_resize();
        self.osd_fx.on_resize();
        self.shot_fx.on_resize();
        self.hun_fx.on_resize();
        self.navigation.on_resize();
        self.media.on_resize();
        self.on_emu_resize();
        self.shell_dirty = true;
        self.needs_present = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transform_matches_input_mapping() {
        // The scanout transform must agree with the touch mapping. smithay's
        // DrmCompositor places elements with `output_transform.invert()`, so a
        // logical point lands where `transform_for(o).invert()` puts it — and
        // that must be the native point `Orientation::logical_to_native` says.
        let native = ApiSize::new(720, 1600);
        for o in Orientation::ALL {
            let l = o.logical_size(native);
            let area = smithay::utils::Size::<f64, Logical>::from((f64::from(l.w), f64::from(l.h)));
            for (x, y) in [(0.0, 0.0), (10.0, 30.0), (f64::from(l.w) - 1.0, 5.0)] {
                let t = transform_for(o).invert().transform_point_in(Point::<f64, Logical>::from((x, y)), &area);
                let (nx, ny) = o.logical_to_native(x, y, native);
                assert!(
                    (t.x - nx).abs() < 1e-6 && (t.y - ny).abs() < 1e-6,
                    "{o:?}: transform gave ({}, {}), mapping ({nx}, {ny})",
                    t.x,
                    t.y
                );
            }
        }
    }

    #[test]
    fn policy_round_trips_prefs() {
        let mut p = de_config::Prefs::default();
        assert!(policy_from_prefs(&p).auto);
        p.auto_rotate = false;
        p.rotation_lock = "landscape_cw".into();
        let pol = policy_from_prefs(&p);
        assert!(!pol.auto);
        assert_eq!(pol.locked, Orientation::LandscapeCw);
    }
}
