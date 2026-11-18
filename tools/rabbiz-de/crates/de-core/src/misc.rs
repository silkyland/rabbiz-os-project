//! Output, viewporter and dmabuf handler impls (M1-T3).

use smithay::backend::allocator::dmabuf::Dmabuf;
use smithay::wayland::dmabuf::{DmabufGlobal, DmabufHandler, DmabufState, ImportNotifier};
use smithay::wayland::output::OutputHandler;

use crate::state::De;

impl OutputHandler for De {}
smithay::delegate_output!(De);
smithay::delegate_viewporter!(De);

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
