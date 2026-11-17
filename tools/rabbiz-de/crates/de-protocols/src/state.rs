use smithay::backend::allocator::dmabuf::Dmabuf;
use smithay::backend::allocator::{Format, Fourcc, Modifier as DrmModifier};
use smithay::desktop::{PopupManager, Space, Window};
use smithay::input::{Seat, SeatState};
use smithay::output::{Mode, Output, PhysicalProperties, Scale, Subpixel};
use smithay::reexports::wayland_server::backend::{ClientData, ClientId, DisconnectReason};
use smithay::reexports::wayland_server::protocol::wl_buffer::WlBuffer;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::{Client, DisplayHandle, Resource};
use smithay::utils::{IsAlive, Logical, Point, Serial, Transform};
use smithay::wayland::buffer::BufferHandler;
use smithay::wayland::compositor::{
    CompositorClientState, CompositorHandler, CompositorState,
};
use smithay::wayland::dmabuf::{
    DmabufFeedbackBuilder, DmabufGlobal, DmabufState, ImportNotifier,
};
use smithay::wayland::output::{OutputHandler, OutputManagerState};
use smithay::wayland::selection::data_device::DataDeviceState;
use smithay::wayland::selection::ext_data_control::DataControlState as ExtDataControlState;
use smithay::wayland::selection::primary_selection::PrimarySelectionState;
use smithay::wayland::selection::wlr_data_control::DataControlState as WlrDataControlState;
use smithay::wayland::shell::xdg::decoration::XdgDecorationState;
use smithay::wayland::shell::xdg::XdgShellState;
use smithay::wayland::shm::{ShmHandler, ShmState};
use smithay::wayland::viewporter::ViewporterState;
use smithay::wayland::xdg_activation::XdgActivationState;
use smithay::{
    delegate_compositor, delegate_output, delegate_shm, delegate_viewporter,
};

use crate::focus::FocusStack;

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
    pub xdg_activation_state: XdgActivationState,
    pub data_device_state: DataDeviceState,
    pub primary_selection_state: PrimarySelectionState,
    pub wlr_data_control_state: WlrDataControlState,
    pub ext_data_control_state: ExtDataControlState,
    pub viewporter_state: ViewporterState,
    pub dmabuf_state: DmabufState,
    pub dmabuf_global: Option<DmabufGlobal>,
    pub space: Space<Window>,
    pub popup_manager: PopupManager,
    /// Toplevels that have been configured but not yet committed a buffer.
    /// They enter `space` (and therefore rendering/focus) only on first commit.
    pub uncommitted_windows: Vec<Window>,
    /// Ordered focus stack of mapped toplevel `wl_surface`s; last = topmost.
    /// Destruction of the top entry falls back to the previous window.
    pub focus_stack: FocusStack<WlSurface>,
    /// Set when a client holds an xdg_popup grab on a mapped popup; touch and
    /// keyboard focus go to that popup surface while the grab is active.
    pub popup_grab: Option<WlSurface>,
    pub seat: Seat<Self>,
    pub output: Output,
    pub output_size: (i32, i32),
    pub pending_dmabufs: Vec<(Dmabuf, ImportNotifier)>,
    pub pointer_location: Point<f64, Logical>,
    dh: DisplayHandle,
}

impl ProtocolState {
    pub fn new(
        dh: &DisplayHandle,
        output_name: &str,
        width: u32,
        height: u32,
        refresh_mhz: i32,
        drm_dev_id: Option<libc::dev_t>,
    ) -> Result<Self, String> {
        let compositor_state = CompositorState::new::<Self>(dh);
        let shm_state = ShmState::new::<Self>(dh, vec![]);
        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(dh);
        let mut seat_state = SeatState::new();
        let mut seat = seat_state.new_wl_seat(dh, "seat0");

        let xkb = smithay::input::keyboard::XkbConfig {
            rules: "",
            model: "",
            layout: "us,th",
            variant: "",
            options: Some("grp:alt_shift_toggle".to_string()),
        };
        // Fatal: a compositor that cannot build its keymap cannot type into
        // apps, so continuing silently would leave a half-working session.
        seat.add_keyboard(xkb, 250, 30)
            .map_err(|err| format!("seat.add_keyboard failed: {err}"))?;
        seat.add_pointer();
        seat.add_touch();

        let xdg_shell_state = XdgShellState::new::<Self>(dh);
        let xdg_decoration_state = XdgDecorationState::new::<Self>(dh);
        let xdg_activation_state = XdgActivationState::new::<Self>(dh);
        let data_device_state = DataDeviceState::new::<Self>(dh);
        let primary_selection_state = PrimarySelectionState::new::<Self>(dh);
        // wl-paste --watch uses zwlr_data_control_manager_v1; wl-data-control
        // clients that prefer the stable ext variant get it too. The filter
        // allows every client: M1 has no security boundary between apps.
        let wlr_data_control_state = WlrDataControlState::new::<Self, _>(
            dh,
            Some(&primary_selection_state),
            |_| true,
        );
        let ext_data_control_state = ExtDataControlState::new::<Self, _>(
            dh,
            Some(&primary_selection_state),
            |_| true,
        );
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

        Ok(Self {
            compositor_state,
            shm_state,
            output_manager_state,
            seat_state,
            xdg_shell_state,
            xdg_decoration_state,
            xdg_activation_state,
            data_device_state,
            primary_selection_state,
            wlr_data_control_state,
            ext_data_control_state,
            viewporter_state,
            dmabuf_state,
            dmabuf_global,
            space,
            popup_manager: PopupManager::default(),
            uncommitted_windows: Vec::new(),
            focus_stack: FocusStack::default(),
            popup_grab: None,
            seat,
            output,
            output_size: (width as i32, height as i32),
            pending_dmabufs: Vec::new(),
            pointer_location: (width as f64 / 2.0, height as f64 / 2.0).into(),
            dh: dh.clone(),
        })
    }

