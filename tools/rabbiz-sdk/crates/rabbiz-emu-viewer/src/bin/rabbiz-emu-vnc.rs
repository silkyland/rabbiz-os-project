//! RFB 3.8 (VNC) bridge from `rabbiz-emu-proto` (`127.0.0.1:5910`) to `127.0.0.1:5900`
//! so `RabbizMirror` can mirror the Tier 2 emulator simultaneously with `rabbiz-emu-viewer`.

#![forbid(unsafe_code)]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use rabbiz_emu_proto::{client_handshake, client_poll_raw_if_newer};

fn main() {
    let emu_addr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:5910".to_string());
    let ctl_addr = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "127.0.0.1:5911".to_string());
    let vnc_addr = std::env::args()
        .nth(3)
        .unwrap_or_else(|| "127.0.0.1:5900".to_string());

    let listener = TcpListener::bind(&vnc_addr).expect("bind VNC 5900");
    eprintln!("rabbiz-emu-vnc: listening on {vnc_addr} -> emu {emu_addr} (ctl {ctl_addr})");

    for stream in listener.incoming().flatten() {
        let emu = emu_addr.clone();
        let ctl = ctl_addr.clone();
        std::thread::spawn(move || {
            if let Err(e) = serve_client(stream, &emu, &ctl) {
                eprintln!("rabbiz-emu-vnc client ended: {e}");
            }
        });
    }
}

