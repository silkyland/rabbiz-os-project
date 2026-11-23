//! `De` — the compositor's top-level state.
//!
//! M0 fields: session/DRM/compositor/renderer/brand. M1 adds the Wayland
//! side: `DisplayHandle`, protocol states, seat + touch/keyboard, the
//! window `Space` (fullscreen handset policy) and the in-process shell
//! stub textures.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;

use de_api::{OutputInfo, RendererKind};
use de_backend::drm::{DrmSetup, OutputGeometry};
use de_backend::probe::DeCompositor;
use de_backend::session::SessionError;
use de_config::flags::{BackendSelect, Flags, RendererSelect};
use de_render::{DeRenderer, DeTexture, RenderError};
use smithay::backend::egl::EGLDisplay;
use smithay::backend::session::libseat::{LibSeatSession, LibSeatSessionNotifier};
use smithay::desktop::{PopupManager, Space, Window};
use smithay::input::keyboard::{LedState, XkbConfig};
use smithay::input::pointer::PointerHandle;
use smithay::input::touch::TouchHandle;
use smithay::input::{Seat, SeatState};
use smithay::output::{Mode, Output, PhysicalProperties, Subpixel};
use smithay::reexports::wayland_server::{Display, DisplayHandle};
use smithay::utils::{Clock, Monotonic};
use smithay::wayland::compositor::{CompositorClientState, CompositorState};
use smithay::wayland::content_type::ContentTypeState;
use smithay::wayland::cursor_shape::CursorShapeManagerState;
use smithay::wayland::dmabuf::{DmabufGlobal, DmabufState};
use smithay::wayland::fractional_scale::FractionalScaleManagerState;
use smithay::wayland::idle_inhibit::IdleInhibitManagerState;
use smithay::wayland::input_method::InputMethodManagerState;
use smithay::wayland::keyboard_shortcuts_inhibit::KeyboardShortcutsInhibitState;
use smithay::wayland::output::OutputManagerState;
use smithay::wayland::pointer_constraints::PointerConstraintsState;
use smithay::wayland::presentation::PresentationState;
use smithay::wayland::relative_pointer::RelativePointerManagerState;
use smithay::wayland::selection::data_device::DataDeviceState;
use smithay::wayland::selection::ext_data_control::DataControlState as ExtDataControlState;
use smithay::wayland::selection::primary_selection::PrimarySelectionState;
use smithay::wayland::selection::wlr_data_control::DataControlState;
use smithay::wayland::session_lock::SessionLockManagerState;
use smithay::wayland::shell::wlr_layer::WlrLayerShellState;
use smithay::wayland::shell::xdg::decoration::XdgDecorationState;
use smithay::wayland::shell::xdg::XdgShellState;
use smithay::wayland::shm::ShmState;
use smithay::wayland::single_pixel_buffer::SinglePixelBufferState;
use smithay::wayland::text_input::TextInputManagerState;
use smithay::wayland::viewporter::ViewporterState;
use smithay::wayland::virtual_keyboard::VirtualKeyboardManagerState;
use smithay::wayland::xdg_activation::XdgActivationState;
use smithay::wayland::xdg_foreign::XdgForeignState;
use smithay::wayland::xdg_system_bell::XdgSystemBellState;
use smithay::wayland::xdg_toplevel_icon::XdgToplevelIconManager;
use smithay::wayland::xdg_toplevel_tag::XdgToplevelTagManager;

/// Free CMA below which `auto` picks the software renderer (plan Decision 2).
const CMA_FLOOR_KB: u64 = 96 * 1024;

/// Per-client state attached to every Wayland client (needed by
/// `CompositorHandler::client_compositor_state`).
#[derive(Debug, Default)]
pub struct DeClientState {
    pub compositor_state: CompositorClientState,
    pub(crate) session_input_method: bool,
    /// Privileged globals explicitly granted to a compositor-spawned private-socket helper.
    pub(crate) private_grants: Vec<de_config::Privileged>,
    /// Filled once when the socket is accepted (see `privileged.rs`).
    pub(crate) identity: std::sync::OnceLock<crate::privileged::ClientIdentity>,
}
impl smithay::reexports::wayland_server::backend::ClientData for DeClientState {
    fn initialized(&self, _client_id: smithay::reexports::wayland_server::backend::ClientId) {}
    fn disconnected(
        &self,
        _client_id: smithay::reexports::wayland_server::backend::ClientId,
        _reason: smithay::reexports::wayland_server::backend::DisconnectReason,
    ) {
    }
}

/// Everything `de-core` needs to run the session.
#[allow(clippy::struct_excessive_bools)] // compositor-wide state; the bools are independent flags
pub struct De {
    /// Kept alive for the session's lifetime (`None` in virtual mode).
    #[allow(dead_code)]
    pub session: Option<LibSeatSession>,
    pub seat_notifier: Option<LibSeatSessionNotifier>,
    pub setup: Option<DrmSetup>,
    pub compositor: Option<DeCompositor>,
    pub renderer: DeRenderer,
    /// The uploaded brand frame texture (validates the renderer at init).
    pub brand: DeTexture,
    /// Source pixels — re-uploaded when the home stub needs damage.
    pub brand_px: de_api::Pixels,
    pub output: OutputGeometry,
    pub started: Instant,
    /// First successful page flip has happened (gates readiness + exit code).
    pub first_flip: bool,

    // --- M1: Wayland state ---
    pub display_handle: DisplayHandle,
    pub socket_name: Option<String>,
    pub running: Arc<AtomicBool>,
    pub clock: Clock<Monotonic>,

