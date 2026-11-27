use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use flate2::{Decompress, FlushDecompress, Status};

use crate::zrle;

#[derive(Debug, Clone)]
pub struct FrameSnapshot {
    pub width: usize,
    pub height: usize,
    /// BGRA8888 little-endian (B, G, R, A), 4 bytes per pixel, tightly packed.
    pub bgra: Vec<u8>,
}

#[derive(Debug, Clone)]
pub enum RfbEvent {
    Connected { width: usize, height: usize },
    Frame(FrameSnapshot),
    Disconnected(String),
}

#[derive(Debug)]
enum RfbError {
    Io(String),
    Disconnected,
    Protocol(String),
    Auth(String),
}

impl RfbError {
    fn message(&self) -> String {
        match self {
            RfbError::Io(m) => format!("Connection failed: {m}"),
            RfbError::Disconnected => "The device closed the connection.".to_string(),
            RfbError::Protocol(m) => format!("VNC protocol error: {m}"),
            RfbError::Auth(m) => format!("VNC authentication: {m}"),
        }
    }
}

struct Inflater {
    decompress: Decompress,
    output: Vec<u8>,
}

impl Inflater {
    fn new() -> Self {
        Self {
            decompress: Decompress::new(true),
            output: vec![0u8; 1 << 20],
        }
    }

    fn inflate(&mut self, input: &[u8], expected: usize) -> Result<&[u8], RfbError> {
        if input.is_empty() {
            return Err(RfbError::Protocol("empty zlib block".to_string()));
        }
        let needed = (expected + 65_536).max(1 << 16);
        if self.output.len() < needed {
            self.output.resize(needed, 0);
        }

        let mut in_pos = 0usize;
        let mut out_pos = 0usize;

        loop {
            if out_pos == self.output.len() {
                let new_len = self.output.len().saturating_mul(2);
                if new_len > (64 << 20) {
                    return Err(RfbError::Protocol("zlib block larger than 64 MiB".to_string()));
                }
                self.output.resize(new_len, 0);
            }

            let before_in = self.decompress.total_in();
            let before_out = self.decompress.total_out();

            let status = self
                .decompress
                .decompress(
                    &input[in_pos..],
                    &mut self.output[out_pos..],
                    FlushDecompress::Sync,
                )
                .map_err(|e| RfbError::Protocol(format!("zlib inflate failed: {e}")))?;

            let consumed = (self.decompress.total_in() - before_in) as usize;
            let produced = (self.decompress.total_out() - before_out) as usize;
            in_pos += consumed;
            out_pos += produced;

            match status {
                Status::StreamEnd => break,
                Status::Ok => {
                    if in_pos >= input.len() && out_pos < self.output.len() {
                        break;
                    }
                }
                Status::BufError => {
                    if in_pos >= input.len() && produced == 0 {
                        break;
                    }
                    if out_pos == self.output.len() {
                        continue;
                    }
                    if consumed == 0 && produced == 0 {
                        break;
                    }
                }
            }
        }

        if in_pos < input.len() {
            return Err(RfbError::Protocol("zlib block truncated".to_string()));
        }
        Ok(&self.output[..out_pos])
    }
}

struct Framebuffer {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
}

impl Framebuffer {
    fn new() -> Self {
        Self {
            width: 0,
            height: 0,
            pixels: Vec::new(),
        }
    }

    fn resize(&mut self, width: usize, height: usize) {
        if self.width == width && self.height == height {
            return;
        }
        self.width = width;
        self.height = height;
        self.pixels.resize(width * height * 4, 0);
        // Set alpha to 255
        for chunk in self.pixels.chunks_exact_mut(4) {
            chunk[3] = 255;
        }
    }

    fn snapshot(&self) -> Option<FrameSnapshot> {
        if self.width == 0 || self.height == 0 || self.pixels.is_empty() {
            return None;
        }
        Some(FrameSnapshot {
            width: self.width,
            height: self.height,
            bgra: self.pixels.clone(),
        })
    }
}

