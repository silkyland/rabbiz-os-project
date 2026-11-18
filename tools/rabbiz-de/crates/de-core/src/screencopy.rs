//! `zwlr_screencopy_manager_v1` — output capture into client shm buffers
//! (grim / `rabbiz-screenshot` / wayvnc's Rabbiz Mirror path).
//!
//! v1 is shm-only: the frame's `buffer` event offers `Argb8888` and
//! `Xrgb8888`; no `linux_dmabuf` event is sent. `copy` composites
//! immediately through [`composite_output`]; `copy_with_damage` queues on
//! `De::screencopy_pending` and completes after the next non-empty
//! `render_frame`, matching the protocol's "waits until there is damage"
//! contract.

use smithay::reexports::wayland_protocols_wlr::screencopy::v1::server::{
    zwlr_screencopy_frame_v1::{self, ZwlrScreencopyFrameV1},
    zwlr_screencopy_manager_v1::{self, ZwlrScreencopyManagerV1},
};
use smithay::reexports::wayland_server::backend::{ClientId, GlobalId};
use smithay::reexports::wayland_server::protocol::{wl_buffer::WlBuffer, wl_shm};
use smithay::reexports::wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource,
};
use smithay::utils::{Logical, Rectangle};
use smithay::wayland::shm;

use crate::composite::{composite_output, crop};
use crate::state::De;

/// Highest protocol version we implement (v3 = `buffer_done` +
/// `linux_dmabuf` enumeration — the latter simply is not sent for shm-only).
const VERSION: u32 = 3;

/// A queued `copy_with_damage` request, completed on the next damaged frame.
pub(crate) struct PendingCapture {
    frame: ZwlrScreencopyFrameV1,
    buffer: WlBuffer,
    rect: Rectangle<i32, Logical>,
}

/// Register the global. Called once from `state::init`.
pub(crate) fn init(dh: &DisplayHandle) -> GlobalId {
    dh.create_global::<De, ZwlrScreencopyManagerV1, ()>(VERSION, ())
}

struct FrameData {
    rect: Rectangle<i32, Logical>,
    /// A frame may be copied once; a second `copy` is `already_used`.
    used: std::sync::atomic::AtomicBool,
}

impl GlobalDispatch<ZwlrScreencopyManagerV1, ()> for De {
    fn bind(
        _state: &mut Self,
        _dh: &DisplayHandle,
        _client: &Client,
        resource: New<ZwlrScreencopyManagerV1>,
        _data: &(),
        init: &mut DataInit<'_, Self>,
    ) {
        init.init(resource, ());
    }
}

impl Dispatch<ZwlrScreencopyManagerV1, ()> for De {
    fn request(
        state: &mut Self,
        _client: &Client,
        resource: &ZwlrScreencopyManagerV1,
        request: zwlr_screencopy_manager_v1::Request,
        _data: &(),
        _dh: &DisplayHandle,
        init: &mut DataInit<'_, Self>,
    ) {
        let full =
            Rectangle::from_size((state.output.width as i32, state.output.height as i32).into());
        let (frame, rect) = match request {
            zwlr_screencopy_manager_v1::Request::CaptureOutput { frame, .. } => (frame, full),
            zwlr_screencopy_manager_v1::Request::CaptureOutputRegion {
                frame,
                x,
                y,
                width,
                height,
                ..
            } => (
                frame,
                full.intersection(Rectangle::new((x, y).into(), (width, height).into()))
                    .unwrap_or_default(),
            ),
            zwlr_screencopy_manager_v1::Request::Destroy => return,
            _ => return,
        };
        if rect.is_empty() {
            return;
        }
        let version = resource.version();
        let frame = init.init(
            frame,
            FrameData {
                rect,
                used: std::sync::atomic::AtomicBool::new(false),
            },
        );
        // Protocol: `buffer` is sent ONCE (one shm format offer), never
        // per-format — grim sends `copy` per `buffer` event, and a second
        // copy must fail with `already_used`.
        frame.buffer(
            wl_shm::Format::Argb8888.into(),
            rect.size.w as u32,
            rect.size.h as u32,
            rect.size.w as u32 * 4,
        );
        if version >= 3 {
            frame.buffer_done();
        }
    }
}

impl Dispatch<ZwlrScreencopyFrameV1, FrameData> for De {
    fn request(
        state: &mut Self,
        _client: &Client,
        frame: &ZwlrScreencopyFrameV1,
        request: zwlr_screencopy_frame_v1::Request,
        data: &FrameData,
        _dh: &DisplayHandle,
        _init: &mut DataInit<'_, Self>,
    ) {
        let with_damage =
            matches!(request, zwlr_screencopy_frame_v1::Request::CopyWithDamage { .. });
        let buffer = match request {
            zwlr_screencopy_frame_v1::Request::Copy { buffer }
            | zwlr_screencopy_frame_v1::Request::CopyWithDamage { buffer } => buffer,
            zwlr_screencopy_frame_v1::Request::Destroy => return,
            _ => return,
        };
        if data.used.swap(true, std::sync::atomic::Ordering::Relaxed) {
            frame.post_error(
                zwlr_screencopy_frame_v1::Error::AlreadyUsed as u32,
                "frame already used to copy a buffer",
            );
            return;
        }
        if !buffer_ok(&buffer, data.rect) {
            frame.post_error(
                zwlr_screencopy_frame_v1::Error::InvalidBuffer as u32,
                "buffer size/format does not match the offer",
            );
            return;
        }
        if with_damage {
            state.screencopy_pending.push(PendingCapture {
                frame: frame.clone(),
                buffer,
                rect: data.rect,
            });
        } else if !state.screencopy_copy(frame, &buffer, data.rect) {
            frame.failed();
        }
    }