    /// Input-focus surface used by the frame loop's libinput wiring.
    pub fn top_wl_surface(&self) -> Option<WlSurface> {
        self.focus_surface()
    }

    /// Client for the surface that should currently receive input focus.
    /// An active popup grab outranks the toplevel focus stack.
    pub fn focus_surface(&self) -> Option<WlSurface> {
        if let Some(popup) = &self.popup_grab {
            if popup.alive() {
                return Some(popup.clone());
            }
        }
        self.focus_stack.top().cloned()
    }

    /// Push a surface to the top of the focus stack.
    pub fn focus_stack_raise(&mut self, surface: &WlSurface) {
        self.focus_stack.raise(surface.clone());
    }

    /// Drop a surface from the focus stack (on unmap/destroy).
    pub fn focus_stack_remove(&mut self, surface: &WlSurface) {
        self.focus_stack.remove(surface);
    }

    pub fn display_handle(&self) -> &DisplayHandle {
        &self.dh
    }

    pub fn client_for_surface(&self, surface: &WlSurface) -> Option<Client> {
        self.dh.get_client(surface.id()).ok()
    }

    /// Focus the currently topmost input target (popup grab, else stack top).
    pub fn refocus(&mut self, serial: Serial) {
        let target = self.focus_surface();
        if let Some(kb) = self.seat.get_keyboard() {
            kb.set_focus(self, target, serial);
        }
    }

    /// Any pending `wl_callback` frame callbacks across mapped toplevel
    /// surfaces. Used by the frame loop to deliver callbacks for commits
    /// that produced no visible damage.
    pub fn has_pending_frame_callbacks(&self) -> bool {
        self.space.elements().any(|w| {
            w.toplevel()
                .map(|t| surface_has_frame_callbacks(t.wl_surface()))
                .unwrap_or(false)
        }) || self
            .mapped_popup_surfaces()
            .iter()
            .any(|s| surface_has_frame_callbacks(s))
    }

    /// WlSurfaces of mapped popups (for frame callbacks on popup commits).
    pub fn mapped_popup_surfaces(&self) -> Vec<WlSurface> {
        self.space
            .elements()
            .filter_map(|w| w.toplevel().map(|t| t.wl_surface().clone()))
            .flat_map(|surface| {
                PopupManager::popups_for_surface(&surface)
                    .map(|(popup, _)| popup.wl_surface().clone())
                    .collect::<Vec<_>>()
            })
            .collect()
    }
}

/// True if `surface` (or any surface in its tree) holds un-sent frame callbacks.
fn surface_has_frame_callbacks(surface: &WlSurface) -> bool {
    use std::cell::Cell;
    let pending = Cell::new(false);
    smithay::wayland::compositor::with_surface_tree_downward(
        surface,
        (),
        |_, _, &()| smithay::wayland::compositor::TraversalAction::DoChildren(()),
        |_, states, &()| {
            if !states
                .cached_state
                .get::<smithay::wayland::compositor::SurfaceAttributes>()
                .current()
                .frame_callbacks
                .is_empty()
            {
                pending.set(true);
            }
        },
        |_, _, &()| !pending.get(),
    );
    pending.get()
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
        self.popup_manager.commit(surface);
        smithay::backend::renderer::utils::on_commit_buffer_handler::<Self>(surface);

        // Toplevel first-commit mapping: the window only enters `space`
        // once the client attached a real buffer, so the shell bar and
        // focus never target a toplevel with nothing to show.
        if let Some(pos) = self
            .uncommitted_windows
            .iter()
            .position(|w| {
                w.toplevel()
                    .map(|t| t.wl_surface() == surface)
                    .unwrap_or(false)
            })
        {
            // The buffer assignment was just consumed into the renderer
            // surface state by `on_commit_buffer_handler` — read it there
            // (same check smithay's xdg post_commit_hook performs).
            let has_buffer = smithay::backend::renderer::utils::with_renderer_surface_state(
                surface,
                |state| state.buffer().is_some(),
            )
            .unwrap_or(false);
            if has_buffer {
                let window = self.uncommitted_windows.swap_remove(pos);
                self.space.map_element(window, (0, 0), true);
                self.focus_stack_raise(surface);
                self.refocus(smithay::utils::SERIAL_COUNTER.next_serial());
                return;
            }
        }

        if let Some(window) = self
            .space
            .elements()
            .find(|w| w.toplevel().map(|t| t.wl_surface() == surface).unwrap_or(false))
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
