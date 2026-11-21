//! Non-blocking Unix-socket + optional TCP runtime control server and emulator
//! frame bridge (`docs/DE-IPC.md`, Tier 1.6 + Wave S2/S6).
//!
//! Binds `$XDG_RUNTIME_DIR/rabbiz-de-ctl.sock` (`0600`) and optionally
//! `--ctl-listen` / `--emu-listen` TCP listeners (restricted to
//! `--backend=virtual`), polling accepted connections synchronously inside
//! `de-core`'s event loop.

use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use de_api::Size;
use de_config::ipc::{
    decode_request, encode_response, socket_path, InsetSummary, IpcCommand, IpcResponse, PanelKind,
    ShellStateSnapshot, TouchInjectPhase, WindowSummary, MAX_MESSAGE_BYTES,
};
use de_ui::hun::{HunCandidate, HunContext, HunPriority};

use crate::hun_ui::HunBannerData;
use crate::state::{De, Foreground};

/// Bound non-blocking Unix (+ optional TCP) listener for runtime `rabbiz-de ctl` requests.
pub(crate) struct CtlIpc {
    listener: UnixListener,
    path: PathBuf,
    tcp_listener: Option<TcpListener>,
}

impl CtlIpc {
    /// Bind the user-scoped control socket (`0600`), replacing any stale socket file.
    #[allow(dead_code)]
    pub(crate) fn bind() -> Option<Self> {
        Self::bind_with_tcp(None)
    }

    /// Bind the user-scoped Unix socket plus an optional TCP control listener.
    pub(crate) fn bind_with_tcp(tcp_addr: Option<&str>) -> Option<Self> {
        Self::bind_at_with_tcp(socket_path(), tcp_addr)
    }

    #[cfg(test)]
    pub(crate) fn bind_at(path: PathBuf) -> Option<Self> {
        Self::bind_at_with_tcp(path, None)
    }

    pub(crate) fn bind_at_with_tcp(path: PathBuf, tcp_addr: Option<&str>) -> Option<Self> {
        let _ = std::fs::remove_file(&path);
        let listener = match UnixListener::bind(&path) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("rabbiz-de: ctl ipc bind {} failed: {e}", path.display());
                return None;
            }
        };
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        if let Err(e) = listener.set_nonblocking(true) {
            eprintln!("rabbiz-de: ctl ipc nonblocking failed: {e}");
            let _ = std::fs::remove_file(&path);
            return None;
        }
        let tcp_listener = tcp_addr.and_then(|addr| match TcpListener::bind(addr) {
            Ok(l) => {
                if let Err(e) = l.set_nonblocking(true) {
                    eprintln!("rabbiz-de: ctl tcp nonblocking {addr} failed: {e}");
                    None
                } else {
                    Some(l)
                }
            }
            Err(e) => {
                eprintln!("rabbiz-de: ctl tcp bind {addr} failed: {e}");
                None
            }
        });
        Some(Self {
            listener,
            path,
            tcp_listener,
        })
    }

    fn try_accept(&self) -> Option<UnixStream> {
        match self.listener.accept() {
            Ok((stream, _)) => Some(stream),
            Err(e) if e.kind() == ErrorKind::WouldBlock => None,
            Err(e) => {
                eprintln!("rabbiz-de: ctl ipc accept error: {e}");
                None
            }
        }
    }

    fn try_accept_tcp(&self) -> Option<TcpStream> {
        let listener = self.tcp_listener.as_ref()?;
        match listener.accept() {
            Ok((stream, _)) => Some(stream),
            Err(e) if e.kind() == ErrorKind::WouldBlock => None,
            Err(e) => {
                eprintln!("rabbiz-de: ctl tcp accept error: {e}");
                None
            }
        }
    }
}