    fn destroyed(
        state: &mut Self,
        _client: ClientId,
        resource: &ZwlrScreencopyFrameV1,
        _data: &FrameData,
    ) {
        state
            .screencopy_pending
            .retain(|p| p.frame.id() != resource.id());
    }
}

impl De {
    /// Immediate `copy` path: composite, write, `flags` + `ready`.
    fn screencopy_copy(
        &mut self,
        frame: &ZwlrScreencopyFrameV1,
        buffer: &WlBuffer,
        rect: Rectangle<i32, Logical>,
    ) -> bool {
        let Some(full) = composite_output(self) else {
            return false;
        };
        if !write_capture(&full, buffer, rect) {
            return false;
        }
        frame.flags(zwlr_screencopy_frame_v1::Flags::empty());
        let (hi, lo, nsec) = ready_timestamp(self);
        frame.ready(hi, lo, nsec);
        true
    }

    /// Complete queued `copy_with_damage` requests after a damaged frame was
    /// queued. Called from `present_once` only when `render_frame` returned
    /// real damage.
    pub(crate) fn screencopy_flush_pending(&mut self) {
        if self.screencopy_pending.is_empty() {
            return;
        }
        let Some(full) = composite_output(self) else {
            return;
        };
        let (hi, lo, nsec) = ready_timestamp(self);
        for p in std::mem::take(&mut self.screencopy_pending) {
            if !p.frame.is_alive() || !p.buffer.is_alive() {
                continue;
            }
            if write_capture(&full, &p.buffer, p.rect) {
                p.frame.damage(
                    p.rect.loc.x.max(0) as u32,
                    p.rect.loc.y.max(0) as u32,
                    p.rect.size.w as u32,
                    p.rect.size.h as u32,
                );
                p.frame.flags(zwlr_screencopy_frame_v1::Flags::empty());
                p.frame.ready(hi, lo, nsec);
            } else {
                p.frame.failed();
            }
        }
    }
}

/// `ready` timestamp from the monotonic clock (tv_sec_hi, tv_sec_lo, tv_nsec).
fn ready_timestamp(de: &De) -> (u32, u32, u32) {
    let d = std::time::Duration::from(de.clock.now());
    let sec = d.as_secs();
    (
        (sec >> 32) as u32,
        (sec & 0xffff_ffff) as u32,
        d.subsec_nanos(),
    )
}

/// Client buffer must be shm, exactly the captured size, in a format we can
/// write.
fn buffer_ok(buffer: &WlBuffer, rect: Rectangle<i32, Logical>) -> bool {
    shm::with_buffer_contents(buffer, |_ptr, _len, data| {
        matches!(
            data.format,
            wl_shm::Format::Argb8888 | wl_shm::Format::Xrgb8888
        ) && data.width == rect.size.w
            && data.height == rect.size.h
            && data.stride >= rect.size.w * 4
    })
    .unwrap_or(false)
}

/// Convert the captured region into the client buffer's format and write it.
#[allow(unsafe_code)] // shm pool is only reachable through the raw pointer
fn write_capture(full: &de_api::Pixels, buffer: &WlBuffer, rect: Rectangle<i32, Logical>) -> bool {
    let Some(src) = crop(
        full,
        de_api::Rect::new(rect.loc.x, rect.loc.y, rect.size.w, rect.size.h),
    ) else {
        return false;
    };
    shm::with_buffer_contents_mut(buffer, |ptr, len, data| {
        let swap_rb = matches!(
            data.format,
            wl_shm::Format::Argb8888 | wl_shm::Format::Xrgb8888
        );
        let opaque = matches!(
            data.format,
            wl_shm::Format::Xrgb8888 | wl_shm::Format::Xbgr8888
        );
        let (w, h) = (data.width, data.height);
        if w != src.size.w || h != src.size.h || data.offset < 0 || data.stride < w * 4 {
            return false;
        }
        let need = data.offset as usize + (h as usize - 1) * data.stride as usize + w as usize * 4;
        if need > len {
            return false;
        }
        for y in 0..h {
            let row = (data.offset + y * data.stride) as usize;
            for x in 0..w {
                let s = (y * w + x) as usize * 4;
                let d = row + x as usize * 4;
                let (r, g, b, a) = (
                    src.data[s],
                    src.data[s + 1],
                    src.data[s + 2],
                    src.data[s + 3],
                );
                // SAFETY: bounds checked above against the pool length.
                unsafe {
                    if swap_rb {
                        ptr.add(d).write(b);
                        ptr.add(d + 1).write(g);
                        ptr.add(d + 2).write(r);
                    } else {
                        ptr.add(d).write(r);
                        ptr.add(d + 1).write(g);
                        ptr.add(d + 2).write(b);
                    }
                    ptr.add(d + 3).write(if opaque { 255 } else { a });
                }
            }
        }
        true
    })
    .unwrap_or(false)
}