#[derive(Clone)]
pub struct RfbClient {
    writer: Arc<Mutex<Option<TcpStream>>>,
    stopped: Arc<AtomicBool>,
}

impl RfbClient {
    pub fn connect(host: String, port: u16, events: Sender<RfbEvent>) -> Self {
        let writer = Arc::new(Mutex::new(None));
        let stopped = Arc::new(AtomicBool::new(false));

        let client = Self {
            writer: Arc::clone(&writer),
            stopped: Arc::clone(&stopped),
        };

        thread::Builder::new()
            .name("RabbizMirror.RFB".to_string())
            .spawn(move || {
                let reason = match run_client(&host, port, &writer, &stopped, &events) {
                    Ok(()) => "Disconnected".to_string(),
                    Err(e) => e.message(),
                };
                if let Ok(mut guard) = writer.lock() {
                    if let Some(stream) = guard.take() {
                        let _ = stream.shutdown(Shutdown::Both);
                    }
                }
                let user_stopped = stopped.load(Ordering::SeqCst);
                let _ = events.send(RfbEvent::Disconnected(if user_stopped {
                    "Disconnected".to_string()
                } else {
                    reason
                }));
            })
            .expect("spawn RFB thread");

        client
    }

    pub fn disconnect(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        if let Ok(mut guard) = self.writer.lock() {
            if let Some(stream) = guard.take() {
                let _ = stream.shutdown(Shutdown::Both);
            }
        }
    }

    pub fn send_pointer(&self, x: i32, y: i32, buttons: u8) {
        let clamped_x = x.clamp(0, 65_535) as u16;
        let clamped_y = y.clamp(0, 65_535) as u16;
        let msg = [
            5u8,
            buttons,
            (clamped_x >> 8) as u8,
            (clamped_x & 0xFF) as u8,
            (clamped_y >> 8) as u8,
            (clamped_y & 0xFF) as u8,
        ];
        self.send_bytes(&msg);
    }

    pub fn send_key(&self, keysym: u32, down: bool) {
        let msg = [
            4u8,
            u8::from(down),
            0,
            0,
            ((keysym >> 24) & 0xFF) as u8,
            ((keysym >> 16) & 0xFF) as u8,
            ((keysym >> 8) & 0xFF) as u8,
            (keysym & 0xFF) as u8,
        ];
        self.send_bytes(&msg);
    }

    fn send_bytes(&self, bytes: &[u8]) {
        if let Ok(mut guard) = self.writer.lock() {
            if let Some(stream) = guard.as_mut() {
                let _ = stream.write_all(bytes);
            }
        }
    }
}

fn open_socket(host: &str, port: u16) -> Result<TcpStream, RfbError> {
    let addr_str = format!("{host}:{port}");
    let addrs: Vec<_> = addr_str
        .to_socket_addrs()
        .map_err(|e| RfbError::Io(format!("{addr_str} — {e}")))?
        .collect();
    if addrs.is_empty() {
        return Err(RfbError::Io(format!("{addr_str} — no usable address")));
    }

    let mut last_err = "no usable address".to_string();
    for addr in addrs {
        match TcpStream::connect_timeout(&addr, Duration::from_secs(5)) {
            Ok(stream) => {
                let _ = stream.set_nodelay(true);
                return Ok(stream);
            }
            Err(e) => {
                last_err = e.to_string();
            }
        }
    }
    Err(RfbError::Io(format!("{addr_str} — {last_err}")))
}

fn read_exact_rfb(stream: &mut TcpStream, buf: &mut [u8]) -> Result<(), RfbError> {
    let mut read = 0usize;
    while read < buf.len() {
        match stream.read(&mut buf[read..]) {
            Ok(0) => return Err(RfbError::Disconnected),
            Ok(n) => read += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(RfbError::Io(e.to_string())),
        }
    }
    Ok(())
}