impl Drop for CtlIpc {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Persistent TCP viewer/control connection on `--emu-listen`.
struct EmuClient {
    stream: TcpStream,
    rx_buf: Vec<u8>,
}

/// Non-blocking TCP server for `--emu-listen` speaking `"HELLO\n"`, `"FRAME\n"`
/// (length-prefixed PNG frames), `"RAW\n"`, `"RAW_IF_NEWER <seq>\n"`, and
/// line-delimited JSON [`IpcRequest`]s.
pub(crate) struct EmuFrameServer {
    listener: TcpListener,
    clients: Vec<EmuClient>,
    seq: u64,
    dirty: bool,
    cached_px: Option<de_api::Pixels>,
}

impl EmuFrameServer {
    pub(crate) fn bind(addr: &str) -> Option<Self> {
        let listener = match TcpListener::bind(addr) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("rabbiz-de: emu-listen bind {addr} failed: {e}");
                return None;
            }
        };
        if let Err(e) = listener.set_nonblocking(true) {
            eprintln!("rabbiz-de: emu-listen nonblocking {addr} failed: {e}");
            return None;
        }
        Some(Self {
            listener,
            clients: Vec::new(),
            seq: 1,
            dirty: true,
            cached_px: None,
        })
    }

    fn accept_pending(&mut self) {
        for _ in 0..4 {
            match self.listener.accept() {
                Ok((stream, _)) => {
                    let _ = stream.set_nonblocking(true);
                    let _ = stream.set_nodelay(true);
                    self.clients.push(EmuClient {
                        stream,
                        rx_buf: Vec::with_capacity(512),
                    });
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) => {
                    eprintln!("rabbiz-de: emu-listen accept error: {e}");
                    break;
                }
            }
        }
    }
}

fn write_emu_bytes(stream: &mut TcpStream, bytes: &[u8]) -> bool {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_write_timeout(Some(Duration::from_millis(500)));
    let ok = stream.write_all(bytes).is_ok() && stream.flush().is_ok();
    let _ = stream.set_nonblocking(true);
    ok
}

impl De {
    /// Drain pending non-blocking control requests (at most 8 per tick to bound latency).
    pub(crate) fn poll_ctl_ipc(&mut self) {
        let mut accepted_unix = Vec::new();
        let mut accepted_tcp = Vec::new();
        if let Some(ipc) = self.ctl_ipc.as_ref() {
            for _ in 0..8 {
                let Some(stream) = ipc.try_accept() else {
                    break;
                };
                accepted_unix.push(stream);
            }
            for _ in 0..8 {
                let Some(stream) = ipc.try_accept_tcp() else {
                    break;
                };
                accepted_tcp.push(stream);
            }
        }
        for stream in accepted_unix {
            self.handle_ctl_stream(stream);
        }
        for stream in accepted_tcp {
            self.handle_ctl_tcp_stream(stream);
        }
    }

    fn handle_ctl_stream(&mut self, mut stream: UnixStream) {
        let _ = stream.set_nonblocking(false);
        let timeout = Some(Duration::from_millis(250));
        let _ = stream.set_read_timeout(timeout);
        let _ = stream.set_write_timeout(timeout);

        let Ok(reader_stream) = stream.try_clone() else {
            return;
        };
        let mut reader = BufReader::new(reader_stream.take((MAX_MESSAGE_BYTES + 1) as u64));
        let mut line = String::new();
        if reader.read_line(&mut line).is_err() || line.is_empty() {
            return;
        }

        let resp = match decode_request(&line) {
            Ok(req) => self.dispatch_ctl(req.id, req.command),
            Err(err) => IpcResponse::err(0, err),
        };
        if let Ok(encoded) = encode_response(&resp) {
            let _ = stream.write_all(encoded.as_bytes());
        }
    }

    fn handle_ctl_tcp_stream(&mut self, mut stream: TcpStream) {
        let _ = stream.set_nonblocking(false);
        let timeout = Some(Duration::from_millis(250));
        let _ = stream.set_read_timeout(timeout);
        let _ = stream.set_write_timeout(timeout);

        let Ok(reader_stream) = stream.try_clone() else {
            return;
        };
        let mut reader = BufReader::new(reader_stream.take((MAX_MESSAGE_BYTES + 1) as u64));
        let mut line = String::new();
        if reader.read_line(&mut line).is_err() || line.is_empty() {
            return;
        }

        let resp = match decode_request(&line) {
            Ok(req) => self.dispatch_ctl(req.id, req.command),
            Err(err) => IpcResponse::err(0, err),
        };
        if let Ok(encoded) = encode_response(&resp) {
            let _ = stream.write_all(encoded.as_bytes());
        }
    }

