use smithay::backend::allocator::dmabuf::Dmabuf;
use smithay::backend::allocator::{Buffer, Fourcc, Modifier as DrmModifier};
use smithay::delegate_dmabuf;
use smithay::wayland::dmabuf::{DmabufGlobal, DmabufHandler, DmabufState, ImportNotifier};

use crate::state::ProtocolState;

impl DmabufHandler for ProtocolState {
    fn dmabuf_state(&mut self) -> &mut DmabufState {
        &mut self.dmabuf_state
    }

    fn dmabuf_imported(
        &mut self,
        _global: &DmabufGlobal,
        dmabuf: Dmabuf,
        notifier: ImportNotifier,
    ) {
        let fmt = dmabuf.format();
        let linear_ok = fmt.modifier == DrmModifier::Linear
            && matches!(fmt.code, Fourcc::Argb8888 | Fourcc::Xrgb8888);
        if !linear_ok {
            notifier.invalid_format();
            return;
        }
        self.pending_dmabufs.push((dmabuf, notifier));
    }
}

delegate_dmabuf!(ProtocolState);
