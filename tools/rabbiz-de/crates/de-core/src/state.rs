//! `De` — the compositor's top-level state (M0: one output, one brand layer).

use std::time::Instant;

use de_api::{OutputInfo, RendererKind};
use de_backend::drm::{DrmSetup, OutputSelection};
use de_backend::probe::DeCompositor;
use de_backend::session::SessionError;
use de_config::flags::{Flags, RendererSelect};
use de_render::{DeRenderer, DeTexture, RenderError};
use smithay::backend::egl::EGLDisplay;
use smithay::backend::session::libseat::{LibSeatSession, LibSeatSessionNotifier};

/// Free CMA below which `auto` picks the software renderer (plan Decision 2).
const CMA_FLOOR_KB: u64 = 96 * 1024;

/// Everything `de-core` needs to present frames on the one output.
pub struct De {
    /// Kept alive for the session's lifetime; `notifier` is taken by the
    /// event loop once at start.
    #[allow(dead_code)]
    pub session: LibSeatSession,
    pub seat_notifier: Option<LibSeatSessionNotifier>,
    pub setup: DrmSetup,
    pub compositor: DeCompositor,
    pub renderer: DeRenderer,
    /// The uploaded brand frame texture.
    pub brand: DeTexture,
    pub output: OutputSelection,
    pub started: Instant,
    /// First successful page flip has happened (gates readiness + exit code).
    pub first_flip: bool,
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

/// Initialize the session, card, output, compositor, renderer and the brand
/// texture. Fails before the first page flip ⇒ exit 75 territory.
pub fn init(flags: &Flags) -> Result<De, InitError> {
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
        let display: EGLDisplay = setup.egl_display.clone().expect("egl init");
        match de_render::gl::create(&display) {
            Ok(r) => renderer = Some(DeRenderer::Gl(r)),
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

    // Rasterize + upload the brand frame once (P0).
    let px = de_ui::boot::brand_frame(
        de_api::Size::new(output.width as i32, output.height as i32),
        0.5,
    );
    let brand = de_render::upload(&mut renderer, &px).map_err(InitError::Render)?;

    Ok(De {
        session,
        seat_notifier: Some(seat_notifier),
        setup,
        compositor,
        renderer,
        brand,
        output,
        started: Instant::now(),
        first_flip: false,
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
}