    /// Poll the `--emu-listen` TCP bridge for `"HELLO\n"`, `"FRAME\n"`, and JSON IPC lines.
    pub(crate) fn poll_emu_server(&mut self) {
        let Some(mut server) = self.emu_server.take() else {
            return;
        };
        server.accept_pending();

        let mut i = 0;
        while i < server.clients.len() {
            let mut disconnected = false;
            let mut tmp = [0u8; 4096];
            loop {
                match server.clients[i].stream.read(&mut tmp) {
                    Ok(0) => {
                        disconnected = true;
                        break;
                    }
                    Ok(n) => {
                        if server.clients[i].rx_buf.len() + n <= MAX_MESSAGE_BYTES * 2 {
                            server.clients[i].rx_buf.extend_from_slice(&tmp[..n]);
                        } else {
                            disconnected = true;
                            break;
                        }
                    }
                    Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                    Err(_) => {
                        disconnected = true;
                        break;
                    }
                }
            }

            while !disconnected {
                let Some(nl_pos) = server.clients[i].rx_buf.iter().position(|&b| b == b'\n') else {
                    break;
                };
                let raw_line: Vec<u8> = server.clients[i].rx_buf.drain(..=nl_pos).collect();
                let Ok(line_str) = std::str::from_utf8(&raw_line) else {
                    disconnected = true;
                    break;
                };
                let cmd = line_str.trim();
                if cmd.is_empty() {
                    continue;
                }
                if cmd.eq_ignore_ascii_case("HELLO") {
                    let reply = format!(
                        "HELLO {} {} {}\n",
                        self.output.width,
                        self.output.height,
                        self.output.refresh_hz.round() as u32
                    );
                    if !write_emu_bytes(&mut server.clients[i].stream, reply.as_bytes()) {
                        disconnected = true;
                    }
                } else if cmd.eq_ignore_ascii_case("FRAME") {
                    let png_frame = self.encode_emu_png_frame();
                    server.seq = server.seq.wrapping_add(1);
                    let header = format!(
                        "FRAME {} {} {} {}\n",
                        server.seq,
                        self.output.width,
                        self.output.height,
                        png_frame.len()
                    );
                    if !write_emu_bytes(&mut server.clients[i].stream, header.as_bytes())
                        || !write_emu_bytes(&mut server.clients[i].stream, &png_frame)
                    {
                        disconnected = true;
                    }
                } else if cmd.eq_ignore_ascii_case("RAW") {
                    if server.dirty || server.cached_px.is_none() {
                        server.cached_px = Some(self.composite_emu_pixels());
                        server.dirty = false;
                    }
                    if let Some(px) = server.cached_px.as_ref() {
                        let header = format!(
                            "RAW {} {} {} {}\n",
                            server.seq,
                            px.size.w,
                            px.size.h,
                            px.data.len()
                        );
                        if !write_emu_bytes(&mut server.clients[i].stream, header.as_bytes())
                            || !write_emu_bytes(&mut server.clients[i].stream, &px.data)
                        {
                            disconnected = true;
                        }
                    }
                } else if let Some(rest) = cmd
                    .strip_prefix("RAW_IF_NEWER ")
                    .or_else(|| cmd.strip_prefix("raw_if_newer "))
                {
                    let client_seq = rest.trim().parse::<u64>().unwrap_or(0);
                    if self.needs_present || self.shell_dirty {
                        server.dirty = true;
                        server.seq = server.seq.wrapping_add(1);
                    }
                    if !server.dirty && server.cached_px.is_some() && client_seq == server.seq {
                        let reply = format!("UNCHANGED {}\n", server.seq);
                        if !write_emu_bytes(&mut server.clients[i].stream, reply.as_bytes()) {
                            disconnected = true;
                        }
                    } else {
                        if server.dirty || server.cached_px.is_none() {
                            server.cached_px = Some(self.composite_emu_pixels());
                            server.dirty = false;
                        }
                        if let Some(px) = server.cached_px.as_ref() {
                            let header = format!(
                                "RAW {} {} {} {}\n",
                                server.seq,
                                px.size.w,
                                px.size.h,
                                px.data.len()
                            );
                            if !write_emu_bytes(&mut server.clients[i].stream, header.as_bytes())
                                || !write_emu_bytes(&mut server.clients[i].stream, &px.data)
                            {
                                disconnected = true;
                            }
                        }
                    }
                } else {
                    let resp = match decode_request(cmd) {
                        Ok(req) => {
                            let r = self.dispatch_ctl(req.id, req.command);
                            server.dirty = true;
                            server.seq = server.seq.wrapping_add(1);
                            r
                        }
                        Err(err) => IpcResponse::err(0, err),
                    };
                    if let Ok(encoded) = encode_response(&resp) {
                        if !write_emu_bytes(&mut server.clients[i].stream, encoded.as_bytes()) {
                            disconnected = true;
                        }
                    }
                }
            }

            if disconnected {
                server.clients.swap_remove(i);
            } else {
                i += 1;
            }
        }

        self.emu_server = Some(server);
    }

