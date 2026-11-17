use smithay::backend::allocator::dmabuf::Dmabuf;
use smithay::backend::allocator::{Format, Fourcc, Modifier as DrmModifier};
use smithay::desktop::{Space, Window};
use smithay::input::{Seat, SeatState};
use smithay::output::{Mode, Output, PhysicalProperties, Scale, Subpixel};
use smithay::reexports::wayland_server::backend::{ClientData, ClientId, DisconnectReason};
use smithay::reexports::wayland_server::protocol::wl_buffer::WlBuffer;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::DisplayHandle;
use smithay::utils::{Logical, Point, Serial, Transform};
use smithay::wayland::buffer::BufferHandler;
use smithay::wayland::compositor::{CompositorClientState, CompositorHandler, CompositorState};
use smithay::wayland::dmabuf::{DmabufFeedbackBuilder, DmabufGlobal, DmabufState, ImportNotifier};
use smithay::wayland::output::{OutputHandler, OutputManagerState};
use smithay::wayland::selection::data_device::DataDeviceState;
use smithay::wayland::selection::ext_data_control::DataControlState as ExtDataControlState;
use smithay::wayland::selection::primary_selection::PrimarySelectionState;
use smithay::wayland::selection::wlr_data_control::DataControlState as WlrDataControlState;
use smithay::wayland::shell::xdg::decoration::XdgDecorationState;
use smithay::wayland::shell::xdg::XdgShellState;
use smithay::wayland::shm::{ShmHandler, ShmState};
use smithay::wayland::viewporter::ViewporterState;
use smithay::{delegate_compositor, delegate_output, delegate_shm, delegate_viewporter};

#[derive(Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
}

impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {}
    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
}

pub struct ProtocolState {
    pub compositor_state: CompositorState,
    pub shm_state: ShmState,
    pub output_manager_state: OutputManagerState,
    pub seat_state: SeatState<Self>,
    pub xdg_shell_state: XdgShellState,
    pub xdg_decoration_state: XdgDecorationState,
    pub data_device_state: DataDeviceState,
    pub primary_selection_state: PrimarySelectionState,
    pub wlr_data_control_state: WlrDataControlState,
    pub ext_data_control_state: ExtDataControlState,
    pub viewporter_state: ViewporterState,
    pub dmabuf_state: DmabufState,
    pub dmabuf_global: Option<DmabufGlobal>,
    pub space: Space<Window>,
    pub seat: Seat<Self>,
    pub output: Output,
    pub output_size: (i32, i32),
    pub pending_dmabufs: Vec<(Dmabuf, ImportNotifier)>,
    pub pointer_location: Point<f64, Logical>,
}

