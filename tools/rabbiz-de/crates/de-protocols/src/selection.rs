use std::os::unix::io::OwnedFd;

use smithay::input::Seat;
use smithay::wayland::selection::data_device::{
    ClientDndGrabHandler, DataDeviceHandler, DataDeviceState, ServerDndGrabHandler,
};
use smithay::wayland::selection::ext_data_control::{
    DataControlHandler as ExtDataControlHandler, DataControlState as ExtDataControlState,
};
use smithay::wayland::selection::primary_selection::{
    PrimarySelectionHandler, PrimarySelectionState,
};
use smithay::wayland::selection::wlr_data_control::{
    DataControlHandler as WlrDataControlHandler, DataControlState as WlrDataControlState,
};
use smithay::wayland::selection::{SelectionHandler, SelectionSource, SelectionTarget};
use smithay::{
    delegate_data_control, delegate_data_device, delegate_ext_data_control,
    delegate_primary_selection,
};

use crate::state::ProtocolState;

impl SelectionHandler for ProtocolState {
    type SelectionUserData = ();

    fn new_selection(
        &mut self,
        ty: SelectionTarget,
        _source: Option<SelectionSource>,
        _seat: Seat<Self>,
    ) {
        // Propagation to wl_data_device / primary / data-control watchers is
        // handled inside smithay's seat_data plumbing; this hook only proves
        // the change in the compositor log for clipboard debugging.
        eprintln!("selection: new {:?} selection", ty);
    }

    fn send_selection(
        &mut self,
        _ty: SelectionTarget,
        _mime_type: String,
        _fd: OwnedFd,
        _seat: Seat<Self>,
        _user_data: &Self::SelectionUserData,
    ) {
        // The compositor never installs a server-side selection, so this is
        // unreachable; client-provided selections are served peer-to-peer by
        // smithay without entering this hook.
    }
}

impl ClientDndGrabHandler for ProtocolState {}
impl ServerDndGrabHandler for ProtocolState {
    fn send(&mut self, _mime_type: String, _fd: OwnedFd, _seat: Seat<Self>) {
        // No server-side drag-and-drop sources in M1.
    }
}

impl DataDeviceHandler for ProtocolState {
    fn data_device_state(&self) -> &DataDeviceState {
        &self.data_device_state
    }
}

impl PrimarySelectionHandler for ProtocolState {
    fn primary_selection_state(&self) -> &PrimarySelectionState {
        &self.primary_selection_state
    }
}

impl WlrDataControlHandler for ProtocolState {
    fn data_control_state(&self) -> &WlrDataControlState {
        &self.wlr_data_control_state
    }
}

impl ExtDataControlHandler for ProtocolState {
    fn data_control_state(&self) -> &ExtDataControlState {
        &self.ext_data_control_state
    }
}

delegate_data_device!(ProtocolState);
delegate_primary_selection!(ProtocolState);
delegate_data_control!(ProtocolState);
delegate_ext_data_control!(ProtocolState);