    /// Service any pending frame/control requests on `--emu-listen` right after virtual present.
    pub(crate) fn broadcast_emu_frame(&mut self) {
        if let Some(server) = self.emu_server.as_mut() {
            server.dirty = true;
            server.seq = server.seq.wrapping_add(1);
        }
        if self.emu_server.is_some() {
            self.poll_emu_server();
        }
    }

    /// Frame size changed (rotation): the next viewer poll must re-encode.
    pub(crate) fn on_emu_resize(&mut self) {
        if let Some(server) = self.emu_server.as_mut() {
            server.cached_px = None;
            server.dirty = true;
            server.seq = server.seq.wrapping_add(1);
        }
    }

    fn composite_emu_pixels(&mut self) -> de_api::Pixels {
        let now = Instant::now();
        let size = Size::new(self.output.width as i32, self.output.height as i32);
        if self.shell_px.is_none() || self.shell_dirty {
            self.shell_px = Some(self.shell_pixels());
            self.shell_dirty = false;
            self.shell_tex = None;
        }
        self.lock_prepare(size);
        self.menu_prepare(size);
        self.osd_prepare(now);
        self.hun_prepare(size);
        crate::composite::composite_output(self).unwrap_or_else(|| self.brand_px.clone())
    }

    fn encode_emu_png_frame(&mut self) -> Vec<u8> {
        let px = self.composite_emu_pixels();
        de_paint::encode_png(&px).unwrap_or_default()
    }