fn read_bytes(stream: &mut TcpStream, count: usize) -> Result<Vec<u8>, RfbError> {
    let mut buf = vec![0u8; count];
    if count > 0 {
        read_exact_rfb(stream, &mut buf)?;
    }
    Ok(buf)
}

fn read_u8(stream: &mut TcpStream) -> Result<u8, RfbError> {
    let mut b = [0u8; 1];
    read_exact_rfb(stream, &mut b)?;
    Ok(b[0])
}

fn read_u16(stream: &mut TcpStream) -> Result<u16, RfbError> {
    let mut b = [0u8; 2];
    read_exact_rfb(stream, &mut b)?;
    Ok(u16::from_be_bytes(b))
}

fn read_u32(stream: &mut TcpStream) -> Result<u32, RfbError> {
    let mut b = [0u8; 4];
    read_exact_rfb(stream, &mut b)?;
    Ok(u32::from_be_bytes(b))
}

fn write_bytes(writer: &Mutex<Option<TcpStream>>, bytes: &[u8]) -> Result<(), RfbError> {
    let mut guard = writer.lock().map_err(|_| RfbError::Disconnected)?;
    let stream = guard.as_mut().ok_or(RfbError::Disconnected)?;
    stream
        .write_all(bytes)
        .map_err(|e| RfbError::Io(e.to_string()))
}

fn read_reason(stream: &mut TcpStream) -> Result<Option<String>, RfbError> {
    let len = read_u32(stream)? as usize;
    if len == 0 || len > 4096 {
        return Ok(None);
    }
    let bytes = read_bytes(stream, len)?;
    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}

