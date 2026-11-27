use std::fs::File;
use std::io::BufWriter;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::rfb::{FrameSnapshot, RfbClient, RfbEvent};

pub fn run(args: &[String]) -> i32 {
    let mut host = "172.16.42.1".to_string();
    let mut port: u16 = 5900;
    let mut output = "/tmp/rabbiz-mirror.png".to_string();
    let mut frame_target: usize = 30;
    let mut timeout_secs: f64 = 25.0;

    let mut it = args.iter().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--host" => {
                if let Some(v) = it.next() {
                    host = v.clone();
                }
            }
            "--port" => {
                if let Some(v) = it.next() {
                    if let Ok(p) = v.parse() {
                        port = p;
                    }
                }
            }
            "--out" => {
                if let Some(v) = it.next() {
                    output = v.clone();
                }
            }
            "--frames" => {
                if let Some(v) = it.next() {
                    if let Ok(f) = v.parse() {
                        frame_target = f;
                    }
                }
            }
            "--timeout" => {
                if let Some(v) = it.next() {
                    if let Ok(t) = v.parse() {
                        timeout_secs = t;
                    }
                }
            }
            _ => {}
        }
    }

    println!("selftest: connecting to {host}:{port}");
    let (tx, rx) = mpsc::channel();
    let client = RfbClient::connect(host, port, tx);

    let started = Instant::now();
    let deadline = started + Duration::from_secs_f64(timeout_secs.max(1.0));
    let mut frames: usize = 0;
    let mut last_frame: Option<FrameSnapshot> = None;
    let mut last_frame_at: Option<Instant> = None;
    let mut success = false;
    let mut reason = "timed out".to_string();

    loop {
        let now = Instant::now();
        if now >= deadline {
            break;
        }
        let wait = (deadline - now).min(Duration::from_millis(200));
        match rx.recv_timeout(wait) {
            Ok(RfbEvent::Connected { width, height }) => {
                println!("selftest: connected {width}x{height}");
            }
            Ok(RfbEvent::Frame(snap)) => {
                frames += 1;
                if let Some(prev) = last_frame_at {
                    if frames <= 12 {
                        println!(
                            "selftest: frame {frames} after {:.0} ms",
                            prev.elapsed().as_secs_f64() * 1000.0
                        );
                    }
                }
                last_frame_at = Some(Instant::now());
                if frames == 1 {
                    println!("selftest: first frame {}x{}", snap.width, snap.height);
                }
                last_frame = Some(snap);
                if frames >= frame_target {
                    success = true;
                    reason = format!("{frame_target} frames");
                    break;
                }
            }
            Ok(RfbEvent::Disconnected(msg)) => {
                reason = msg;
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                reason = "channel closed".to_string();
                break;
            }
        }
    }

    client.disconnect();

    let elapsed = started.elapsed().as_secs_f64();
    let saved = last_frame
        .as_ref()
        .is_some_and(|snap| write_png(snap, &output));
    let fps = if elapsed > 0.0 {
        frames as f64 / elapsed
    } else {
        0.0
    };
    println!(
        "selftest: {} — {} frames in {:.1}s ({:.1} fps), {}",
        if success { "ok" } else { "failed" },
        frames,
        elapsed,
        fps,
        reason
    );
    if saved {
        println!("selftest: wrote {output}");
    }
    if success {
        0
    } else {
        1
    }
}

fn write_png(snap: &FrameSnapshot, path: &str) -> bool {
    let Ok(file) = File::create(path) else {
        return false;
    };
    let w = BufWriter::new(file);
    let mut encoder = png::Encoder::new(w, snap.width as u32, snap.height as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let Ok(mut writer) = encoder.write_header() else {
        return false;
    };

    // Convert BGRA -> RGBA for PNG
    let mut rgba = Vec::with_capacity(snap.bgra.len());
    for px in snap.bgra.chunks_exact(4) {
        rgba.push(px[2]);
        rgba.push(px[1]);
        rgba.push(px[0]);
        rgba.push(px[3]);
    }
    writer.write_image_data(&rgba).is_ok()
}