    pub space: Space<Window>,
    pub popups: PopupManager,
    /// smithay Output object (for `send_frame`/globals).
    pub output_obj: Output,
    /// Orientation policy + sensor (`runtime/rotation.rs`).
    pub(crate) rotation: crate::rotation::Rotation,
    /// Active Desktop Mode (floating windows + auto-landscape on handset).
    pub(crate) desktop_mode: bool,
    /// Device's baseline form factor from `device.toml` (restored when exiting Desktop Mode).
    pub(crate) default_form_factor: de_design::motion::FormFactor,
    /// Saved rotation policy and orientation before entering Desktop Mode.
    pub(crate) pre_desktop_rotation: Option<(de_api::RotationPolicy, de_api::Orientation)>,
    /// Now-playing (pill + shade card).
    pub(crate) media: crate::media_ui::MediaFx,

    pub compositor_state: CompositorState,
    pub shm_state: ShmState,
    pub xdg_shell_state: XdgShellState,
    pub(crate) layer_shell_state: WlrLayerShellState,
    pub(crate) exclusive_layer_focus:
        Option<smithay::reexports::wayland_server::protocol::wl_surface::WlSurface>,
    pub xdg_decoration_state: XdgDecorationState,
    pub xdg_activation_state: XdgActivationState,
    pub data_device_state: DataDeviceState,
    pub primary_selection_state: PrimarySelectionState,
    pub data_control_state: DataControlState,
    pub ext_data_control_state: ExtDataControlState,
    pub seat_state: SeatState<De>,
    pub viewporter_state: ViewporterState,
    pub fractional_scale_state: FractionalScaleManagerState,
    pub output_manager_state: OutputManagerState,
    pub dmabuf_state: DmabufState,
    pub dmabuf_global: Option<DmabufGlobal>,
    /// Kept alive for the `zwp_idle_inhibit_manager_v1` global's lifetime.
    #[allow(dead_code)]
    pub(crate) idle_inhibit_state: IdleInhibitManagerState,
    /// Kept alive for `zwp_relative_pointer_manager_v1` / `zwp_pointer_constraints_v1`.
    #[allow(dead_code)]
    pub(crate) relative_pointer_state: RelativePointerManagerState,
    #[allow(dead_code)]
    pub(crate) pointer_constraints_state: PointerConstraintsState,
    pub(crate) session_lock_state: SessionLockManagerState,
    /// Single-lock and fail-closed recovery tracker for `ext-session-lock-v1`.
    pub(crate) session_lock_tracker: crate::protocols::session_lock::SessionLockTracker,

    // --- standard extras (`protocols/standards.rs`, audit 2026-11-21) ---
    #[allow(dead_code)]
    pub(crate) single_pixel_buffer_state: SinglePixelBufferState,
    #[allow(dead_code)]
    pub(crate) cursor_shape_state: CursorShapeManagerState,
    pub(crate) xdg_foreign_state: XdgForeignState,
    #[allow(dead_code)]
    pub(crate) content_type_state: ContentTypeState,
    #[allow(dead_code)]
    pub(crate) xdg_toplevel_icon_manager: XdgToplevelIconManager,
    #[allow(dead_code)]
    pub(crate) xdg_toplevel_tag_manager: XdgToplevelTagManager,
    pub(crate) kbd_shortcuts_inhibit_state: KeyboardShortcutsInhibitState,
    #[allow(dead_code)]
    pub(crate) system_bell_state: XdgSystemBellState,
    /// Kept alive for the `wp_presentation` global; feedback goes out beside
    /// `send_frames_surface_tree` (`frame/render.rs`).
    #[allow(dead_code)]
    pub(crate) presentation_state: PresentationState,
    /// Monotonic `wp_presentation` feedback sequence.
    pub(crate) present_seq: u64,

    /// Surfaces currently holding a `zwp_idle_inhibitor_v1` (`protocols/idle_inhibit.rs`).
    pub(crate) idle_inhibitors:
        Vec<smithay::reexports::wayland_server::protocol::wl_surface::WlSurface>,
    /// `org.freedesktop.ScreenSaver` service — cookies + activity pokes feed
    /// the same idle-blank gate as `idle_inhibitors` (`de-system/screensaver.rs`).
    pub(crate) screensaver: de_system::screensaver::ScreenSaver,

    pub seat: Seat<De>,
    pub seat_name: String,
    pub pointer: PointerHandle<De>,
    pub touch: TouchHandle<De>,

    // --- pointer / hardware-keyboard tracking (mouse+keyboard support) ---
    /// Any pointer-style event (motion/button/axis, physical or virtual)
    /// has been seen — the on-screen cursor is drawn from then on.
    pub pointer_active: bool,
    /// A physical keyboard is attached (libinput `Keyboard` capability on a
    /// device with a real USB/Bluetooth VID:PID — platform key devices like
    /// the power/volume buttons report `(0, 0)` and do not count).
    pub hw_keyboard: bool,
    /// A physical mouse/trackpad is attached (same VID:PID rule).
    pub hw_pointer: bool,
    /// Device ids currently counted as hardware keyboards/pointers.
    pub(crate) kbd_devices: std::collections::HashSet<String>,
    pub(crate) ptr_devices: std::collections::HashSet<String>,
    /// Left button went down on a shell-owned surface: `(pos, button)` —
    /// a nearby release becomes a tap (`input.rs`).
    pub pointer_press: Option<(smithay::utils::Point<f64, smithay::utils::Logical>, u32)>,
    /// Super (logo) key is held and no other key has been pressed since —
    /// releasing it alone toggles the app drawer (`shortcuts.rs`).
    pub launcher_pending: bool,
    /// Rasterized cursor sprite + its uploaded texture (lazily created in
    /// the frame pump, `frame.rs`).
    pub cursor_px: Option<de_api::Pixels>,
    pub cursor_tex: Option<Box<dyn std::any::Any>>,