fn run_client(
    host: &str,
    port: u16,
    writer: &Arc<Mutex<Option<TcpStream>>>,
    stopped: &Arc<AtomicBool>,
    events: &Sender<RfbEvent>,
) -> Result<(), RfbError> {
    let mut stream = open_socket(host, port)?;
    if stopped.load(Ordering::SeqCst) {
        return Ok(());
    }
    let write_clone = stream.try_clone().map_err(|e| RfbError::Io(e.to_string()))?;
    {
        let mut guard = writer.lock().map_err(|_| RfbError::Disconnected)?;
        *guard = Some(write_clone);
    }

    // Handshake: RFB 003.008
    write_bytes(writer, b"RFB 003.008\n")?;
    let version = read_bytes(&mut stream, 12)?;
    if !version.starts_with(b"RFB") {
        return Err(RfbError::Protocol("not a VNC server".to_string()));
    }
    if !version.starts_with(b"RFB 003") || version[10] < b'8' {
        return Err(RfbError::Protocol(
            "server does not speak RFB 3.8".to_string(),
        ));
    }

    let type_count = read_u8(&mut stream)? as usize;
    if type_count == 0 {
        let reason = read_reason(&mut stream)?.unwrap_or_else(|| "no security types offered".to_string());
        return Err(RfbError::Auth(reason));
    }
    let types = read_bytes(&mut stream, type_count)?;
    if !types.contains(&1) {
        return Err(RfbError::Auth(format!(
            "device requires authentication (types {types:?})"
        )));
    }

    write_bytes(writer, &[1])?;
    let sec_result = read_u32(&mut stream)?;
    if sec_result != 0 {
        let reason = read_reason(&mut stream)?.unwrap_or_else(|| "rejected".to_string());
        return Err(RfbError::Auth(reason));
    }

    // ClientInit (shared = 1)
    write_bytes(writer, &[1])?;
    let header = read_bytes(&mut stream, 24)?;
    let width = u16::from_be_bytes([header[0], header[1]]) as usize;
    let height = u16::from_be_bytes([header[2], header[3]]) as usize;
    let name_len = u32::from_be_bytes([header[20], header[21], header[22], header[23]]) as usize;
    if name_len > 0 && name_len <= 65_536 {
        let _ = read_bytes(&mut stream, name_len)?;
    }
    if width == 0 || height == 0 || width > 16_384 || height > 16_384 {
        return Err(RfbError::Protocol(format!(
            "implausible framebuffer {width}x{height}"
        )));
    }

    let mut framebuffer = Framebuffer::new();
    framebuffer.resize(width, height);

    // SetPixelFormat: 32bpp, depth 24, little-endian, true-colour, R16 G8 B0
    let mut pixel_fmt = [0u8; 20];
    pixel_fmt[0] = 0; // SetPixelFormat
    pixel_fmt[4] = 32; // bpp
    pixel_fmt[5] = 24; // depth
    pixel_fmt[6] = 0; // little-endian
    pixel_fmt[7] = 1; // true-colour
    pixel_fmt[8..10].copy_from_slice(&255u16.to_be_bytes());
    pixel_fmt[10..12].copy_from_slice(&255u16.to_be_bytes());
    pixel_fmt[12..14].copy_from_slice(&255u16.to_be_bytes());
    pixel_fmt[14] = 16; // red shift
    pixel_fmt[15] = 8; // green shift
    pixel_fmt[16] = 0; // blue shift
    write_bytes(writer, &pixel_fmt)?;

    // SetEncodings: ZRLE (16), CopyRect (1), DesktopSize (-223), Raw (0)
    let encodings: [i32; 4] = [16, 1, -223, 0];
    let mut enc_msg = Vec::with_capacity(4 + encodings.len() * 4);
    enc_msg.extend_from_slice(&[2, 0, 0, encodings.len() as u8]);
    for enc in encodings {
        enc_msg.extend_from_slice(&enc.to_be_bytes());
    }
    write_bytes(writer, &enc_msg)?;

    let mut inflater = Inflater::new();

    let _ = events.send(RfbEvent::Connected { width, height });

    // Update loop
    let mut incremental = false;
    let mut last_full_refresh = Instant::now();

    while !stopped.load(Ordering::SeqCst) {
        let full_refresh = last_full_refresh.elapsed() > Duration::from_secs(4);
        let req_inc = incremental && !full_refresh;
        if full_refresh {
            last_full_refresh = Instant::now();
        }
        incremental = true;

        let w = (framebuffer.width.min(u16::MAX as usize)) as u16;
        let h = (framebuffer.height.min(u16::MAX as usize)) as u16;
        let mut req = [3u8, u8::from(req_inc), 0, 0, 0, 0, 0, 0, 0, 0];
        req[6..8].copy_from_slice(&w.to_be_bytes());
        req[8..10].copy_from_slice(&h.to_be_bytes());
        write_bytes(writer, &req)?;

        let msg_type = read_u8(&mut stream)?;
        match msg_type {
            0 => {
                let changed = read_framebuffer_update(&mut stream, &mut framebuffer, &mut inflater)?;
                if changed {
                    if let Some(snap) = framebuffer.snapshot() {
                        let _ = events.send(RfbEvent::Frame(snap));
                    }
                } else {
                    thread::sleep(Duration::from_millis(10));
                }
            }
            1 => {
                let _ = read_bytes(&mut stream, 1)?;
                let count = read_u16(&mut stream)? as usize;
                let _ = read_bytes(&mut stream, 6 * count)?;
            }
            2 => {} // Bell
            3 => {
                let _ = read_bytes(&mut stream, 3)?;
                let len = read_u32(&mut stream)? as usize;
                if len > 0 {
                    let _ = read_bytes(&mut stream, len)?;
                }
            }
            other => {
                return Err(RfbError::Protocol(format!(
                    "unknown server message {other}"
                )));
            }
        }
    }

    Ok(())
}

