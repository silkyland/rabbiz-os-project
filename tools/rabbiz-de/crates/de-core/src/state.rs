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
use de_backend::drm::{DrmSetup, OutputSelection};
use de_backend::probe::DeCompositor;
use de_backend::session::SessionError;
use de_config::flags::{Flags, RendererSelect};
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
use smithay::wayland::dmabuf::{DmabufGlobal, DmabufState};
use smithay::wayland::input_method::InputMethodManagerState;
use smithay::wayland::output::OutputManagerState;
use smithay::wayland::selection::data_device::DataDeviceState;
use smithay::wayland::selection::ext_data_control::DataControlState as ExtDataControlState;
use smithay::wayland::selection::primary_selection::PrimarySelectionState;
use smithay::wayland::selection::wlr_data_control::DataControlState;
use smithay::wayland::shell::wlr_layer::WlrLayerShellState;
use smithay::wayland::shell::xdg::decoration::XdgDecorationState;
use smithay::wayland::shell::xdg::XdgShellState;
use smithay::wayland::shm::ShmState;
use smithay::wayland::text_input::TextInputManagerState;
use smithay::wayland::viewporter::ViewporterState;
use smithay::wayland::virtual_keyboard::VirtualKeyboardManagerState;
use smithay::wayland::xdg_activation::XdgActivationState;

/// Free CMA below which `auto` picks the software renderer (plan Decision 2).
const CMA_FLOOR_KB: u64 = 96 * 1024;

/// Per-client state attached to every Wayland client (needed by
/// `CompositorHandler::client_compositor_state`).
#[derive(Debug, Default)]
pub struct DeClientState {
    pub compositor_state: CompositorClientState,
    pub(crate) session_input_method: bool,
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
    /// Kept alive for the session's lifetime.
    #[allow(dead_code)]
    pub session: LibSeatSession,
    pub seat_notifier: Option<LibSeatSessionNotifier>,
    pub setup: DrmSetup,
    pub compositor: DeCompositor,
    pub renderer: DeRenderer,
    /// The uploaded brand frame texture (validates the renderer at init).
    pub brand: DeTexture,
    /// Source pixels — re-uploaded when the home stub needs damage.
    pub brand_px: de_api::Pixels,
    pub output: OutputSelection,
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
    pub output_manager_state: OutputManagerState,
    pub dmabuf_state: DmabufState,
    pub dmabuf_global: Option<DmabufGlobal>,

    pub seat: Seat<De>,
    pub seat_name: String,
    pub pointer: PointerHandle<De>,
    pub touch: TouchHandle<De>,

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
    /// Whether the screen shows a fullscreen app or a shell surface.
    pub foreground: Foreground,
    /// System-gesture recognizer (edge-back / home / recents / shade).
    pub gestures: de_ui::gestures::GestureTracker,
    /// The active touch is a shell gesture — never delivered to clients.
    pub touch_claimed: bool,
    /// Current home page index.
    pub home_page: usize,
    /// desktop-id → spawn → first toplevel bookkeeping + timeout.
    pub launch: crate::launch::LaunchTracker,
    pub(crate) morph: Option<crate::morph::WindowMorph>,
    pub(crate) motion_clock: de_paint::animation::AnimationClock,
    /// Last launch failure (shown by the error screen).
    pub launch_error: Option<String>,
    /// Last shm frame per app for the recents rail (LRU).
    pub snapshots: crate::snapshots::SnapshotCache,
    /// Scanned `.desktop` catalog.
    pub catalog: de_apps::catalog::Catalog,
    /// Hide/pin/dock + wallpaper prefs (`~/.config/rabbiz-de/prefs.json`).
    pub app_prefs: de_apps::prefs::AppPrefs,
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

    /// Queued `copy_with_damage` screencopy requests awaiting a damaged frame.
    pub(crate) screencopy_pending: Vec<crate::screencopy::PendingCapture>,
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
fn shell_locales() -> Vec<String> {
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
    let profile = de_config::device_profile::DeviceProfile::load().unwrap_or_else(|error| {
        eprintln!("device profile: {error}; using handset timing");
        de_config::device_profile::DeviceProfile::default()
    });
    let form_factor =
        de_design::motion::FormFactor::from_profile_name(&profile.display.form_factor);
    let motion_clock = de_paint::animation::AnimationClock::new().for_form_factor(form_factor);
    let card =
        de_backend::drm::find_card().ok_or_else(|| InitError::Compositor("no drm card".into()))?;
    let (mut session, seat_notifier) = de_backend::session::activate()?;
    let mut setup = de_backend::drm::open_drm(&mut session, &card)?;

    let output = de_backend::drm::select_output(&mut setup.device)?
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
    let mut renderer = renderer.expect("renderer");

    let surface = setup
        .device
        .create_surface(output.crtc, output.mode, &[output.connector])
        .map_err(|e| InitError::Compositor(format!("surface: {e}")))?;
    let planes = setup
        .device
        .planes(&output.crtc)
        .map_err(|e| InitError::Compositor(format!("planes: {e}")))?;
    let (allocator, exporter) = de_backend::drm::allocators(&setup);
    let compositor = DeCompositor::new(
        smithay::output::OutputModeSource::Static {
            size: smithay::utils::Size::from((output.width as i32, output.height as i32)),
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
    let output_manager_state = OutputManagerState::new_with_xdg_output::<De>(&dh);
    crate::screencopy::init(&dh);
    crate::virtual_pointer::init(&dh);
    let mut dmabuf_state = DmabufState::new();
    let render_node = setup
        .device
        .device_fd()
        .dev_id()
        .ok()
        .and_then(|id| smithay::backend::drm::DrmNode::from_dev_id(id).ok());
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

    let mut space = Space::default();
    space.map_output(&output_obj, (0, 0));

    // Output geometry for the gesture tracker (before `output` moves).
    let (out_w, out_h) = (output.width as i32, output.height as i32);

    Ok(De {
        session,
        seat_notifier: Some(seat_notifier),
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
        output_obj,

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
        output_manager_state,
        dmabuf_state,
        dmabuf_global,

        seat,
        seat_name,
        pointer,
        touch,

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
        home_page: 0,
        launch: crate::launch::LaunchTracker::new(),
        morph: None,
        motion_clock,
        launch_error: None,
        snapshots: crate::snapshots::SnapshotCache::default(),
        catalog: de_apps::catalog::Catalog::scan(&shell_locales()),
        app_prefs: de_apps::prefs::AppPrefs::load(),
        strings: de_ui::i18n::Strings::load(shell_locale()),
        shell_dirty: true,
        shell_px: None,
        wallpaper: None,
        icon_cache: std::collections::HashMap::new(),
        osk: None,
        osk_respawn_at: None,
        osk_spawned_at: None,
        screencopy_pending: Vec::new(),
    })
}

impl De {
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
}