    /// Drain pending `de_system::mock` notifications and apply mocked system state.
    pub(crate) fn poll_mock_system(&mut self) {
        let now = Instant::now();
        let now_ms = self.uptime_ms(now);
        let drained = de_system::mock::drain_notifications();
        if !drained.is_empty() {
            let ctx = HunContext {
                dnd: self.app_prefs.prefs().do_not_disturb,
                shade_open: self.panels.shown(),
                locked: self.locked() || self.is_session_locked(),
            };
            for notif in drained {
                let id = self.next_mock_notif_id;
                self.next_mock_notif_id = self.next_mock_notif_id.wrapping_add(1);
                let candidate = HunCandidate {
                    id,
                    priority: HunPriority::Normal,
                    is_incoming_call: false,
                };
                let data = HunBannerData {
                    id,
                    app: notif.app.clone(),
                    title: notif.title.clone(),
                    body: notif.body.clone(),
                    actions: Vec::new(),
                    critical: false,
                };
                self.hun_fx.enqueue(candidate, data, ctx, now_ms);
                self.panels.notifications.insert(
                    0,
                    de_ui::panels::Notification {
                        id,
                        app: notif.app,
                        title: notif.title,
                        body: notif.body,
                        when: "now".to_string(),
                        icon: None,
                    },
                );
            }
            self.lock.stale = true;
            self.meta_cache = None;
            self.shell_dirty = true;
            self.needs_present = true;
        }

        if let Some(fp_ok) = de_system::mock::take_fingerprint() {
            if fp_ok {
                if self.locked() || self.sleeping() {
                    self.power_wake();
                    self.lock.unlock_now();
                    self.shell_dirty = true;
                    self.needs_present = true;
                }
            } else if self.locked() {
                self.lock.fail_shake(now);
                self.needs_present = true;
            }
        }

        let m = de_system::mock::snapshot();
        if m.enabled {
            let mut dirty = false;
            if let Some(w) = m.wifi_enabled {
                if self.panels.control_state.wifi != Some(w) {
                    self.panels.control_state.wifi = Some(w);
                    dirty = true;
                }
            }
            if let Some(bt) = m.bluetooth_enabled {
                if self.panels.control_state.bluetooth != Some(bt) {
                    self.panels.control_state.bluetooth = Some(bt);
                    dirty = true;
                }
            }
            if let Some(ap) = m.airplane {
                if self.panels.control_state.airplane != Some(ap) {
                    self.panels.control_state.airplane = Some(ap);
                    dirty = true;
                }
            }
            if let Some(dnd) = m.dnd {
                if self.app_prefs.prefs().do_not_disturb != dnd {
                    let _ = self.app_prefs.set_do_not_disturb(dnd);
                    dirty = true;
                }
            }
            if let Some(b) = m.brightness_pct {
                let f = f32::from(b) / 100.0;
                if (self.panels.brightness - f).abs() > 0.005 {
                    self.panels.brightness = f;
                    self.panels.control_state.brightness = Some(f);
                    dirty = true;
                }
            }
            if let Some(v) = m.volume_pct {
                let f = f32::from(v) / 100.0;
                if (self.panels.volume - f).abs() > 0.005 {
                    self.panels.volume = f;
                    self.panels.control_state.volume = Some(f);
                    dirty = true;
                }
            }
            if dirty {
                self.meta_cache = None;
                self.lock.stale = true;
                self.shell_dirty = true;
                self.needs_present = true;
            }
        }
    }