impl ProtocolState {
    pub fn new(
        dh: &DisplayHandle,
        output_name: &str,
        width: u32,
        height: u32,
        refresh_mhz: i32,
        drm_dev_id: Option<libc::dev_t>,
        keymap_layout: &str,
        keymap_options: Option<&str>,
    ) -> Self {
        let compositor_state = CompositorState::new::<Self>(dh);
        let shm_state = ShmState::new::<Self>(dh, vec![]);
        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(dh);
        let mut seat_state = SeatState::new();
        let mut seat = seat_state.new_wl_seat(dh, "seat0");

        let xkb = smithay::input::keyboard::XkbConfig {
            rules: "",
            model: "",
            layout: keymap_layout,
            variant: "",
            options: keymap_options.map(str::to_string),
        };
        if let Err(err) = seat.add_keyboard(xkb, 250, 30) {
            eprintln!("seat.add_keyboard failed: {err}");
        }
        seat.add_pointer();
        seat.add_touch();

        let xdg_shell_state = XdgShellState::new::<Self>(dh);
        let xdg_decoration_state = XdgDecorationState::new::<Self>(dh);
        let data_device_state = DataDeviceState::new::<Self>(dh);
        let primary_selection_state = PrimarySelectionState::new::<Self>(dh);
        let wlr_data_control_state =
            WlrDataControlState::new::<Self, _>(dh, Some(&primary_selection_state), |_| true);
        let ext_data_control_state =
            ExtDataControlState::new::<Self, _>(dh, Some(&primary_selection_state), |_| true);
        let viewporter_state = ViewporterState::new::<Self>(dh);

        let mut dmabuf_state = DmabufState::new();
        let dmabuf_global = drm_dev_id.and_then(|dev_id| {
            let linear_formats = [
                Format {
                    code: Fourcc::Argb8888,
                    modifier: DrmModifier::Linear,
                },
                Format {
                    code: Fourcc::Xrgb8888,
                    modifier: DrmModifier::Linear,
                },
            ];
            match DmabufFeedbackBuilder::new(dev_id, linear_formats).build() {
                Ok(feedback) => {
                    Some(dmabuf_state.create_global_with_default_feedback::<Self>(dh, &feedback))
                }
                Err(err) => {
                    eprintln!("DmabufFeedbackBuilder failed: {err}");
                    None
                }
            }
        });

        let output = Output::new(
            output_name.to_string(),
            PhysicalProperties {
                size: (68, 152).into(),
                subpixel: Subpixel::Unknown,
                make: "Samsung".to_string(),
                model: "SM-A217F".to_string(),
            },
        );
        let mode = Mode {
            size: (width as i32, height as i32).into(),
            refresh: refresh_mhz,
        };
        output.change_current_state(
            Some(mode),
            Some(Transform::Normal),
            Some(Scale::Integer(1)),
            Some((0, 0).into()),
        );
        output.set_preferred(mode);
        output.create_global::<Self>(dh);

        let mut space = Space::default();
        space.map_output(&output, (0, 0));

        Self {
            compositor_state,
            shm_state,
            output_manager_state,
            seat_state,
            xdg_shell_state,
            xdg_decoration_state,
            data_device_state,
            primary_selection_state,
            wlr_data_control_state,
            ext_data_control_state,
            viewporter_state,
            dmabuf_state,
            dmabuf_global,
            space,
            seat,
            output,
            output_size: (width as i32, height as i32),
            pending_dmabufs: Vec::new(),
            pointer_location: (width as f64 / 2.0, height as f64 / 2.0).into(),
        }
    }

    pub fn top_window(&self) -> Option<&Window> {
        self.space.elements().last()
    }

    pub fn top_wl_surface(&self) -> Option<WlSurface> {
        self.top_window()
            .and_then(|w| w.toplevel().map(|t| t.wl_surface().clone()))
    }

    pub fn focus_top_window(&mut self, serial: Serial) {
        let target = self.top_wl_surface();
        if let Some(kb) = self.seat.get_keyboard() {
            kb.set_focus(self, target, serial);
        }
    }
}

impl BufferHandler for ProtocolState {
    fn buffer_destroyed(&mut self, _buffer: &WlBuffer) {}
}

impl CompositorHandler for ProtocolState {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

    fn client_compositor_state<'a>(
        &self,
        client: &'a smithay::reexports::wayland_server::Client,
    ) -> &'a CompositorClientState {
        &client.get_data::<ClientState>().unwrap().compositor_state
    }

    fn commit(&mut self, surface: &WlSurface) {
        smithay::backend::renderer::utils::on_commit_buffer_handler::<Self>(surface);
        if let Some(window) = self
            .space
            .elements()
            .find(|w| {
                w.toplevel()
                    .map(|t| t.wl_surface() == surface)
                    .unwrap_or(false)
            })
            .cloned()
        {
            window.on_commit();
        }
    }
}

impl ShmHandler for ProtocolState {
    fn shm_state(&self) -> &ShmState {
        &self.shm_state
    }
}

impl OutputHandler for ProtocolState {}

delegate_compositor!(ProtocolState);
delegate_shm!(ProtocolState);
delegate_output!(ProtocolState);
delegate_viewporter!(ProtocolState);
