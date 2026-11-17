//! Selection / clipboard handler impls (M1-T5): data device, primary
//! selection and wlr+ext data-control (wl-paste --watch for the OSK
//! clipboard sheet).

use std::os::unix::io::OwnedFd;

use smithay::input::Seat;
use smithay::reexports::wayland_server::protocol::wl_data_source::WlDataSource;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
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
    DataControlHandler, DataControlState,
};
use smithay::wayland::selection::SelectionHandler;

use crate::state::De;

impl SelectionHandler for De {
    type SelectionUserData = ();
}

impl DataDeviceHandler for De {
    fn data_device_state(&self) -> &DataDeviceState {
        &self.data_device_state
    }
}

impl ClientDndGrabHandler for De {
    fn started(&mut self, _source: Option<WlDataSource>, _icon: Option<WlSurface>, _s: Seat<Self>) {}
    fn dropped(&mut self, _target: Option<WlSurface>, _validated: bool, _s: Seat<Self>) {}
}

impl ServerDndGrabHandler for De {
    fn send(&mut self, _mime_type: String, _fd: OwnedFd, _seat: Seat<Self>) {}
}

smithay::delegate_data_device!(De);

impl PrimarySelectionHandler for De {
    fn primary_selection_state(&self) -> &PrimarySelectionState {
        &self.primary_selection_state
    }
}

smithay::delegate_primary_selection!(De);

// wlr-data-control (wl-clipboard / wl-paste --watch).
impl DataControlHandler for De {
    fn data_control_state(&self) -> &DataControlState {
        &self.data_control_state
    }
}
smithay::delegate_data_control!(De);

// ext-data-control (newer clients).
impl ExtDataControlHandler for De {
    fn data_control_state(&self) -> &ExtDataControlState {
        &self.ext_data_control_state
    }
}
smithay::delegate_ext_data_control!(De);
