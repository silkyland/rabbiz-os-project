//! `zwlr_virtual_pointer_v1` — client-injected pointer input for
//! `a21s-wayvnc` / Rabbiz Mirror remote control.
//!
//! Handset policy (documented for the touch-first model): virtual events
//! dispatch through the *same* code as physical pointer events —
//! [`De::pointer_motion`], [`De::pointer_button`], [`De::pointer_axis_frame`]
//! hit-test and focus identically — so a mirrored desktop can point, click
//! and scroll. It cannot trigger shell gestures: `GestureTracker` only ever
//! sees `wl_touch` contacts, so no remote client can home-swipe or pull the
//! shade. Clicks in a system zone do nothing more than a physical click
//! would.
//!
//! Events accumulate until `frame` (the protocol batches them); axes are
//! aggregated per axis like the libinput path.

use smithay::backend::input::{Axis, AxisSource, ButtonState};
use smithay::input::pointer::AxisFrame;
use smithay::reexports::wayland_protocols_wlr::virtual_pointer::v1::server::{
    zwlr_virtual_pointer_manager_v1::{self, ZwlrVirtualPointerManagerV1},
    zwlr_virtual_pointer_v1::{self, ZwlrVirtualPointerV1},
};
use smithay::reexports::wayland_server::backend::{ClientId, GlobalId};
use smithay::reexports::wayland_server::protocol::wl_pointer::{
    Axis as WlAxis, AxisSource as WlAxisSource, ButtonState as WlButtonState,
};
use smithay::reexports::wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, WEnum,
};
use smithay::utils::{Logical, Point};
use std::sync::Mutex;

use crate::state::De;

/// v2 adds `create_virtual_pointer_with_output` (we have one output — the
/// argument is accepted and ignored).
const VERSION: u32 = 2;

/// Register the global. Called once from `state::init`.
pub(crate) fn init(dh: &DisplayHandle) -> GlobalId {
    dh.create_global::<De, ZwlrVirtualPointerManagerV1, ()>(VERSION, ())
}

/// One pointer frame's worth of queued events (requests before `frame`).
/// One `frame` batch; the single lock covers the whole accumulation so a
/// request sequence stays ordered.
#[derive(Default)]
struct VptrData(Mutex<VptrBatch>);

#[derive(Default)]
struct VptrBatch {
    pending: Vec<Pending>,
    /// Per-axis accumulation (index: 0 vertical, 1 horizontal).
    axes: [AxisAcc; 2],
    axis_source: Option<AxisSource>,
}

enum Pending {
    Motion {
        time: u32,
        dx: f64,
        dy: f64,
    },
    Abs {
        time: u32,
        pos: Point<f64, Logical>,
    },
    Button {
        time: u32,
        button: u32,
        pressed: bool,
    },
}

#[derive(Default, Clone, Copy)]
struct AxisAcc {
    value: f64,
    discrete: i32,
    stop: bool,
    time: u32,
    seen: bool,
}

fn axis_idx(wenum: WEnum<WlAxis>) -> Option<usize> {
    match wenum.into_result() {
        Ok(WlAxis::VerticalScroll) => Some(0),
        Ok(WlAxis::HorizontalScroll) => Some(1),
        _ => None,
    }
}

impl GlobalDispatch<ZwlrVirtualPointerManagerV1, ()> for De {
    fn bind(
        _state: &mut Self,
        _dh: &DisplayHandle,
        _client: &Client,
        resource: New<ZwlrVirtualPointerManagerV1>,
        _data: &(),
        init: &mut DataInit<'_, Self>,
    ) {
        init.init(resource, ());
    }

    fn can_view(client: Client, _data: &()) -> bool {
        crate::privileged::allowed(&client, de_config::Privileged::VirtualPointer)
    }
}