fn serve_client(mut vnc: TcpStream, emu_addr: &str, ctl_addr: &str) -> Result<(), String> {
    let _ = vnc.set_nodelay(true);
    let mut emu = TcpStream::connect(emu_addr).map_err(|e| format!("connect emu: {e}"))?;
    let _ = emu.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = emu.set_write_timeout(Some(Duration::from_secs(2)));
    let hello = client_handshake(&mut emu)?;
    let (mut w, mut h) = (hello.width as u16, hello.height as u16);

    // 1. RFB 3.8 version handshake
    vnc.write_all(b"RFB 003.008\n").map_err(|e| e.to_string())?;
    let mut ver = [0u8; 12];
    vnc.read_exact(&mut ver).map_err(|e| e.to_string())?;

    // 2. Security: 1 type = None (1)
    vnc.write_all(&[1, 1]).map_err(|e| e.to_string())?;
    let mut sec = [0u8; 1];
    vnc.read_exact(&mut sec).map_err(|e| e.to_string())?;
    vnc.write_all(&0u32.to_be_bytes()).map_err(|e| e.to_string())?;

    // 3. ClientInit (shared flag)
    let mut shared = [0u8; 1];
    vnc.read_exact(&mut shared).map_err(|e| e.to_string())?;

    // 4. ServerInit: width, height, 32bpp RGBA pixel format (R at byte 0, G at 1, B at 2)
    let name = b"RabbizOS Emu";
    let mut init = Vec::with_capacity(24 + name.len());
    init.extend_from_slice(&w.to_be_bytes());
    init.extend_from_slice(&h.to_be_bytes());
    // PixelFormat (16 bytes): bpp=32, depth=24, big_endian=0, true_color=1, max=255, BGRA shifts r=16,g=8,b=0
    init.extend_from_slice(&[32, 24, 0, 1]);
    init.extend_from_slice(&255u16.to_be_bytes());
    init.extend_from_slice(&255u16.to_be_bytes());
    init.extend_from_slice(&255u16.to_be_bytes());
    init.extend_from_slice(&[16, 8, 0, 0, 0, 0]);
    init.extend_from_slice(&(name.len() as u32).to_be_bytes());
    init.extend_from_slice(name);
    vnc.write_all(&init).map_err(|e| e.to_string())?;

    let _ = vnc.set_read_timeout(Some(Duration::from_millis(16)));
    let mut last_seq = 0u64;
    let mut prev_buttons = 0u8;
    let mut pending_fb_req = false;

    loop {
        // Drain incoming RFB client messages without blocking frame stream
        let mut msg_type = [0u8; 1];
        match vnc.read_exact(&mut msg_type) {
            Ok(()) => match msg_type[0] {
                0 => {
                    let mut buf = [0u8; 19];
                    vnc.read_exact(&mut buf).map_err(|e| e.to_string())?;
                }
                2 => {
                    let mut pad = [0u8; 1];
                    vnc.read_exact(&mut pad).map_err(|e| e.to_string())?;
                    let mut n = [0u8; 2];
                    vnc.read_exact(&mut n).map_err(|e| e.to_string())?;
                    let count = u16::from_be_bytes(n) as usize;
                    let mut encs = vec![0u8; count * 4];
                    vnc.read_exact(&mut encs).map_err(|e| e.to_string())?;
                }
                3 => {
                    let mut req = [0u8; 9];
                    vnc.read_exact(&mut req).map_err(|e| e.to_string())?;
                    pending_fb_req = true;
                }
                4 => {
                    let mut key = [0u8; 7];
                    vnc.read_exact(&mut key).map_err(|e| e.to_string())?;
                }
                5 => {
                    let mut ptr = [0u8; 5];
                    vnc.read_exact(&mut ptr).map_err(|e| e.to_string())?;
                    let buttons = ptr[0] & 1;
                    let px = u16::from_be_bytes([ptr[1], ptr[2]]) as i32;
                    let py = u16::from_be_bytes([ptr[3], ptr[4]]) as i32;
                    if buttons != prev_buttons || buttons != 0 {
                        let phase = if buttons != 0 && prev_buttons == 0 {
                            "down"
                        } else if buttons == 0 && prev_buttons != 0 {
                            "up"
                        } else {
                            "move"
                        };
                        let _ = send_touch_ctl(ctl_addr, phase, px, py);
                        prev_buttons = buttons;
                    }
                }
                6 => {
                    let mut pad = [0u8; 3];
                    vnc.read_exact(&mut pad).map_err(|e| e.to_string())?;
                    let mut len_b = [0u8; 4];
                    vnc.read_exact(&mut len_b).map_err(|e| e.to_string())?;
                    let len = u32::from_be_bytes(len_b) as usize;
                    let mut txt = vec![0u8; len.min(65536)];
                    vnc.read_exact(&mut txt).map_err(|e| e.to_string())?;
                }
                _ => {}
            },
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(e) => return Err(e.to_string()),
        }

        if pending_fb_req {
            if let Ok(Some(frame)) = client_poll_raw_if_newer(&mut emu, last_seq) {
                last_seq = frame.seq;
                if let Some(tile) = frame.tiles.first() {
                    w = frame.full_size.w as u16;
                    h = frame.full_size.h as u16;
                    let mut hdr = Vec::with_capacity(16);
                    hdr.extend_from_slice(&[0, 0]);
                    hdr.extend_from_slice(&1u16.to_be_bytes());
                    hdr.extend_from_slice(&0u16.to_be_bytes());
                    hdr.extend_from_slice(&0u16.to_be_bytes());
                    hdr.extend_from_slice(&w.to_be_bytes());
                    hdr.extend_from_slice(&h.to_be_bytes());
                    hdr.extend_from_slice(&0i32.to_be_bytes()); // Raw encoding
                    let mut bgra = vec![0u8; tile.png_or_rgba.len()];
                    for (dst, src) in bgra
                        .chunks_exact_mut(4)
                        .zip(tile.png_or_rgba.chunks_exact(4))
                    {
                        dst[0] = src[2]; // B
                        dst[1] = src[1]; // G
                        dst[2] = src[0]; // R
                        dst[3] = 0xFF;
                    }
                    vnc.write_all(&hdr).map_err(|e| e.to_string())?;
                    vnc.write_all(&bgra).map_err(|e| e.to_string())?;
                    vnc.flush().map_err(|e| e.to_string())?;
                    pending_fb_req = false;
                }
            }
        }
    }
}

fn send_touch_ctl(ctl_addr: &str, phase: &str, x: i32, y: i32) -> std::io::Result<()> {
    let mut s = TcpStream::connect(ctl_addr)?;
    let _ = s.set_write_timeout(Some(Duration::from_millis(200)));
    let line = format!(
        "{{\"id\":1,\"cmd\":\"inject_touch\",\"phase\":\"{phase}\",\"slot\":0,\"x\":{x},\"y\":{y}}}\n"
    );
    s.write_all(line.as_bytes())
}