    fn dispatch_ctl(&mut self, id: u64, cmd: IpcCommand) -> IpcResponse {
        if (self.locked() || self.is_session_locked()) && !command_allowed_while_locked(&cmd) {
            return IpcResponse::err(id, "device_locked");
        }
        match cmd {
            IpcCommand::GetState => {
                let snap = self.ctl_state_snapshot();
                match serde_json::to_value(snap) {
                    Ok(v) => IpcResponse::ok(id, Some(v)),
                    Err(e) => IpcResponse::err(id, format!("serialize state: {e}")),
                }
            }
            IpcCommand::Lock => {
                self.power_lock();
                self.needs_present = true;
                IpcResponse::ok(id, None)
            }
            IpcCommand::Launch { desktop_id } => match self.launch.begin(&desktop_id) {
                Ok(pid) => {
                    self.needs_present = true;
                    IpcResponse::ok(id, Some(serde_json::json!({ "pid": pid })))
                }
                Err(e) => IpcResponse::err(id, e),
            },
            IpcCommand::ReloadTheme => {
                self.app_prefs = de_apps::prefs::AppPrefs::load();
                self.theme = if self.app_prefs.prefs().theme_mode.trim() == "dark" {
                    de_design::tokens::DARK
                } else {
                    de_design::theme::persisted_mode().theme()
                };
                self.osd_fx.mark_stale();
                self.hun_fx.mark_stale();
                self.gesture_bar = None;
                self.shell_dirty = true;
                self.needs_present = true;
                IpcResponse::ok(id, None)
            }
            IpcCommand::ReloadApps => {
                self.catalog = de_apps::catalog::Catalog::scan(&crate::state::shell_locales_vec());
                self.icon_cache.clear();
                self.home_ui.invalidate();
                self.shell_dirty = true;
                self.needs_present = true;
                IpcResponse::ok(
                    id,
                    Some(serde_json::json!({ "apps": self.catalog.all().len() })),
                )
            }
            IpcCommand::SetPanel { open: false, .. } => {
                self.panel_close();
                self.needs_present = true;
                IpcResponse::ok(id, None)
            }
            IpcCommand::SetPanel { panel, open: true } => {
                if self.panel_open(map_panel_kind(panel)) {
                    IpcResponse::ok(id, None)
                } else {
                    IpcResponse::err(id, "panel_not_allowed")
                }
            }
            IpcCommand::ShowOsd {
                kind,
                percent,
                muted,
            } => {
                self.osd_fx
                    .show(kind, percent.min(100), muted, Instant::now());
                self.needs_present = true;
                IpcResponse::ok(id, None)
            }
            IpcCommand::Screenshot { save } => self.ctl_screenshot(id, save),
            IpcCommand::InjectTouch { phase, slot, x, y } => {
                let time_ms = std::time::Duration::from(self.clock.now()).as_millis() as u32;
                let pos = smithay::utils::Point::<f64, smithay::utils::Logical>::from((
                    f64::from(x),
                    f64::from(y),
                ));
                match phase {
                    TouchInjectPhase::Down => self.inject_touch_down(slot, pos, time_ms),
                    TouchInjectPhase::Move => self.inject_touch_motion(slot, pos, time_ms),
                    TouchInjectPhase::Up => {
                        if (x != 0 || y != 0)
                            && ((self.last_touch_pos.x as i32) != x
                                || (self.last_touch_pos.y as i32) != y)
                        {
                            self.inject_touch_motion(slot, pos, time_ms);
                        }
                        self.inject_touch_up(slot, time_ms);
                    }
                    TouchInjectPhase::Cancel => self.inject_touch_cancel(slot, time_ms),
                }
                self.needs_present = true;
                IpcResponse::ok(id, None)
            }
            IpcCommand::InjectKey { keycode, pressed } => {
                if !self.hw_keyboard {
                    self.hw_keyboard = true;
                    self.osk = None;
                }
                let time_ms = std::time::Duration::from(self.clock.now()).as_millis() as u32;
                self.inject_key(keycode, pressed, time_ms);
                self.needs_present = true;
                IpcResponse::ok(id, None)
            }
            IpcCommand::InjectHwButton { button } => {
                self.inject_hw_button(button);
                self.needs_present = true;
                IpcResponse::ok(id, None)
            }
            IpcCommand::SetRotation { mode } => match self.set_rotation_mode(&mode) {
                Ok(()) => IpcResponse::ok(
                    id,
                    Some(serde_json::json!({ "orientation": self.rotation.current.name() })),
                ),
                Err(e) => IpcResponse::err(id, e),
            },
            IpcCommand::SetAppOrientation { app_id, request } => {
                self.rotation.app_requests.insert(app_id, request);
                self.reevaluate_orientation();
                IpcResponse::ok(
                    id,
                    Some(serde_json::json!({ "orientation": self.rotation.current.name() })),
                )
            }
            IpcCommand::SetMock { key, value } => match de_system::mock::apply(&key, &value) {
                Ok(state_json) => {
                    self.poll_mock_system();
                    self.meta_cache = None;
                    self.shell_dirty = true;
                    self.needs_present = true;
                    IpcResponse::ok(id, Some(state_json))
                }
                Err(e) => IpcResponse::err(id, e),
            },
        }
    }

