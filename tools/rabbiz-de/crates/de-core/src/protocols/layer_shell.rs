//! Layer-shell surfaces share the output with the fullscreen handset shell.

use de_protocols::layer_shell::{origin, ABOVE_WINDOWS, BELOW_WINDOWS};
use smithay::desktop::{layer_map_for_output, LayerSurface, PopupKind, WindowSurfaceType};
use smithay::reexports::wayland_server::protocol::{wl_output::WlOutput, wl_surface::WlSurface};
use smithay::utils::{Logical, Point};
use smithay::wayland::compositor::with_states;
use smithay::wayland::shell::{
    wlr_layer::{self, KeyboardInteractivity, Layer, LayerSurfaceData, WlrLayerShellHandler},
    xdg::PopupSurface,
};

use crate::comp::has_buffer;
use crate::input::SurfaceHit;
use crate::state::De;

impl WlrLayerShellHandler for De {
    fn shell_state(&mut self) -> &mut wlr_layer::WlrLayerShellState {
        &mut self.layer_shell_state
    }

    fn new_layer_surface(
        &mut self,
        surface: wlr_layer::LayerSurface,
        output: Option<WlOutput>,
        _layer: Layer,
        namespace: String,
    ) {
        if output.is_some_and(|output| !self.output_obj.owns(&output)) {
            surface.send_close();
            return;
        }
        let layer = LayerSurface::new(surface, namespace);
        if let Err(err) = layer_map_for_output(&self.output_obj).map_layer(&layer) {
            eprintln!("map layer: {err}");
            layer.layer_surface().send_close();
        }
    }

    fn new_popup(&mut self, _parent: wlr_layer::LayerSurface, popup: PopupSurface) {
        if let Err(err) = self.popups.track_popup(PopupKind::from(popup)) {
            eprintln!("track layer popup: {err}");
        }
    }

    fn layer_destroyed(&mut self, surface: wlr_layer::LayerSurface) {
        let mut map = layer_map_for_output(&self.output_obj);
        let layer = map
            .layer_for_surface(surface.wl_surface(), WindowSurfaceType::TOPLEVEL)
            .cloned();
        if let Some(layer) = layer {
            map.unmap_layer(&layer);
            map.arrange();
        }
        drop(map);
        self.sync_layer_insets();
        self.needs_present = true;
    }
}

smithay::delegate_layer_shell!(De);

impl De {
    pub(crate) fn commit_layer(&mut self, surface: &WlSurface) {
        let mut map = layer_map_for_output(&self.output_obj);
        let Some(layer) = map
            .layer_for_surface(surface, WindowSurfaceType::TOPLEVEL)
            .cloned()
        else {
            return;
        };
        map.arrange();
        let configured = with_states(surface, |states| {
            states
                .data_map
                .get::<LayerSurfaceData>()
                .is_some_and(|data| data.lock().unwrap().initial_configure_sent)
        });
        if !configured {
            layer.layer_surface().send_configure();
        }
        drop(map);
        self.sync_layer_insets();
    }

    /// Recompute `insets.ime.bottom` from exclusive-zone layer-shell surfaces
    /// (such as the session OSK) and re-configure the active toplevel when it changes.
    pub(crate) fn sync_layer_insets(&mut self) {
        let out_h = self.output.height as i32;
        let nz = layer_map_for_output(&self.output_obj).non_exclusive_zone();
        let ime_bottom = (out_h - (nz.loc.y + nz.size.h)).max(0);
        if self.insets.ime.bottom == ime_bottom {
            return;
        }
        self.insets.ime.bottom = ime_bottom;
        let frame = self
            .insets
            .immersive_frame(de_api::Size::new(self.output.width as i32, out_h));
        let logical = smithay::utils::Size::<i32, Logical>::from((frame.w, frame.h));
        for window in self.space.elements() {
            if let Some(toplevel) = window.toplevel() {
                de_protocols::configure_fullscreen(toplevel, logical);
                toplevel.send_configure();
            }
        }
        self.shell_dirty = true;
        self.needs_present = true;
    }

    pub(crate) fn layer_under(&self, pos: Point<f64, Logical>, above: bool) -> Option<SurfaceHit> {
        let map = layer_map_for_output(&self.output_obj);
        let levels = if above { ABOVE_WINDOWS } else { BELOW_WINDOWS };
        for level in levels {
            for layer in map.layers_on(level).rev() {
                let Some(loc) = origin(&map, layer) else {
                    continue;
                };
                if let Some((surface, offset)) =
                    layer.surface_under(pos - loc.to_f64(), WindowSurfaceType::ALL)
                {
                    return Some(SurfaceHit {
                        surface,
                        location: (loc + offset).to_f64(),
                        keyboard_target: layer
                            .can_receive_keyboard_focus()
                            .then(|| layer.wl_surface().clone()),
                        above_windows: above,
                    });
                }
            }
        }
        None
    }

    pub(crate) fn exclusive_layer(&self) -> Option<WlSurface> {
        let map = layer_map_for_output(&self.output_obj);
        let focused = ABOVE_WINDOWS
            .into_iter()
            .flat_map(|level| map.layers_on(level).rev())
            .find(|layer| {
                layer.cached_state().keyboard_interactivity == KeyboardInteractivity::Exclusive
                    && has_buffer(layer.wl_surface())
            })
            .map(|layer| layer.wl_surface().clone());
        focused
    }
}