    /// A client commit or shell change requested a frame.
    pub needs_present: bool,
    /// Shell frame texture; `None` = not yet uploaded. Stored as
    /// `TextureBuffer<R::TextureId>` boxed — the active renderer variant
    /// always matches (re-uploaded on renderer init and on change).
    pub shell_tex: Option<Box<dyn std::any::Any>>,
    /// Gesture-bar overlay texture (static).
    pub gesture_bar: Option<Box<dyn std::any::Any>>,
    /// Last minute rendered into the shell frame (re-raster on change).
    pub shell_minute: u64,
    /// Last status-bar/date reading (`shell.rs`, reused for 2 s).
    pub(crate) meta_cache: Option<(std::time::Instant, u64, de_ui::chrome::StatusInfo, String)>,
    /// Last touch position in logical coords (`TouchUp` carries none).
    pub last_touch_pos: smithay::utils::Point<f64, smithay::utils::Logical>,

    /// M1-T4 counters (observed from `render_frame` results).
    pub direct_scanout: u64,
    pub primary_composite: u64,

    // --- M3: launcher runtime ---
    /// Shell back-stack (which screen is up when `foreground == Shell`).
    pub nav: de_ui::nav::Nav,
    /// The active xdg popup grab (an open context menu), if any.
    pub(crate) popup_grab: Option<smithay::desktop::PopupGrab<De>>,
    /// The shell's open context menu, if any (`shell_menu.rs`).
    pub(crate) shell_menu: Option<crate::shell_menu::ShellMenu>,
    /// Finger-tracked home pager + icon rearrange (`home_ui.rs`).
    pub(crate) home_ui: crate::home_ui::HomeUi,
    /// Notification panel and quick actions (`panel_ui.rs`).
    pub(crate) panels: crate::panel_ui::PanelUi,
    pub(crate) navigation: crate::navigation::Navigation,
    /// Whether the screen shows a fullscreen app or a shell surface.
    pub foreground: Foreground,
    /// System-gesture recognizer (edge-back / home / recents / shade).
    pub gestures: de_ui::gestures::GestureTracker,
    /// The active touch is a shell gesture — never delivered to clients.
    pub touch_claimed: bool,
    /// True when the active slot-0 touch is bridged to `wl_pointer` for a
    /// client that cannot consume `wl_touch` — opt-in via the
    /// `DE_TOUCH_AS_POINTER` app_id denylist, off by default.
    pub touch_as_pointer: bool,
    /// Current home page index.
    pub home_page: usize,
    /// desktop-id → spawn → first toplevel bookkeeping + timeout.
    pub launch: crate::launch::LaunchTracker,
    pub(crate) morph: Option<crate::morph::WindowMorph>,
    pub(crate) motion_clock: de_paint::animation::AnimationClock,
    /// Last launch failure (shown by the error screen).
    pub launch_error: Option<String>,
    /// Scanned `.desktop` catalog.
    pub catalog: de_apps::catalog::Catalog,
    /// Hide/pin/dock + wallpaper prefs (`~/.config/rabbiz-de/prefs.json`).
    pub app_prefs: de_apps::prefs::AppPrefs,
    /// Active theme palette (resolved from prefs / `~/.config/rabbiz-de/theme`).
    pub(crate) theme: de_design::tokens::Theme,
    /// Shell i18n catalog (LANG-derived).
    pub strings: de_ui::i18n::Strings,
    /// Shell frame needs re-raster (nav/status/page/prefs changed).
    pub shell_dirty: bool,
    /// Latest rasterized shell frame (uploaded lazily by the frame pump).
    pub shell_px: Option<de_api::Pixels>,
    /// Cached wallpaper: `(id, pixels)` — reloaded when the id changes.
    pub wallpaper: Option<(String, de_api::Pixels)>,
    /// Icon bitmap cache by desktop-id (PNG decode + scale is not free).
    pub icon_cache: std::collections::HashMap<String, de_api::Pixels>,

    // --- session OSK lifecycle ---
    /// The spawned session keyboard (private-socket input method).
    pub(crate) osk: Option<crate::osk::OskProcess>,
    /// Earliest instant a respawn is allowed (backoff after fast deaths).
    pub(crate) osk_respawn_at: Option<Instant>,
    /// When the current child was spawned (drives the backoff).
    pub(crate) osk_spawned_at: Option<Instant>,
    /// Unix socket bridge for compositor-rendered search inputs (`Search`, `Drawer`).
    pub(crate) shell_im: crate::osk::ShellImBridge,

    /// Queued `copy_with_damage` screencopy requests awaiting a damaged frame.
    pub(crate) screencopy_pending: Vec<crate::screencopy::PendingCapture>,

