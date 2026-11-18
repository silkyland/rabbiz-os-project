//! Output, viewporter and dmabuf handler impls (M1-T3).

use smithay::backend::allocator::dmabuf::Dmabuf;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::wayland::compositor;
use smithay::wayland::dmabuf::{DmabufGlobal, DmabufHandler, DmabufState, ImportNotifier};
use smithay::wayland::fractional_scale::{self, FractionalScaleHandler};
use smithay::wayland::output::OutputHandler;

use crate::state::De;

impl OutputHandler for De {}
smithay::delegate_output!(De);
smithay::delegate_viewporter!(De);

impl FractionalScaleHandler for De {
    fn new_fractional_scale(&mut self, surface: WlSurface) {
        // Single output, scale 1 — but advertising wp_fractional_scale_v1 is
        // what makes clients like Chrome (--force-device-scale-factor) route
        // through wp_viewport so their oversized buffers get downscaled.
        compositor::with_states(&surface, |states| {
            fractional_scale::with_fractional_scale(states, |fs| {
                fs.set_preferred_scale(1.0);
            });
        });
    }
}
smithay::delegate_fractional_scale!(De);

impl DmabufHandler for De {
    fn dmabuf_state(&mut self) -> &mut DmabufState {
        &mut self.dmabuf_state
    }

    fn dmabuf_imported(
        &mut self,
        _global: &DmabufGlobal,
        _dmabuf: Dmabuf,
        notifier: ImportNotifier,
    ) {
        // Lazy import at first draw is correct for both renderers; nothing to
        // do eagerly here beyond acknowledging success.
        let _ = notifier.successful::<De>();
    }
}
smithay::delegate_dmabuf!(De);
