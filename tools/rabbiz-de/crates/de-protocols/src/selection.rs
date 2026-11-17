use smithay::wayland::selection::data_device::{
    ClientDndGrabHandler, DataDeviceHandler, DataDeviceState, ServerDndGrabHandler,
};
use smithay::wayland::selection::ext_data_control;
use smithay::wayland::selection::primary_selection::{
    PrimarySelectionHandler, PrimarySelectionState,
};
use smithay::wayland::selection::wlr_data_control;
use smithay::wayland::selection::SelectionHandler;
use smithay::{
    delegate_data_control, delegate_data_device, delegate_ext_data_control,
    delegate_primary_selection,
};

use crate::state::ProtocolState;

impl SelectionHandler for ProtocolState {
    type SelectionUserData = ();
}

impl ClientDndGrabHandler for ProtocolState {}
impl ServerDndGrabHandler for ProtocolState {
    fn send(
        &mut self,
        _mime_type: String,
        _fd: std::os::unix::io::OwnedFd,
        _seat: smithay::input::Seat<Self>,
    ) {
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

/// wlr data-control (`zwlr_data_control_manager_v1`) — what `wl-paste
/// --watch` uses on wlroots-era tools.
impl wlr_data_control::DataControlHandler for ProtocolState {
    fn data_control_state(&self) -> &wlr_data_control::DataControlState {
        &self.wlr_data_control_state
    }
}

/// ext data-control (`ext_data_control_manager_v1`) — the stable successor.
impl ext_data_control::DataControlHandler for ProtocolState {
    fn data_control_state(&self) -> &ext_data_control::DataControlState {
        &self.ext_data_control_state
    }
}

delegate_data_device!(ProtocolState);
delegate_primary_selection!(ProtocolState);
delegate_data_control!(ProtocolState);
delegate_ext_data_control!(ProtocolState);