    // --- M4: power + lock ---
    /// Power state machine: idle/dim/fade/sleep + cmd/fp watchers (`power.rs`).
    pub(crate) power: crate::power::Power,
    /// Lock overlay: raster + unlock-drag spring (`lock_ui.rs`).
    pub(crate) lock: crate::lock_ui::LockUi,
    /// Screenshot flash + toast (`shot_ui.rs`).
    pub(crate) shot_fx: crate::shot_ui::ShotFx,
    /// Transient volume / brightness OSD pill (`osd_ui.rs`).
    pub(crate) osd_fx: crate::osd_ui::OsdFx,
    /// Heads-up notification banner controller (`hun_ui.rs`).
    pub(crate) hun_fx: crate::hun_ui::HunFx,
    /// Last notification ID forwarded to `hun_fx` (prevents duplicate enqueue).
    pub(crate) last_hun_notif_id: u32,
    /// Pure app lifecycle & freeze-timer state machine (`de_system::lifecycle`).
    pub(crate) lifecycle: de_system::lifecycle::LifecycleManager,
    /// Output window insets (status/nav bars, cutout, active OSK height).
    pub insets: de_api::WindowInsets,
    /// Active display/input backend (`drm` vs `virtual`).
    pub(crate) backend: BackendSelect,
    /// Non-blocking runtime control IPC socket (`ctl_ipc.rs`).
    pub(crate) ctl_ipc: Option<crate::ctl_ipc::CtlIpc>,
    /// Non-blocking TCP emulator frame & control listener (`--emu-listen`).
    pub(crate) emu_server: Option<crate::ctl_ipc::EmuFrameServer>,
    /// Monotonic ID counter for synthetic mock notifications.
    pub(crate) next_mock_notif_id: u32,
    /// Cached battery status updated off-thread by the status poller.
    pub(crate) cached_battery: de_api::services::BatteryState,
    /// Cached privacy-indicator status updated off-thread by the status poller.
    pub(crate) cached_privacy: de_api::services::PrivacyState,
    /// `org.kde.StatusNotifierWatcher` D-Bus service for Desktop Mode tray icons.
    pub(crate) tray_watcher: de_system::tray::StatusNotifierWatcher,
    /// XWayland X11 window manager (`M5-T1` / Phase D3).
    pub(crate) xwm: Option<smithay::xwayland::X11Wm>,
    pub(crate) xwayland_shell_state: smithay::wayland::xwayland_shell::XWaylandShellState,
    /// Active XWayland `DISPLAY` number (when `Xwayland` is running).
    pub(crate) xwayland_display: Option<u32>,
    /// Active Desktop Mode window move/resize drag state.
    pub(crate) desktop_drag: Option<DesktopDrag>,
}

/// Active pointer drag on a Desktop Mode window (titlebar move or edge resize).
#[derive(Debug, Clone)]
pub(crate) enum DesktopDrag {
    Move {
        window: Window,
        start_ptr: (f64, f64),
        start_loc: (i32, i32),
    },
    Resize {
        window: Window,
        edge: de_ui::desktop_chrome::ResizeEdge,
        start_ptr: (f64, f64),
        start_geo: (i32, i32, i32, i32),
    },
}

/// What the screen is showing (M3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Foreground {
    /// A shell surface (home/drawer/recents/…) — app windows keep running
    /// but are not composited.
    Shell,
    /// A fullscreen app window owns the screen.
    App,
}

/// Errors during initialization.
#[derive(Debug)]
pub enum InitError {
    Session(SessionError),
    Render(RenderError),
    /// GL was required or selected but failed.
    GlRequired(String),
    Compositor(String),
}

impl std::fmt::Display for InitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Session(e) => write!(f, "{e}"),
            Self::Render(e) => write!(f, "{e}"),
            Self::GlRequired(e) => write!(f, "gl required: {e}"),
            Self::Compositor(e) => write!(f, "compositor: {e}"),
        }
    }
}

impl std::error::Error for InitError {}

impl From<SessionError> for InitError {
    fn from(e: SessionError) -> Self {
        Self::Session(e)
    }
}

/// Current free CMA in KiB (`/proc/meminfo`, 0 when unknown).
#[must_use]
pub fn cma_free_kb() -> u64 {
    std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("CmaFree"))
                .and_then(|l| l.split_whitespace().nth(1)?.parse().ok())
        })
        .unwrap_or(0)
}

/// Shell locale from `LANG` (falls back to English).
fn shell_locale() -> de_ui::i18n::Locale {
    let lang = std::env::var("LANG").unwrap_or_default();
    let code = lang.split(['.', '_']).next().unwrap_or("en");
    de_ui::i18n::Locale::from_code(code)
}

/// Catalog scan locales: Thai first when the shell is Thai.
pub(crate) fn shell_locales_vec() -> Vec<String> {
    match shell_locale() {
        de_ui::i18n::Locale::Th => vec!["th".into(), "en".into()],
        de_ui::i18n::Locale::En => vec!["en".into()],
    }
}

/// Current process RSS in KiB (`/proc/self/status VmRSS`).
#[must_use]
pub fn rss_kb() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmRSS"))
                .and_then(|l| l.split_whitespace().nth(1)?.parse().ok())
        })
        .unwrap_or(0)
}

