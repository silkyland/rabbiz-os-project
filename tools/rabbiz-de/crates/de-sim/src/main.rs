//! `de-sim` — host simulator + golden harness for `rabbiz-de` screens.
//!
//! Usage:
//!   de-sim --scene <name|all> [--out file.png | --out-dir dir]
//!          [--size WxH] [--window] [--render-ms]
//!   de-sim --bless --golden-dir <dir>   (update goldens; usually via de_golden.sh)
//!
//! Scenes: `brand` (P0 boot frame). `all` expands to every registered scene.

mod golden;

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use de_api::{Pixels, Size};

const DEFAULT_SIZE: Size = Size { w: 720, h: 1600 };

/// Render a named scene to premultiplied pixels.
fn render_scene(name: &str, size: Size) -> Option<Pixels> {
    match name {
        "brand" => Some(de_ui::boot::brand_frame(size, 0.5)),
        _ => None,
    }
}

/// All scene names for `--scene all` (golden file stems, 1:1).
fn all_scenes() -> &'static [&'static str] {
    &["brand"]
}

struct Args {
    scene: String,
    out: Option<PathBuf>,
    out_dir: Option<PathBuf>,
    golden_dir: Option<PathBuf>,
    size: Size,
    window: bool,
    render_ms: bool,
    bless: bool,
    check: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args {
        scene: "brand".to_string(),
        out: None,
        out_dir: None,
        golden_dir: None,
        size: DEFAULT_SIZE,
        window: false,
        render_ms: false,
        bless: false,
        check: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--scene" => a.scene = it.next().ok_or("--scene needs a value")?,
            "--out" => a.out = Some(PathBuf::from(it.next().ok_or("--out needs a path")?)),
            "--out-dir" => {
                a.out_dir = Some(PathBuf::from(it.next().ok_or("--out-dir needs a path")?))
            }
            "--golden-dir" => {
                a.golden_dir = Some(PathBuf::from(it.next().ok_or("--golden-dir needs a path")?))
            }
            "--size" => {
                let v = it.next().ok_or("--size needs WxH")?;
                let (w, h) = v.split_once('x').ok_or("--size WxH")?;
                a.size = Size::new(
                    w.parse().map_err(|_| "--size width")?,
                    h.parse().map_err(|_| "--size height")?,
                );
            }
            "--window" => a.window = true,
            "--render-ms" => a.render_ms = true,
            "--bless" => a.bless = true,
            "--check" => a.check = true,
            "--help" | "-h" => {
                println!(
                    "de-sim --scene <name|all> [--out file.png|--out-dir dir] \
                     [--size WxH] [--window] [--render-ms] [--bless --golden-dir dir]"
                );
                std::process::exit(0);
            }
            other => return Err(format!("unknown arg: {other}")),
        }
    }
    Ok(a)
}

fn scenes_for(name: &str) -> Result<Vec<String>, String> {
    if name == "all" {
        Ok(all_scenes().iter().map(|s| (*s).to_string()).collect())
    } else if render_scene(name, Size::new(1, 1)).is_some() {
        Ok(vec![name.to_string()])
    } else {
        Err(format!(
            "unknown scene '{name}' (have: {})",
            all_scenes().join(", ")
        ))
    }
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("de-sim: {e}");
            return ExitCode::from(2);
        }
    };

    let scenes = match scenes_for(&args.scene) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("de-sim: {e}");
            return ExitCode::from(2);
        }
    };

    let mut failed = false;
    for scene in &scenes {
        let t = Instant::now();
        let Some(px) = render_scene(scene, args.size) else {
            eprintln!("de-sim: scene '{scene}' returned nothing");
            failed = true;
            continue;
        };
        if args.render_ms {
            eprintln!(
                "{scene}: render {:.2} ms",
                t.elapsed().as_secs_f64() * 1000.0
            );
        }

        if args.check {
            let dir = args
                .golden_dir
                .clone()
                .unwrap_or_else(|| PathBuf::from("tests/golden"));
            let path = dir.join(format!("{scene}.png"));
            match golden::compare(&px, &path) {
                Ok(c) => {
                    println!("{scene}: {c}");
                    if !c.pass {
                        failed = true;
                    }
                }
                Err(e) => {
                    println!("{scene}: FAIL ({e})");
                    failed = true;
                }
            }
            continue;
        }

        if args.bless {
            let dir = args
                .golden_dir
                .clone()
                .unwrap_or_else(|| PathBuf::from("tests/golden"));
            let path = dir.join(format!("{scene}.png"));
            match golden::write_png(&px, &path) {
                Ok(()) => eprintln!("blessed {}", path.display()),
                Err(e) => {
                    eprintln!("de-sim: {e}");
                    failed = true;
                }
            }
            continue;
        }

        if let Some(dir) = &args.out_dir {
            let path = dir.join(format!("{scene}.png"));
            if let Err(e) = golden::write_png(&px, &path) {
                eprintln!("de-sim: {e}");
                failed = true;
            }
        } else if let Some(path) = &args.out {
            if let Err(e) = golden::write_png(&px, path) {
                eprintln!("de-sim: {e}");
                failed = true;
            }
        }
    }

    if args.window {
        if let Some(px) = render_scene(&scenes[0], args.size) {
            run_window(&px);
        }
    }

    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// Interactive preview window (host dev tool; not used by the golden harness).
fn run_window(px: &Pixels) {
    // Premultiplied RGBA8 -> packed 0xAARRGGBB for minifb.
    let mut buf = Vec::with_capacity(px.data.len() / 4);
    for c in px.data.chunks_exact(4) {
        buf.push((c[3] as u32) << 24 | (c[0] as u32) << 16 | (c[1] as u32) << 8 | c[2] as u32);
    }
    let (w, h) = (px.size.w as usize, px.size.h as usize);
    let Ok(mut win) = minifb::Window::new(
        "de-sim",
        w,
        h,
        minifb::WindowOptions {
            scale: minifb::Scale::X1,
            ..minifb::WindowOptions::default()
        },
    ) else {
        eprintln!("de-sim: cannot open window (no display?)");
        return;
    };
    while win.is_open() && !win.is_key_down(minifb::Key::Escape) {
        let _ = win.update_with_buffer(&buf, w, h);
    }
}