    fn ctl_screenshot(&mut self, id: u64, save: bool) -> IpcResponse {
        let Some(px) = crate::composite::capture_output(self) else {
            return IpcResponse::err(id, "no_frame_available");
        };
        let png = match de_paint::encode_png(&px) {
            Ok(bytes) => bytes,
            Err(e) => return IpcResponse::err(id, format!("encode_png: {e}")),
        };
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        let path = screenshot_path(save, now_ms);
        if let Some(parent) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                return IpcResponse::err(id, format!("mkdir {}: {e}", parent.display()));
            }
        }
        if let Err(e) = std::fs::write(&path, png) {
            return IpcResponse::err(id, format!("write {}: {e}", path.display()));
        }
        let path_str = path.to_string_lossy().into_owned();
        if save {
            self.shot_fx.fire(&path_str, Instant::now());
            self.needs_present = true;
        }
        IpcResponse::ok(
            id,
            Some(serde_json::json!({
                "path": path_str,
                "width": px.size.w,
                "height": px.size.h,
                "saved": save,
            })),
        )
    }

    fn ctl_state_snapshot(&self) -> ShellStateSnapshot {
        let foreground = match self.foreground {
            Foreground::Shell => "shell",
            Foreground::App => "app",
        }
        .to_string();
        let screen = format!("{:?}", self.nav.current).to_ascii_lowercase();
        let focused_surface = self.seat.get_keyboard().and_then(|kb| kb.current_focus());
        let windows = self
            .space
            .elements()
            .map(|w| {
                let app_id = w
                    .toplevel()
                    .and_then(|t| {
                        smithay::wayland::compositor::with_states(t.wl_surface(), |states| {
                            states
                                .data_map
                                .get::<smithay::wayland::shell::xdg::XdgToplevelSurfaceData>()
                                .and_then(|d| d.lock().ok()?.app_id.clone())
                        })
                    })
                    .unwrap_or_default();
                let focused = focused_surface
                    .as_ref()
                    .is_some_and(|fs| w.toplevel().is_some_and(|t| t.wl_surface() == fs));
                WindowSummary { app_id, focused }
            })
            .collect();

        ShellStateSnapshot {
            foreground,
            screen,
            locked: self.locked(),
            output: Size::new(self.output.width as i32, self.output.height as i32),
            insets: InsetSummary {
                ime_bottom: self.insets.ime.bottom,
            },
            windows,
            orientation: self.rotation.current.name().to_string(),
            auto_rotate: self.rotation.policy.auto,
        }
    }
}

/// Commands permitted over the runtime control socket while Keyguard or
/// `ext-session-lock-v1` is active (`docs/DE-KEYGUARD-AUTH.md` §2.4).
#[must_use]
fn command_allowed_while_locked(cmd: &IpcCommand) -> bool {
    matches!(
        cmd,
        IpcCommand::GetState
            | IpcCommand::Lock
            | IpcCommand::ReloadTheme
            | IpcCommand::ReloadApps
            | IpcCommand::ShowOsd { .. }
            | IpcCommand::SetPanel { open: false, .. }
            | IpcCommand::InjectTouch { .. }
            | IpcCommand::InjectKey { .. }
            | IpcCommand::InjectHwButton { .. }
            | IpcCommand::SetMock { .. }
            | IpcCommand::SetRotation { .. }
            | IpcCommand::SetAppOrientation { .. }
    )
}

fn map_panel_kind(panel: PanelKind) -> crate::panel_ui::PanelKind {
    match panel {
        PanelKind::Notifications => crate::panel_ui::PanelKind::Notifications,
        PanelKind::Quick => crate::panel_ui::PanelKind::Quick,
    }
}