/// Initialize session, card, output, compositor, renderer, brand texture
/// and the whole Wayland protocol state. `display` must be `Display::<De>`.
/// Fails before the first page flip ⇒ exit 75 territory.
#[allow(clippy::too_many_lines)] // sequential bring-up; splitting hides ordering
pub fn init(flags: &Flags, display: &mut Display<De>) -> Result<De, InitError> {
    if flags.backend == BackendSelect::Virtual {
        de_system::mock::enable_virtual_backend();
    }
    let profile = de_config::device_profile::DeviceProfile::load().unwrap_or_else(|error| {
        eprintln!("device profile: {error}; using handset timing");
        de_config::device_profile::DeviceProfile::default()
    });
    let form_factor =
        de_design::motion::FormFactor::from_profile_name(&profile.display.form_factor);
    let scheme = match std::env::var("RABBIZ_MOTION_SCHEME").as_deref() {
        Ok("expressive") => de_design::motion::MotionScheme::Expressive,
        _ => de_design::motion::MotionScheme::Standard,
    };
    let reduced_motion = matches!(
        std::env::var("RABBIZ_REDUCED_MOTION").as_deref(),
        Ok("1" | "true")
    );
    let motion_clock =
        de_paint::animation::AnimationClock::with_motion(scheme, reduced_motion, false)
            .for_form_factor(form_factor);
    let app_prefs = de_apps::prefs::AppPrefs::load();
    let start_locked = should_start_locked(&app_prefs.prefs().lock_method);
    let theme = if app_prefs.prefs().theme_mode.trim() == "dark" {
        de_design::tokens::DARK
    } else {
        de_design::theme::persisted_mode().theme()
    };

    let (session, seat_notifier, setup, mut renderer, compositor, output, render_node) = match flags
        .backend
    {
        BackendSelect::Drm => {
            let card = de_backend::drm::find_card()
                .ok_or_else(|| InitError::Compositor("no drm card".into()))?;
            let (mut session, seat_notifier) = de_backend::session::activate()?;
            let mut setup = de_backend::drm::open_drm(&mut session, &card)?;

            let drm_out = de_backend::drm::select_output(&mut setup.device)?
                .ok_or_else(|| InitError::Compositor("no connected connector".into()))?;

            // Renderer selection: explicit > auto (GL if it inits and CMA is healthy).
            let want_gl = match flags.renderer {
                RendererSelect::Gl => true,
                RendererSelect::Soft => false,
                RendererSelect::Auto => cma_free_kb() >= CMA_FLOOR_KB,
            };
            let mut renderer: Option<DeRenderer> = None;
            let mut gl_error: Option<String> = None;
            if want_gl && de_backend::drm::init_egl(&mut setup).is_ok() {
                let display_egl: EGLDisplay = setup.egl_display.clone().expect("egl init");
                match de_render::gl::create(&display_egl) {
                    Ok(r) => renderer = Some(DeRenderer::Gl(Box::new(r))),
                    Err(e) => gl_error = Some(e.to_string()),
                }
            } else if want_gl {
                gl_error = Some("egl init failed".to_string());
            }
            if renderer.is_none() {
                if flags.require_gl || flags.renderer == RendererSelect::Gl {
                    return Err(InitError::GlRequired(
                        gl_error.unwrap_or_else(|| "unavailable".into()),
                    ));
                }
                let r = de_render::soft::create()
                    .map_err(|e| InitError::Render(RenderError::Soft(format!("{e}"))))?;
                renderer = Some(DeRenderer::Soft(r));
            }
            let renderer = renderer.expect("renderer");

            let surface = setup
                .device
                .create_surface(drm_out.crtc, drm_out.mode, &[drm_out.connector])
                .map_err(|e| InitError::Compositor(format!("surface: {e}")))?;
            let planes = setup
                .device
                .planes(&drm_out.crtc)
                .map_err(|e| InitError::Compositor(format!("planes: {e}")))?;
            let (allocator, exporter) = de_backend::drm::allocators(&setup);
            let compositor = DeCompositor::new(
                smithay::output::OutputModeSource::Static {
                    size: smithay::utils::Size::from((drm_out.width as i32, drm_out.height as i32)),
                    scale: smithay::utils::Scale::from(1.0),
                    transform: smithay::utils::Transform::Normal,
                },
                surface,
                Some(planes),
                allocator,
                exporter,
                [
                    smithay::backend::allocator::Fourcc::Argb8888,
                    smithay::backend::allocator::Fourcc::Xrgb8888,
                ],
                renderer.renderer_formats(),
                smithay::utils::Size::<u32, smithay::utils::Buffer>::from((64, 64)),
                Some(setup.gbm.clone()),
            )
            .map_err(|e| InitError::Compositor(format!("{e:?}")))?;
            let render_node = setup
                .device
                .device_fd()
                .dev_id()
                .ok()
                .and_then(|id| smithay::backend::drm::DrmNode::from_dev_id(id).ok());
            let output = OutputGeometry::from(&drm_out);
            (
                Some(session),
                Some(seat_notifier),
                Some(setup),
                renderer,
                Some(compositor),
                output,
                render_node,
            )
        }
        BackendSelect::Virtual => {
            if flags.require_gl || flags.renderer == RendererSelect::Gl {
                return Err(InitError::GlRequired(
                    "virtual backend uses software renderer".into(),
                ));
            }
            let r = de_render::soft::create()
                .map_err(|e| InitError::Render(RenderError::Soft(format!("{e}"))))?;
            let output = OutputGeometry {
                connector_name: "VIRTUAL-1".to_string(),
                width: flags.virtual_size.0,
                height: flags.virtual_size.1,
                refresh_hz: flags.virtual_refresh_hz,
            };
            (None, None, None, DeRenderer::Soft(r), None, output, None)
        }
    };

    // Rasterize + upload the brand frame once (P0 → shown until the stub is
    // first composed).
    let px = de_ui::boot::brand_frame(
        de_api::Size::new(output.width as i32, output.height as i32),
        0.5,
    );
    let brand = de_render::upload(&mut renderer, &px).map_err(InitError::Render)?;

    // --- M1: Wayland globals + seat ---
    let dh = display.handle();
    let clock = Clock::new();

    let compositor_state = CompositorState::new::<De>(&dh);
    let shm_state = ShmState::new::<De>(&dh, de_protocols::shm_formats());
    let xdg_shell_state = XdgShellState::new::<De>(&dh);
    crate::privileged::init_policy();
    let layer_shell_state = WlrLayerShellState::new_with_filter::<De, _>(&dh, |c| {
        crate::privileged::allowed(c, de_config::Privileged::LayerShell)
    });
    TextInputManagerState::new::<De>(&dh);
    InputMethodManagerState::new::<De, _>(&dh, crate::osk::is_session_input_method);
    // Privileged globals are default-deny (docs/DE-PRIVILEGED-PROTOCOLS.md): the shipped tools
    // (wayvnc, grim, wl-clipboard, the session keyboard) are on the built-in list.
    VirtualKeyboardManagerState::new::<De, _>(&dh, |c| {
        crate::privileged::allowed(c, de_config::Privileged::VirtualKeyboard)
    });
    let xdg_decoration_state = XdgDecorationState::new::<De>(&dh);
    let xdg_activation_state = XdgActivationState::new::<De>(&dh);
    let data_device_state = DataDeviceState::new::<De>(&dh);
    let primary_selection_state = PrimarySelectionState::new::<De>(&dh);
    let data_control_state =
        DataControlState::new::<De, _>(&dh, Some(&primary_selection_state), |c| {
            crate::privileged::allowed(c, de_config::Privileged::DataControl)
        });
    let ext_data_control_state =
        ExtDataControlState::new::<De, _>(&dh, Some(&primary_selection_state), |c| {
            crate::privileged::allowed(c, de_config::Privileged::DataControl)
        });
    let viewporter_state = ViewporterState::new::<De>(&dh);
    let fractional_scale_state = FractionalScaleManagerState::new::<De>(&dh);
    let output_manager_state = OutputManagerState::new_with_xdg_output::<De>(&dh);
    let idle_inhibit_state = IdleInhibitManagerState::new::<De>(&dh);
    let relative_pointer_state = RelativePointerManagerState::new::<De>(&dh);
    let pointer_constraints_state = PointerConstraintsState::new::<De>(&dh);
    let session_lock_state = SessionLockManagerState::new::<De, _>(&dh, |c| {
        crate::privileged::allowed(c, de_config::Privileged::SessionLock)
    });
    crate::screencopy::init(&dh);
    crate::virtual_pointer::init(&dh);

    // Stock freedesktop globals (`protocols/standards.rs`) — each `new`
    // registers its global immediately; nothing here is privileged.
    let single_pixel_buffer_state = SinglePixelBufferState::new::<De>(&dh);
    let cursor_shape_state = CursorShapeManagerState::new::<De>(&dh);
    let xdg_foreign_state = XdgForeignState::new::<De>(&dh);
    let content_type_state = ContentTypeState::new::<De>(&dh);
    let xdg_toplevel_icon_manager = XdgToplevelIconManager::new::<De>(&dh);
    let xdg_toplevel_tag_manager = XdgToplevelTagManager::new::<De>(&dh);
    let kbd_shortcuts_inhibit_state = KeyboardShortcutsInhibitState::new::<De>(&dh);
    let system_bell_state = XdgSystemBellState::new::<De>(&dh);
    // `wp_presentation` clock id 1 = CLOCK_MONOTONIC (matches `Clock<Monotonic>`).
    let presentation_state = PresentationState::new::<De>(&dh, 1);
    let mut dmabuf_state = DmabufState::new();
    let dmabuf_global = render_node
        .and_then(de_protocols::dmabuf_feedback)
        .map(|fb| dmabuf_state.create_global_with_default_feedback::<De>(&dh, &fb));

    // Seat + capabilities (touch is the primary device on the handset).
    let seat_name = "seat0".to_string();
    let mut seat_state = SeatState::new();
    let mut seat = seat_state.new_wl_seat(&dh, seat_name.clone());
    let pointer = seat.add_pointer();
    // Handset contract: us,th layouts + Alt-Shift toggle (env.rs is the
    // authority; these fallbacks keep bare `rabbiz-de run` sane).
    let xkb = XkbConfig {
        layout: std::env::var("XKB_DEFAULT_LAYOUT")
            .unwrap_or_else(|_| "us,th".into())
            .leak(),
        options: Some(
            std::env::var("XKB_DEFAULT_OPTIONS").unwrap_or_else(|_| "grp:alt_shift_toggle".into()),
        ),
        ..XkbConfig::default()
    };
    seat.add_keyboard(xkb, 200, 25).expect("keyboard");
    let touch = seat.add_touch();

    // smithay Output object for globals + frame callbacks.
    let output_obj = Output::new(
        output.connector_name.clone(),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: Subpixel::Unknown,
            make: "RabbizOS".into(),
            model: output.connector_name.clone(),
        },
    );
    output_obj.create_global::<De>(&dh);
    output_obj.change_current_state(
        Some(Mode {
            size: (output.width as i32, output.height as i32).into(),
            refresh: (output.refresh_hz * 1000.0) as i32,
        }),
        None,
        None,
        Some((0, 0).into()),
    );

    let rotation = crate::rotation::Rotation::new(
        de_api::Size::new(output.width as i32, output.height as i32),
        crate::rotation::policy_from_prefs(app_prefs.prefs()),
    );
    let mut space = Space::default();
    space.map_output(&output_obj, (0, 0));

    // Output geometry for the gesture tracker (before `output` moves).
    let (out_w, out_h) = (output.width as i32, output.height as i32);

    Ok(De {
        session,
        seat_notifier,
        setup,
        compositor,
        renderer,
        brand,
        brand_px: px,
        output,
        started: Instant::now(),
        first_flip: false,

        display_handle: dh,
        socket_name: None,
        running: Arc::new(AtomicBool::new(true)),
        clock,

        space,
        popups: PopupManager::default(),
        popup_grab: None,
        shell_menu: None,
        home_ui: crate::home_ui::HomeUi::new(),
        panels: {
            let mut p = crate::panel_ui::PanelUi::new();
            p.pull.set_reach(de_ui::pull::reach_for(out_h));
            p
        },
        navigation: crate::navigation::Navigation::new(),
        output_obj,
        rotation,
        media: crate::media_ui::MediaFx::new(),

        compositor_state,
        shm_state,
        xdg_shell_state,
        layer_shell_state,
        exclusive_layer_focus: None,
        xdg_decoration_state,
        xdg_activation_state,
        data_device_state,
        primary_selection_state,
        data_control_state,
        ext_data_control_state,
        seat_state,
        viewporter_state,
        fractional_scale_state,
        output_manager_state,
        dmabuf_state,
        dmabuf_global,
        idle_inhibit_state,
        relative_pointer_state,
        pointer_constraints_state,
        session_lock_state,
        session_lock_tracker: crate::protocols::session_lock::SessionLockTracker::new(),
        single_pixel_buffer_state,
        cursor_shape_state,
        xdg_foreign_state,
        content_type_state,
        xdg_toplevel_icon_manager,
        xdg_toplevel_tag_manager,
        kbd_shortcuts_inhibit_state,
        system_bell_state,
        presentation_state,
        present_seq: 0,
        desktop_mode: form_factor == de_design::motion::FormFactor::Desktop,
        default_form_factor: form_factor,
        pre_desktop_rotation: None,
        idle_inhibitors: Vec::new(),
        screensaver: de_system::screensaver::ScreenSaver::start(),

        seat,
        seat_name,
        pointer,
        touch,
        pointer_active: false,
        hw_keyboard: false,
        hw_pointer: false,
        kbd_devices: std::collections::HashSet::new(),
        ptr_devices: std::collections::HashSet::new(),
        pointer_press: None,
        launcher_pending: false,
        cursor_px: None,
        cursor_tex: None,

        needs_present: true,
        shell_tex: None,
        gesture_bar: None,
        shell_minute: u64::MAX,
        meta_cache: None,
        last_touch_pos: (0.0, 0.0).into(),
        direct_scanout: 0,
        primary_composite: 0,

        nav: {
            let mut n = de_ui::nav::Nav::new();
            n.booted();
            n.unlock(); // → Home; lock/unlock lands with M4.
            n
        },
        foreground: Foreground::Shell,
        gestures: de_ui::gestures::GestureTracker::new({
            let mut c = de_ui::gestures::GestureConfig::a21s();
            c.output_w = out_w;
            c.output_h = out_h;
            c
        }),
        touch_claimed: false,
        touch_as_pointer: false,
        home_page: 0,
        launch: crate::launch::LaunchTracker::new(),
        morph: None,
        motion_clock,
        launch_error: None,
        catalog: de_apps::catalog::Catalog::scan(&shell_locales_vec()),
        app_prefs,
        theme,
        strings: de_ui::i18n::Strings::load(shell_locale()),
        shell_dirty: true,
        shell_px: None,
        wallpaper: None,
        icon_cache: std::collections::HashMap::new(),
        osk: None,
        osk_respawn_at: None,
        osk_spawned_at: None,
        shell_im: crate::osk::ShellImBridge::bind(),
        screencopy_pending: Vec::new(),
        power: crate::power::Power::default(),
        lock: {
            let mut lock = crate::lock_ui::LockUi::default();
            if start_locked {
                lock.open();
            }
            lock
        },
        shot_fx: crate::shot_ui::ShotFx::default(),
        osd_fx: crate::osd_ui::OsdFx::default(),
        hun_fx: crate::hun_ui::HunFx::default(),
        last_hun_notif_id: 0,
        lifecycle: de_system::lifecycle::LifecycleManager::default(),
        insets: de_api::WindowInsets {
            system_bars: de_api::EdgeInsets::new(
                de_ui::chrome::STATUS_H,
                0,
                de_design::tokens::GESTURE_BAR_HEIGHT,
                0,
            ),
            cutout: de_api::EdgeInsets::ZERO,
            ime: de_api::EdgeInsets::ZERO,
        },
        backend: flags.backend,
        ctl_ipc: crate::ctl_ipc::CtlIpc::bind_with_tcp(
            if flags.backend == BackendSelect::Virtual {
                flags.ctl_listen.as_deref()
            } else {
                None
            },
        ),
        emu_server: if flags.backend == BackendSelect::Virtual {
            flags
                .emu_listen
                .as_deref()
                .and_then(crate::ctl_ipc::EmuFrameServer::bind)
        } else {
            None
        },
        next_mock_notif_id: 10_000,
        cached_battery: de_api::services::BatteryState::default(),
        cached_privacy: de_api::services::PrivacyState::default(),
        tray_watcher: de_system::tray::StatusNotifierWatcher::start(),
        xwm: None,
        xwayland_shell_state: smithay::wayland::xwayland_shell::XWaylandShellState::new::<De>(&display.handle()),
        xwayland_display: None,
        desktop_drag: None,
    })
}