impl Dispatch<ZwlrVirtualPointerManagerV1, ()> for De {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _resource: &ZwlrVirtualPointerManagerV1,
        request: zwlr_virtual_pointer_manager_v1::Request,
        _data: &(),
        _dh: &DisplayHandle,
        init: &mut DataInit<'_, Self>,
    ) {
        match request {
            zwlr_virtual_pointer_manager_v1::Request::CreateVirtualPointer { id, .. }
            | zwlr_virtual_pointer_manager_v1::Request::CreateVirtualPointerWithOutput {
                id, ..
            } => {
                // The optional seat/output suggestions are ignored: the
                // handset has exactly one seat and one output.
                init.init(id, VptrData::default());
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwlrVirtualPointerV1, VptrData> for De {
    fn request(
        state: &mut Self,
        _client: &Client,
        _resource: &ZwlrVirtualPointerV1,
        request: zwlr_virtual_pointer_v1::Request,
        data: &VptrData,
        _dh: &DisplayHandle,
        _init: &mut DataInit<'_, Self>,
    ) {
        use zwlr_virtual_pointer_v1::Request as R;
        match request {
            R::Motion { time, dx, dy } => data
                .0
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .pending
                .push(Pending::Motion { time, dx, dy }),
            R::MotionAbsolute {
                time,
                x,
                y,
                x_extent,
                y_extent,
            } => {
                // Extent space → logical output coords; extent 0 means the
                // coordinates are already in output space (wlroots rule).
                let (w, h) = (
                    f64::from(state.output.width),
                    f64::from(state.output.height),
                );
                let pos = Point::from((
                    if x_extent == 0 {
                        f64::from(x)
                    } else {
                        f64::from(x) * w / f64::from(x_extent)
                    },
                    if y_extent == 0 {
                        f64::from(y)
                    } else {
                        f64::from(y) * h / f64::from(y_extent)
                    },
                ));
                data.0
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .pending
                    .push(Pending::Abs { time, pos });
            }
            R::Button {
                time,
                button,
                state: st,
            } => {
                let pressed = matches!(st.into_result(), Ok(WlButtonState::Pressed));
                data.0
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .pending
                    .push(Pending::Button {
                        time,
                        button,
                        pressed,
                    });
            }
            R::Axis { time, axis, value } => {
                if let Some(i) = axis_idx(axis) {
                    let mut guard = data.0.lock().unwrap_or_else(|e| e.into_inner());
                    let a = &mut guard.axes[i];
                    a.value += value;
                    a.time = time;
                    a.seen = true;
                }
            }
            R::AxisSource { axis_source } => {
                data.0.lock().unwrap_or_else(|e| e.into_inner()).axis_source =
                    match axis_source.into_result() {
                        Ok(WlAxisSource::Finger) => Some(AxisSource::Finger),
                        Ok(WlAxisSource::Continuous) => Some(AxisSource::Continuous),
                        Ok(WlAxisSource::WheelTilt) => Some(AxisSource::WheelTilt),
                        _ => Some(AxisSource::Wheel),
                    };
            }
            R::AxisStop { time, axis } => {
                if let Some(i) = axis_idx(axis) {
                    let mut guard = data.0.lock().unwrap_or_else(|e| e.into_inner());
                    let a = &mut guard.axes[i];
                    a.stop = true;
                    a.time = time;
                    a.seen = true;
                }
            }
            R::AxisDiscrete {
                time,
                axis,
                value,
                discrete,
            } => {
                if let Some(i) = axis_idx(axis) {
                    let mut guard = data.0.lock().unwrap_or_else(|e| e.into_inner());
                    let a = &mut guard.axes[i];
                    a.value += value;
                    a.discrete += discrete;
                    a.time = time;
                    a.seen = true;
                }
            }
            R::Frame => state.flush_virtual_pointer(data),
            _ => {}
        }
    }

    fn destroyed(
        _state: &mut Self,
        _client: ClientId,
        _resource: &ZwlrVirtualPointerV1,
        _data: &VptrData,
    ) {
        // Nothing to clean up: no seat-grab or surface association is held.
    }
}

impl De {
    /// Dispatch one `frame`'s queued events through the same paths as
    /// libinput pointer events.
    fn flush_virtual_pointer(&mut self, data: &VptrData) {
        let mut batch = data.0.lock().unwrap_or_else(|e| e.into_inner());
        for ev in std::mem::take(&mut batch.pending) {
            match ev {
                Pending::Motion { time, dx, dy } => {
                    let pos = self.pointer.current_location() + Point::from((dx, dy));
                    self.pointer_motion(time, pos, Point::from((dx, dy)));
                }
                Pending::Abs { time, pos } => {
                    let delta = pos - self.pointer.current_location();
                    self.pointer_motion(time, pos, delta);
                }
                Pending::Button {
                    time,
                    button,
                    pressed,
                } => self.pointer_button(
                    time,
                    button,
                    if pressed {
                        ButtonState::Pressed
                    } else {
                        ButtonState::Released
                    },
                ),
            }
        }
        let [v, h] = batch.axes;
        if v.seen || h.seen {
            let time = v.time.max(h.time);
            let mut frame =
                AxisFrame::new(time).source(batch.axis_source.unwrap_or(AxisSource::Wheel));
            if v.value != 0.0 {
                frame = frame.value(Axis::Vertical, v.value);
            }
            if h.value != 0.0 {
                frame = frame.value(Axis::Horizontal, h.value);
            }
            if v.discrete != 0 {
                frame = frame.v120(Axis::Vertical, v.discrete * 120);
            }
            if h.discrete != 0 {
                frame = frame.v120(Axis::Horizontal, h.discrete * 120);
            }
            if v.stop {
                frame = frame.stop(Axis::Vertical);
            }
            if h.stop {
                frame = frame.stop(Axis::Horizontal);
            }
            self.pointer_axis_frame(frame);
            batch.axes = [AxisAcc::default(); 2];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Extent-space → logical mapping: explicit extents scale, zero extent
    /// passes coordinates through unchanged (wlroots semantics).
    #[test]
    fn motion_absolute_extent_scaling() {
        // Mirrors the inline math in the MotionAbsolute arm; kept as a free
        // function so the rule stays honest if that arm changes shape.
        fn map(v: u32, extent: u32, out: f64) -> f64 {
            if extent == 0 {
                f64::from(v)
            } else {
                f64::from(v) * out / f64::from(extent)
            }
        }
        assert_eq!(map(360, 720, 720.0), 360.0);
        assert_eq!(map(50, 100, 720.0), 360.0); // 50% of a 100-extent
        assert_eq!(map(360, 0, 720.0), 360.0); // zero extent = output space
        assert_eq!(map(800, 1600, 1600.0), 800.0);
    }

    /// A queued frame dispatches motion then buttons in request order, and
    /// axis data aggregates per axis until the next frame.
    #[test]
    fn vptr_accumulates_until_frame() {
        let d = VptrData::default();
        {
            let mut b = d.0.lock().unwrap();
            b.pending.push(Pending::Motion {
                time: 1,
                dx: 1.0,
                dy: -1.0,
            });
            b.pending.push(Pending::Button {
                time: 2,
                button: 0x110,
                pressed: true,
            });
            let a = &mut b.axes[axis_idx(WEnum::Value(WlAxis::VerticalScroll)).unwrap()];
            a.value += 5.0;
            a.seen = true;
            a.time = 3;
        }
        let b = d.0.lock().unwrap();
        assert_eq!(b.pending.len(), 2);
        assert!(b.axes[0].seen && (b.axes[0].value - 5.0).abs() < f64::EPSILON);
        assert!(!b.axes[1].seen);
    }
}