fn read_framebuffer_update(
    stream: &mut TcpStream,
    fb: &mut Framebuffer,
    inflater: &mut Inflater,
) -> Result<bool, RfbError> {
    let _ = read_bytes(stream, 1)?;
    let count = read_u16(stream)? as usize;
    let mut changed = false;

    for _ in 0..count {
        let header = read_bytes(stream, 12)?;
        let x = u16::from_be_bytes([header[0], header[1]]) as usize;
        let y = u16::from_be_bytes([header[2], header[3]]) as usize;
        let width = u16::from_be_bytes([header[4], header[5]]) as usize;
        let height = u16::from_be_bytes([header[6], header[7]]) as usize;
        let encoding = i32::from_be_bytes([header[8], header[9], header[10], header[11]]);

        match encoding {
            16 => {
                // ZRLE
                let len = read_u32(stream)? as usize;
                if len == 0 || len > (64 << 20) {
                    return Err(RfbError::Protocol(format!(
                        "implausible ZRLE block {len}"
                    )));
                }
                let compressed = read_bytes(stream, len)?;
                let data = inflater.inflate(&compressed, width * height * 3 + 65_536)?;
                if x + width > fb.width || y + height > fb.height {
                    return Err(RfbError::Protocol(
                        "ZRLE rectangle outside framebuffer".to_string(),
                    ));
                }
                let fb_w = fb.width;
                zrle::decode(data, x, y, width, height, &mut fb.pixels, fb_w)
                    .map_err(|e| RfbError::Protocol(e.0))?;
                changed = true;
            }
            0 => {
                // Raw
                let raw = read_bytes(stream, width * height * 4)?;
                if x + width > fb.width || y + height > fb.height {
                    return Err(RfbError::Protocol(
                        "raw rectangle outside framebuffer".to_string(),
                    ));
                }
                for row in 0..height {
                    let dst_off = ((y + row) * fb.width + x) * 4;
                    let src_off = row * width * 4;
                    fb.pixels[dst_off..dst_off + width * 4]
                        .copy_from_slice(&raw[src_off..src_off + width * 4]);
                }
                changed = true;
            }
            1 => {
                // CopyRect
                let src = read_bytes(stream, 4)?;
                let src_x = u16::from_be_bytes([src[0], src[1]]) as usize;
                let src_y = u16::from_be_bytes([src[2], src[3]]) as usize;
                if src_x + width > fb.width
                    || src_y + height > fb.height
                    || x + width > fb.width
                    || y + height > fb.height
                {
                    return Err(RfbError::Protocol(
                        "CopyRect outside framebuffer".to_string(),
                    ));
                }
                let row_bytes = width * 4;
                let forward = src_y < y || (src_y == y && src_x < x);
                let mut row_buf = vec![0u8; row_bytes];
                if forward {
                    for row in 0..height {
                        let s = ((src_y + row) * fb.width + src_x) * 4;
                        let d = ((y + row) * fb.width + x) * 4;
                        row_buf.copy_from_slice(&fb.pixels[s..s + row_bytes]);
                        fb.pixels[d..d + row_bytes].copy_from_slice(&row_buf);
                    }
                } else {
                    for row in (0..height).rev() {
                        let s = ((src_y + row) * fb.width + src_x) * 4;
                        let d = ((y + row) * fb.width + x) * 4;
                        row_buf.copy_from_slice(&fb.pixels[s..s + row_bytes]);
                        fb.pixels[d..d + row_bytes].copy_from_slice(&row_buf);
                    }
                }
                changed = true;
            }
            -223 => {
                // DesktopSize
                fb.resize(width, height);
                changed = true;
            }
            -239 => {
                // Cursor (tolerate if sent)
                let bytes_to_skip = width * height * 4 + width.div_ceil(8) * height;
                let _ = read_bytes(stream, bytes_to_skip)?;
            }
            other => {
                return Err(RfbError::Protocol(format!(
                    "unsupported encoding {other}"
                )));
            }
        }
    }

    Ok(changed)
}