/// Whether the compositor must start on the lock screen before the first frame.
#[must_use]
pub(crate) fn should_start_locked(lock_method: &str) -> bool {
    lock_method.trim() != "swipe"
}

impl De {
    /// Active form factor name (`handset`, `tablet`, `desktop`).
    #[must_use]
    pub(crate) fn form_factor_name(&self) -> &'static str {
        match self.motion_clock.form_factor() {
            de_design::motion::FormFactor::Handset => "handset",
            de_design::motion::FormFactor::Tablet => "tablet",
            de_design::motion::FormFactor::Desktop => "desktop",
        }
    }

    /// Reconfigure all mapped xdg toplevels for the current mode (floating
    /// cascaded windows in Desktop Mode; fullscreen in Handset Mode).
    pub(crate) fn reconfigure_windows_for_mode(&mut self) {
        let logical = de_api::Size::new(self.output.width as i32, self.output.height as i32);
        let out_size =
            smithay::utils::Size::<i32, smithay::utils::Logical>::from((logical.w, logical.h));
        let frame = self.insets.immersive_frame(logical);
        let full_size =
            smithay::utils::Size::<i32, smithay::utils::Logical>::from((frame.w, frame.h));
        let windows: Vec<_> = self.space.elements().cloned().collect();
        for (idx, window) in windows.iter().enumerate() {
            if let Some(toplevel) = window.toplevel() {
                if self.desktop_mode {
                    let (x, y, win_size) = de_protocols::desktop_window_geometry(out_size, idx);
                    de_protocols::configure_floating(toplevel, win_size);
                    toplevel.send_configure();
                    self.space.map_element(window.clone(), (x, y), false);
                } else {
                    de_protocols::configure_fullscreen(toplevel, full_size);
                    toplevel.send_configure();
                    self.space.map_element(window.clone(), (0, 0), false);
                }
            }
        }
    }

    /// Toggle Desktop Mode: auto-rotates a portrait handset to landscape
    /// (`1600×720`) and switches windows between floating and fullscreen.
    pub(crate) fn set_desktop_mode(&mut self, enabled: bool) {
        if self.desktop_mode == enabled {
            return;
        }
        self.desktop_mode = enabled;
        let next_ff = if enabled {
            de_design::motion::FormFactor::Desktop
        } else {
            self.default_form_factor
        };
        self.motion_clock.set_form_factor(next_ff);

        if enabled {
            self.pre_desktop_rotation = Some((self.rotation.policy, self.rotation.current));
            if !self.rotation.current.is_landscape() {
                let _ = self.set_rotation_mode("landscape_ccw");
            } else {
                self.reconfigure_windows_for_mode();
            }
        } else if let Some((prev_policy, prev_orient)) = self.pre_desktop_rotation.take() {
            if self.rotation.current != prev_orient {
                let _ = self.set_rotation_mode(prev_orient.name());
            } else {
                self.reconfigure_windows_for_mode();
            }
            self.rotation.policy = prev_policy;
            if prev_policy.auto {
                self.reevaluate_orientation();
            }
        } else {
            self.reconfigure_windows_for_mode();
        }
        self.panels.invalidate();
        self.shell_dirty = true;
        self.needs_present = true;
    }

    #[must_use]
    pub fn kind(&self) -> RendererKind {
        self.renderer.kind()
    }

    /// `OutputInfo` snapshot for stats/reporting.
    #[must_use]
    pub fn output_info(&self) -> OutputInfo {
        OutputInfo {
            name: self.output.connector_name.clone(),
            width: self.output.width,
            height: self.output.height,
            refresh_hz: self.output.refresh_hz,
            scale: 1.0,
        }
    }

    /// Window whose (toplevel) surface is `surface`.
    #[must_use]
    pub fn window_for_surface(
        &self,
        surface: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
    ) -> Option<Window> {
        use smithay::wayland::seat::WaylandFocus;
        self.space
            .elements()
            .find(|w| w.wl_surface().is_some_and(|s| &*s == surface))
            .cloned()
    }

    /// Keep the keyboard LED state device-synced (no-op for the handset).
    pub fn update_led_state(&mut self, _leds: LedState) {}

    /// Whether an external `ext-session-lock-v1` lock (or fail-closed recovery) is active.
    #[must_use]
    pub fn is_session_locked(&self) -> bool {
        self.session_lock_tracker.is_locked()
    }

    /// Monotonic milliseconds elapsed since compositor startup.
    #[must_use]
    pub(crate) fn uptime_ms(&self, now: Instant) -> u64 {
        now.saturating_duration_since(self.started).as_millis() as u64
    }

    /// Record a newly mapped toplevel in the app lifecycle state machine.
    pub(crate) fn lifecycle_on_mapped(&mut self, app_id: &str) {
        if app_id.is_empty() {
            return;
        }
        let now_ms = self.uptime_ms(Instant::now());
        if matches!(
            self.lifecycle.state_of(app_id),
            None | Some(de_system::lifecycle::AppState::Dead)
        ) {
            let launch_actions = self.lifecycle.on_launch(app_id, now_ms);
            for action in launch_actions {
                eprintln!("rabbiz-de: lifecycle {action:?}");
            }
        }
        let actions = self.lifecycle.on_mapped(app_id, now_ms);
        for action in actions {
            eprintln!("rabbiz-de: lifecycle {action:?}");
        }
    }

    /// Record a destroyed toplevel in the app lifecycle state machine.
    pub(crate) fn lifecycle_on_exited(&mut self, app_id: &str) {
        if app_id.is_empty() {
            return;
        }
        self.lifecycle.on_exited(app_id);
        eprintln!("rabbiz-de: lifecycle exited app_id={app_id}");
    }

    /// Advance background freeze timers in the app lifecycle state machine.
    pub(crate) fn lifecycle_tick(&mut self, now_ms: u64) {
        let actions = self.lifecycle.tick(now_ms);
        for action in actions {
            eprintln!("rabbiz-de: lifecycle {action:?}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_start_locked_for_any_non_swipe_method() {
        assert!(!should_start_locked("swipe"));
        assert!(!should_start_locked(" swipe "));
        assert!(should_start_locked("pin"));
        assert!(should_start_locked("passphrase"));
        assert!(should_start_locked("biometric"));
        assert!(should_start_locked(""));
    }

    #[test]
    fn fresh_de_with_pin_starts_locked_before_any_input() {
        let mut lock = crate::lock_ui::LockUi::default();
        assert!(!lock.shown());
        if should_start_locked("pin") {
            lock.open();
        }
        assert!(lock.shown());
        assert!(!lock.is_unlocking());
    }
}
