//! wl_compositor + wl_shm handler impls (M1-T3).

use smithay::backend::renderer::utils::{on_commit_buffer_handler, with_renderer_surface_state};
use smithay::reexports::wayland_server::protocol::wl_buffer::WlBuffer;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::Client;
use smithay::utils::{Logical, Point};
use smithay::wayland::buffer::BufferHandler;
use smithay::wayland::compositor::{
    get_parent, is_sync_subsurface, with_states, CompositorClientState, CompositorHandler,
    CompositorState,
};
use smithay::wayland::seat::WaylandFocus;
use smithay::wayland::shm::{ShmHandler, ShmState};

use crate::state::{De, DeClientState};

impl BufferHandler for De {
    fn buffer_destroyed(&mut self, _buffer: &WlBuffer) {}
}

impl CompositorHandler for De {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        &client
            .get_data::<DeClientState>()
            .expect("client has no DeClientState")
            .compositor_state
    }

    fn commit(&mut self, surface: &WlSurface) {
        let had_buffer = has_buffer(surface);
        on_commit_buffer_handler::<Self>(surface);
        self.commit_hook(surface);
        if !had_buffer && has_buffer(surface) {
            if let Some(window) = self.window_for_surface(surface) {
                self.first_app_frame(window);
            }
        }
    }
}

pub(crate) fn has_buffer(surface: &WlSurface) -> bool {
    with_renderer_surface_state(surface, |state| state.buffer().is_some()).unwrap_or(false)
}

impl ShmHandler for De {
    fn shm_state(&self) -> &ShmState {
        &self.shm_state
    }
}

smithay::delegate_compositor!(De);
smithay::delegate_shm!(De);

impl De {
    /// Shared commit bookkeeping: apply buffer offsets, send the initial
    /// configure, mark a frame as needed.
    fn commit_hook(&mut self, surface: &WlSurface) {
        if !is_sync_subsurface(surface) {
            let mut root = surface.clone();
            while let Some(parent) = get_parent(&root) {
                root = parent;
            }
            if let Some(window) = self.window_for_surface(&root) {
                // Refresh the window's cached bbox — Space::element_under
                // filters on it, so without this every hit test misses.
                window.on_commit();
                if &root == surface {
                    let buffer_offset = with_states(surface, |states| {
                        states
                            .cached_state
                            .get::<smithay::wayland::compositor::SurfaceAttributes>()
                            .current()
                            .buffer_delta
                            .take()
                    });
                    if let Some(offset) = buffer_offset {
                        if let Some(loc) = self.space.element_location(&window) {
                            self.space.map_element(
                                window,
                                loc + Point::<i32, Logical>::from((offset.x, offset.y)),
                                false,
                            );
                        }
                    }
                }
            }
        }
        self.popups.commit(surface);
        ensure_initial_configure(surface, &mut self.space, &mut self.popups);
        self.commit_layer(surface);
        self.needs_present = true;
    }
}

/// Send the first configure once a mapped toplevel/popup/layer commits —
/// smithay requires this to happen at commit time, not at creation.
fn ensure_initial_configure(
    surface: &WlSurface,
    space: &mut smithay::desktop::Space<smithay::desktop::Window>,
    popups: &mut smithay::desktop::PopupManager,
) {
    use smithay::desktop::{PopupKind, WindowSurfaceType};
    use smithay::wayland::compositor::TraversalAction;
    use smithay::wayland::shell::xdg::XdgToplevelSurfaceData;
    use std::cell::RefCell;

    with_states(surface, |states| {
        // keep the data_map slot present for resize bookkeeping
        states.data_map.insert_if_missing(|| RefCell::new(()));
    });

    if let Some(window) = space
        .elements()
        .find(|w| w.wl_surface().map(|s| &*s == surface).unwrap_or(false))
        .cloned()
    {
        if let Some(toplevel) = window.toplevel() {
            let sent = with_states(surface, |states| {
                states
                    .data_map
                    .get::<XdgToplevelSurfaceData>()
                    .unwrap()
                    .lock()
                    .unwrap()
                    .initial_configure_sent
            });
            if !sent {
                toplevel.send_configure();
            }
        }
        return;
    }

    if let Some(popup) = popups.find_popup(surface) {
        if let PopupKind::Xdg(ref popup) = popup {
            if !popup.is_initial_configure_sent() {
                let _ = popup.send_configure();
            }
        }
    }

    let _ = WindowSurfaceType::TOPLEVEL;
    let _ = TraversalAction::<()>::DoChildren(());
}