/// Resolve the screenshot destination path:
/// - `save == true`: `$XDG_PICTURES_DIR/Screenshots` (or `$HOME/Pictures/Screenshots`), timestamped PNG.
/// - `save == false`: `$XDG_RUNTIME_DIR/rabbiz-de-shot.png` (ephemeral, does not pollute Pictures).
fn screenshot_path(save: bool, timestamp_ms: u128) -> PathBuf {
    if save {
        let pictures = std::env::var_os("XDG_PICTURES_DIR")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME")
                    .filter(|v| !v.is_empty())
                    .map(|h| PathBuf::from(h).join("Pictures"))
            })
            .unwrap_or_else(|| PathBuf::from("/tmp"));
        pictures
            .join("Screenshots")
            .join(format!("Screenshot_{timestamp_ms}.png"))
    } else {
        let runtime = std::env::var_os("XDG_RUNTIME_DIR")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/tmp"));
        runtime.join("rabbiz-de-shot.png")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_kind_maps_both_variants() {
        assert_eq!(
            map_panel_kind(PanelKind::Notifications),
            crate::panel_ui::PanelKind::Notifications
        );
        assert_eq!(
            map_panel_kind(PanelKind::Quick),
            crate::panel_ui::PanelKind::Quick
        );
    }

    #[test]
    fn screenshot_paths_distinguish_saved_and_ephemeral() {
        let saved = screenshot_path(true, 1_700_000_000_123);
        assert!(saved.ends_with("Screenshots/Screenshot_1700000000123.png"));

        let ephemeral = screenshot_path(false, 1_700_000_000_123);
        assert!(ephemeral.ends_with("rabbiz-de-shot.png"));
        assert_ne!(saved, ephemeral);
    }

    #[test]
    fn lock_guard_blocks_sensitive_commands_while_locked() {
        use de_config::ipc::{HwButtonKind, OsdKind};

        // Allowed while locked: state query, re-lock, theme/app reload, OSD, panel close, and virtual input/mock.
        assert!(command_allowed_while_locked(&IpcCommand::GetState));
        assert!(command_allowed_while_locked(&IpcCommand::Lock));
        assert!(command_allowed_while_locked(&IpcCommand::ReloadTheme));
        assert!(command_allowed_while_locked(&IpcCommand::ReloadApps));
        assert!(command_allowed_while_locked(&IpcCommand::ShowOsd {
            kind: OsdKind::Volume,
            percent: 50,
            muted: false,
        }));
        assert!(command_allowed_while_locked(&IpcCommand::SetPanel {
            panel: PanelKind::Notifications,
            open: false,
        }));
        assert!(command_allowed_while_locked(&IpcCommand::SetPanel {
            panel: PanelKind::Quick,
            open: false,
        }));
        assert!(command_allowed_while_locked(&IpcCommand::InjectTouch {
            phase: TouchInjectPhase::Down,
            slot: 0,
            x: 100,
            y: 200,
        }));
        assert!(command_allowed_while_locked(&IpcCommand::InjectKey {
            keycode: 28,
            pressed: true,
        }));
        assert!(command_allowed_while_locked(&IpcCommand::InjectHwButton {
            button: HwButtonKind::Power,
        }));
        assert!(command_allowed_while_locked(&IpcCommand::SetMock {
            key: "battery".to_string(),
            value: "80".to_string(),
        }));

        // Blocked while locked: launching apps, taking screenshots, opening panels.
        assert!(!command_allowed_while_locked(&IpcCommand::Launch {
            desktop_id: "org.example.app".to_string(),
        }));
        assert!(!command_allowed_while_locked(&IpcCommand::Screenshot {
            save: true,
        }));
        assert!(!command_allowed_while_locked(&IpcCommand::Screenshot {
            save: false,
        }));
        assert!(!command_allowed_while_locked(&IpcCommand::SetPanel {
            panel: PanelKind::Notifications,
            open: true,
        }));
        assert!(!command_allowed_while_locked(&IpcCommand::SetPanel {
            panel: PanelKind::Quick,
            open: true,
        }));
    }

    #[test]
    fn bind_at_sets_mode_0600_and_removes_on_drop() {
        let path = std::env::temp_dir().join(format!(
            "rabbiz-de-ctl-test-{}-{}.sock",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        {
            let ipc = CtlIpc::bind_at(path.clone()).expect("bind_at");
            let mode = std::fs::metadata(&path)
                .expect("metadata")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
            assert!(ipc.try_accept().is_none());
        }
        assert!(!path.exists(), "socket must be unlinked on Drop");
    }
}
