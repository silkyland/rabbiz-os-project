use std::os::unix::io::OwnedFd;

use serde::Serialize;
use smithay::backend::allocator::Fourcc as DrmFourcc;
use smithay::backend::allocator::gbm::{GbmAllocator, GbmBufferFlags, GbmDevice};
use smithay::backend::drm::compositor::DrmCompositor;
use smithay::backend::drm::exporter::gbm::GbmFramebufferExporter;
use smithay::backend::drm::{DrmDevice, DrmDeviceFd, DrmDeviceNotifier, DrmNode, DrmSurface};
use smithay::reexports::drm::control::{
    connector, crtc, plane, Device as ControlDevice, Mode, ModeTypeFlags,
};
use smithay::utils::{Buffer as BufferCoords, DeviceFd, Size};

pub type BackendDrmCompositor = DrmCompositor<
    GbmAllocator<DrmDeviceFd>,
    GbmFramebufferExporter<DrmDeviceFd>,
    (),
    DrmDeviceFd,
>;

#[derive(Debug, Clone, Serialize)]
pub struct PlaneInfo {
    pub name: String,
    #[serde(rename = "type")]
    pub plane_type: String,
    pub formats: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DrmInfo {
    pub connector: String,
    pub mode: String,
    pub refresh_hz: f64,
    pub atomic: bool,
    pub planes: Vec<PlaneInfo>,
    pub formats: Vec<String>,
}

pub struct DrmOutputSetup {
    pub info: DrmInfo,
    pub crtc: crtc::Handle,
    pub connector: connector::Handle,
    pub drm_mode: Mode,
    pub width: u32,
    pub height: u32,
    pub refresh_hz: f64,
    pub raw_formats: Vec<DrmFourcc>,
}

pub fn open_drm_device(
    fd: OwnedFd,
) -> Result<(DrmDevice, DrmDeviceNotifier, DrmDeviceFd), String> {
    let drm_fd = DrmDeviceFd::new(DeviceFd::from(fd));
    let (device, notifier) = DrmDevice::new(drm_fd.clone(), false)
        .map_err(|err| format!("DrmDevice::new failed: {err}"))?;
    Ok((device, notifier, drm_fd))
}

pub fn inspect_drm(drm: &DrmDevice) -> Result<DrmOutputSetup, String> {
    let res = drm
        .resource_handles()
        .map_err(|err| format!("drm resource_handles failed: {err}"))?;

    let mut chosen_conn = None;
    for &conn_handle in res.connectors() {
        let Ok(conn) = drm.get_connector(conn_handle, false) else {
            continue;
        };
        if conn.state() == connector::State::Connected && !conn.modes().is_empty() {
            chosen_conn = Some(conn);
            break;
        }
    }

    let conn = chosen_conn.ok_or_else(|| "no connected DRM connector with modes".to_string())?;
    let connector_name = format!("{}-{}", conn.interface().as_str(), conn.interface_id());

    let drm_mode = conn
        .modes()
        .iter()
        .find(|m| m.mode_type().contains(ModeTypeFlags::PREFERRED))
        .copied()
        .unwrap_or_else(|| conn.modes()[0]);

    let (w, h) = drm_mode.size();
    let width = u32::from(w);
    let height = u32::from(h);
    let refresh_hz = compute_refresh_hz(&drm_mode);
    let mode_str = format!("{width}x{height}");

    let crtc_handle = pick_crtc(drm, &res, &conn)?;

    let mut planes = Vec::new();
    let mut primary_formats = Vec::new();
    let mut primary_fourccs = Vec::new();

    if let Ok(plane_handles) = drm.plane_handles() {
        for (idx, &ph) in plane_handles.iter().enumerate() {
            let Ok(plane_info) = drm.get_plane(ph) else {
                continue;
            };
            let plane_type = read_plane_type(drm, ph).unwrap_or_else(|| "Unknown".to_string());
            let mut fmts = Vec::new();
            let mut fourccs = Vec::new();
            for &raw_fmt in plane_info.formats() {
                if let Ok(fourcc) = DrmFourcc::try_from(raw_fmt) {
                    let s = format_fourcc(fourcc);
                    if !fmts.contains(&s) {
                        fmts.push(s);
                    }
                    if !fourccs.contains(&fourcc) {
                        fourccs.push(fourcc);
                    }
                }
            }
            let name = match plane_type.as_str() {
                "Primary" => {
                    if primary_formats.is_empty() {
                        primary_formats = fmts.clone();
                        primary_fourccs = fourccs.clone();
                    }
                    "primary".to_string()
                }
                "Cursor" => "cursor".to_string(),
                _ => format!("overlay-{idx}"),
            };
            planes.push(PlaneInfo {
                name,
                plane_type,
                formats: fmts,
            });
        }
    }

    if primary_fourccs.is_empty() {
        primary_fourccs.push(DrmFourcc::Argb8888);
        primary_fourccs.push(DrmFourcc::Xrgb8888);
        primary_formats.push("ARGB8888".to_string());
        primary_formats.push("XRGB8888".to_string());
    }

    let atomic = drm.is_atomic();
    let info = DrmInfo {
        connector: connector_name,
        mode: mode_str,
        refresh_hz,
        atomic,
        planes,
        formats: primary_formats,
    };

    Ok(DrmOutputSetup {
        info,
        crtc: crtc_handle,
        connector: conn.handle(),
        drm_mode,
        width,
        height,
        refresh_hz,
        raw_formats: primary_fourccs,
    })
}

fn compute_refresh_hz(mode: &Mode) -> f64 {
    let htotal = f64::from(mode.hsync().2);
    let vtotal = f64::from(mode.vsync().2);
    if htotal > 0.0 && vtotal > 0.0 {
        let hz = (f64::from(mode.clock()) * 1000.0) / (htotal * vtotal);
        (hz * 100.0).round() / 100.0
    } else {
        f64::from(mode.vrefresh())
    }
}

fn pick_crtc(
    drm: &DrmDevice,
    res: &smithay::reexports::drm::control::ResourceHandles,
    conn: &connector::Info,
) -> Result<crtc::Handle, String> {
    if let Some(enc_handle) = conn.current_encoder() {
        if let Ok(enc) = drm.get_encoder(enc_handle) {
            if let Some(crtc) = enc.crtc() {
                return Ok(crtc);
            }
        }
    }
    for &enc_handle in conn.encoders() {
        if let Ok(enc) = drm.get_encoder(enc_handle) {
            for crtc in res.filter_crtcs(enc.possible_crtcs()) {
                return Ok(crtc);
            }
        }
    }
    res.crtcs()
        .first()
        .copied()
        .ok_or_else(|| "no CRTC available for connector".to_string())
}

fn read_plane_type(drm: &DrmDevice, plane_handle: plane::Handle) -> Option<String> {
    let props = drm.get_properties(plane_handle).ok()?;
    for (&prop_handle, &val) in props.iter() {
        let Ok(prop) = drm.get_property(prop_handle) else {
            continue;
        };
        if prop.name().to_str() == Ok("type") {
            return match val {
                0 => Some("Overlay".to_string()),
                1 => Some("Primary".to_string()),
                2 => Some("Cursor".to_string()),
                _ => Some("Unknown".to_string()),
            };
        }
    }
    None
}

pub fn format_fourcc(fourcc: DrmFourcc) -> String {
    format!("{fourcc:?}").to_uppercase()
}

pub fn create_gbm_device(drm_fd: DrmDeviceFd) -> Result<GbmDevice<DrmDeviceFd>, String> {
    GbmDevice::new(drm_fd).map_err(|err| format!("GbmDevice::new failed: {err}"))
}

pub fn create_surface_and_compositor(
    drm: &mut DrmDevice,
    drm_fd: &DrmDeviceFd,
    gbm: GbmDevice<DrmDeviceFd>,
    setup: &DrmOutputSetup,
    renderer_formats: Vec<smithay::backend::allocator::Format>,
) -> Result<BackendDrmCompositor, String> {
    let surface: DrmSurface = drm
        .create_surface(setup.crtc, setup.drm_mode, &[setup.connector])
        .map_err(|err| format!("DrmDevice::create_surface failed: {err}"))?;

    let allocator = GbmAllocator::new(
        gbm.clone(),
        GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT,
    );
    let node = DrmNode::from_file(drm_fd).ok();
    let exporter = GbmFramebufferExporter::new(gbm.clone(), node);
    let mode = setup.drm_mode;
    let color_formats = [DrmFourcc::Argb8888, DrmFourcc::Xrgb8888];
    let cursor_size: Size<u32, BufferCoords> = (64, 64).into();

    DrmCompositor::new(
        smithay::output::OutputModeSource::Static {
            size: (setup.width as i32, setup.height as i32).into(),
            scale: 1.0.into(),
            transform: smithay::utils::Transform::Normal,
        },
        surface,
        None,
        allocator,
        exporter,
        color_formats,
        renderer_formats,
        cursor_size,
        Some(gbm),
    )
    .map_err(|err| format!("DrmCompositor::new(mode={:?}) failed: {err:?}", mode.name()))
}
