//! Layer-shell surface lifecycle, SHM buffer rendering, and core Wayland handlers.

use osk_render::AltBar;

use smithay_client_toolkit::reexports::client::{
    protocol::{wl_output, wl_seat, wl_shm, wl_surface},
    Connection, QueueHandle,
};
use smithay_client_toolkit::{
    compositor::CompositorHandler,
    delegate_compositor, delegate_layer, delegate_output, delegate_registry, delegate_seat,
    delegate_shm,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{Capability, SeatHandler, SeatState},
    shell::{
        wlr_layer::{LayerShellHandler, LayerSurface, LayerSurfaceConfigure},
        WaylandSurface,
    },
    shm::{Shm, ShmHandler},
};

use crate::app::App;

/// Matches `de_design::tokens::GESTURE_BAR_HEIGHT` so the bottom key row sits cleanly above `Zone::NavBar`.
pub(crate) const NAV_BAR_H: i32 = 32;

impl App {
    /// Surface height follows the active layout's base layer; the emoji/numbers rows stretch.
    pub(crate) fn sync_size(&mut self) {
        let h = self.keyboard.layout().height_px().round().max(1.0) as u32;
        if h != self.h {
            self.h = h;
            self.layer.set_size(0, h);
            if self.visible {
                self.layer.set_margin(0, 0, NAV_BAR_H, 0);
                self.layer.set_exclusive_zone(h as i32 + NAV_BAR_H);
            } else {
                self.layer.set_margin(0, 0, -(h as i32 + NAV_BAR_H), 0);
                self.layer.set_exclusive_zone(0);
            }
            self.layer.commit();
            self.pool.resize((self.w * self.h * 4 * 2) as usize).ok();
            self.draw_pending = true;
        }
    }

    pub(crate) fn draw(&mut self) {
        if !self.configured || !self.visible {
            return;
        }
        let pressed = self
            .alt
            .as_ref()
            .map(|alt| alt.key)
            .or_else(|| self.pressed.first().copied());
        let alts = self.alt.as_ref().map(|alt| AltBar {
            items: &alt.items,
            selected: alt.selected,
        });
        if let Some(items) = self.sheet.clone() {
            let pixmap = self.renderer.render_sheet(
                self.keyboard.layout(),
                &self.theme,
                1.0,
                "Clipboard",
                &items,
            );
            self.commit_pixmap(pixmap);
            return;
        }
        let suggestions = if self.keyboard.is_panel() {
            self.keyboard.strip_text().into_iter().collect::<Vec<_>>()
        } else {
            self.keyboard.suggestions()
        };
        let pixmap = self.renderer.render_full(
            self.keyboard.layout(),
            &self.layer_name(),
            self.keyboard.shift(),
            &self.theme,
            1.0,
            pressed,
            alts,
            &suggestions,
        );
        self.commit_pixmap(pixmap);
    }

    fn commit_pixmap(&mut self, pixmap: osk_render::Pixmap) {
        let w = pixmap.width() as i32;
        let h = pixmap.height() as i32;
        if w != self.w as i32 || h != self.h as i32 {
            self.w = w as u32;
            self.h = h as u32;
            self.layer.set_size(0, self.h);
            if self.visible {
                self.layer.set_margin(0, 0, NAV_BAR_H, 0);
                self.layer.set_exclusive_zone(self.h as i32 + NAV_BAR_H);
            } else {
                self.layer.set_margin(0, 0, -(self.h as i32 + NAV_BAR_H), 0);
                self.layer.set_exclusive_zone(0);
            }
            self.layer.commit();
            self.pool.resize((self.w * self.h * 4 * 2) as usize).ok();
            self.draw_pending = true;
            return;
        }
        let stride = w * 4;
        let (buffer, canvas) = match self
            .pool
            .create_buffer(w, h, stride, wl_shm::Format::Argb8888)
        {
            Ok(pair) => pair,
            Err(err) => {
                eprintln!("rabbiz-osk: buffer error {err:?}");
                return;
            }
        };
        for (dst, src) in canvas
            .chunks_exact_mut(4)
            .zip(pixmap.data().chunks_exact(4))
        {
            dst[0] = src[2];
            dst[1] = src[1];
            dst[2] = src[0];
            dst[3] = src[3];
        }
        let surface = self.layer.wl_surface().clone();
        surface.damage_buffer(0, 0, w, h);
        if let Err(err) = buffer.attach_to(&surface) {
            eprintln!("rabbiz-osk: attach failed {err:?}");
            return;
        }
        surface.commit();
        self.held = Some(buffer);
    }

    pub(crate) fn show(&mut self) {
        // Hardware-keyboard mode: the layer surface keeps its role and protocol objects
        // (text-input still works) but is never mapped — no margin flip, no buffer.
        if self.hw_keyboard {
            return;
        }
        if !self.visible {
            self.visible = true;
            // Keep the layer surface mapped: unmapping and re-mapping needs a fresh configure
            // and attaching a buffer first is a protocol error. Sliding it back in with the
            // margin is both simpler and flicker-free. Sit above the 32 px NavBar so the bottom
            // key row (Space / Return / ?123) is never swallowed by system gestures.
            self.layer.set_margin(0, 0, NAV_BAR_H, 0);
            self.layer.set_exclusive_zone(self.h as i32 + NAV_BAR_H);
            self.layer.commit();
        }
        self.sync_size();
        self.draw();
    }

    pub(crate) fn hide(&mut self) {
        if !self.visible {
            return;
        }
        self.visible = false;
        self.alt = None;
        self.pressed.clear();
        self.strip_touches.clear();
        let _ = self.gesture.cancel_all(self.now_ms());
        // Slide the surface below the screen edge and stop reserving space for it.
        self.layer.set_margin(0, 0, -(self.h as i32 + NAV_BAR_H), 0);
        self.layer.set_exclusive_zone(0);
        self.layer.commit();
    }
}

impl CompositorHandler for App {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: i32,
    ) {
    }
    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}
    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for App {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.out
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl LayerShellHandler for App {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        std::process::exit(0);
    }
    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _: u32,
    ) {
        if configure.new_size.0 > 0 {
            self.w = configure.new_size.0;
        }
        if configure.new_size.1 > 0 {
            self.h = configure.new_size.1;
        }
        self.configured = true;
        self.draw_pending = false;
        self.pool.resize((self.w * self.h * 4 * 2) as usize).ok();
        if self.visible {
            self.draw();
        }
    }
}

impl SeatHandler for App {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seats
    }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        cap: Capability,
    ) {
        if cap == Capability::Touch {
            if let Err(err) = self.seats.get_touch(qh, &seat) {
                eprintln!("rabbiz-osk: no touch: {err:?}");
            }
        }
    }
    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        _: Capability,
    ) {
    }
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl ShmHandler for App {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.reg
    }
    registry_handlers![OutputState, SeatState];
}

delegate_compositor!(App);
delegate_output!(App);
delegate_shm!(App);
delegate_seat!(App);
delegate_layer!(App);
delegate_registry!(App);
