//! `de-sim` — host simulator + golden harness for `rabbiz-de` screens.
//!
//! Usage:
//!   de-sim --scene <name|all> [--out file.png | --out-dir dir]
//!          [--size `WxH`] [--window] [--render-ms]
//!   de-sim --bless --golden-dir <dir>   (update goldens; usually via `de_golden.sh`)
//!
//! Scenes: `brand` (P0 boot frame). `all` expands to every registered scene.

mod golden;

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use de_api::{Pixels, Size};

const DEFAULT_SIZE: Size = Size { w: 720, h: 1600 };

/// Clock + wifi + BT + battery — the "normal" status bar for golden scenes.
fn sim_status() -> de_ui::chrome::StatusInfo {
    de_ui::chrome::StatusInfo {
        wifi: Some(true),
        bluetooth: true,
        ..de_ui::chrome::StatusInfo::new("09:41", "86")
    }
}

/// Render a named scene to premultiplied pixels.
#[allow(clippy::too_many_lines)] // flat scene-name → renderer dispatch
fn render_scene(name: &str, size: Size, s: &de_ui::i18n::Strings) -> Option<Pixels> {
    let t = &de_design::tokens::DEFAULT;
    match name {
        "brand" => Some(de_ui::boot::brand_frame(size, 0.5)),
        "lock" => Some(de_ui::lock::render(size, s, t, "09:41", "Mon, Jun 9")),
        "home" => {
            let apps: Vec<de_ui::chrome::IconEntry> = (0..12)
                .map(|i| de_ui::chrome::IconEntry::new(&format!("app{i}"), "App"))
                .enumerate()
                .map(|(i, mut e)| {
                    e.label = format!("App {i}");
                    e
                })
                .collect();
            let dock: Vec<de_ui::chrome::IconEntry> = [
                "a21s-calculator",
                "a21s-files",
                "a21s-browser",
                "a21s-settings",
            ]
            .iter()
            .map(|id| de_ui::chrome::IconEntry::new(id, &id[5..]))
            .collect();
            let m = de_ui::home::HomeModel {
                status: sim_status(),
                clock: "09:41",
                date: "Mon, 9 Jun",
                charging: "",
                wallpaper: Some(&de_paint::wallpaper::wallpaper(None, "ember", size)),
                apps: &apps,
                dock: &dock,
                page: 0,
                page_count: 2,
            };
            Some(de_ui::home::render(size, s, t, &m))
        }
        "drawer" => {
            let apps: Vec<de_ui::chrome::IconEntry> = (0..18)
                .map(|_| de_ui::chrome::IconEntry::new("app", "App"))
                .enumerate()
                .map(|(i, mut e)| {
                    e.id = format!("app{i}");
                    e.label = format!("App {i}");
                    e
                })
                .collect();
            Some(de_ui::drawer::render(size, s, t, &sim_status(), &apps, 0))
        }
        "recents" => {
            let m = de_ui::recents::RecentsModel::new(vec![
                de_ui::recents::RecentEntry::new("a21s-calculator", "Calculator"),
                de_ui::recents::RecentEntry::new("a21s-files", "Files"),
                de_ui::recents::RecentEntry::new("a21s-browser", "Browser"),
            ]);
            Some(de_ui::recents::render(size, s, t, &sim_status(), &m))
        }
        "shade" => Some(de_ui::shade::render(size, s, t, "09:41")),
        "status" => {
            let info = de_ui::chrome::StatusInfo {
                clock: "09:41".into(),
                battery_pct: Some(86),
                charging: true,
                battery_full: false,
                wifi: Some(true),
                bluetooth: true,
                dnd: false,
                airplane: false,
                privacy_cam: true,
                privacy_mic: false,
            };
            Some(de_ui::status::render(size, s, t, &info))
        }
        "qs" => Some(de_ui::qs::render(
            size,
            s,
            t,
            "09:41",
            "qs.wifi",
            &["Rabbiz-5G", "Home-WiFi", "CoffeeShop"],
        )),
        "osd" => Some(de_ui::osd::render(size, s, t, 0.65, "Volume", "\u{e050}")),
        "power" => Some(de_ui::power::render(size, s, t)),
        "search" => {
            let rows = vec![
                de_ui::search::SearchRow {
                    kind: de_ui::search::SearchKind::App,
                    id: "a21s-calculator".into(),
                    title: "Calculator".into(),
                    subtitle: "Utilities".into(),
                    icon: None,
                },
                de_ui::search::SearchRow {
                    kind: de_ui::search::SearchKind::App,
                    id: "a21s-calendar".into(),
                    title: "Calendar".into(),
                    subtitle: "Productivity".into(),
                    icon: None,
                },
                de_ui::search::SearchRow {
                    kind: de_ui::search::SearchKind::Action,
                    id: "a21s-camera".into(),
                    title: "Take a photo".into(),
                    subtitle: "Camera".into(),
                    icon: None,
                },
                de_ui::search::SearchRow {
                    kind: de_ui::search::SearchKind::Setting,
                    id: "settings.display".into(),
                    title: "Display brightness".into(),
                    subtitle: "Settings".into(),
                    icon: None,
                },
            ];
            Some(de_ui::search::render(size, s, t, "ca", &rows))
        }
        "screenshot" => Some(de_ui::screenshot::render(size, s, t)),
        "error" => Some(de_ui::error::render(size, s, t, "com.example.app")),
        "first_run" => Some(de_ui::first_run::render(size, s, t)),
        "settings" => Some(de_ui::settings::render_index(size, s, t, "09:41")),
        _ => None,
    }
}

/// All scene names for `--scene all` (golden file stems, 1:1).
fn all_scenes() -> &'static [&'static str] {
    &[
        "brand",
        "lock",
        "home",
        "drawer",
        "recents",
        "status",
        "shade",
        "qs",
        "osd",
        "power",
        "search",
        "screenshot",
        "error",
        "first_run",
        "settings",
    ]
}

#[allow(clippy::struct_excessive_bools)] // CLI flags
struct Args {
    scene: String,
    out: Option<PathBuf>,
    out_dir: Option<PathBuf>,
    golden_dir: Option<PathBuf>,
    size: Size,
    locale: String,
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
        locale: "en".to_string(),
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
                a.out_dir = Some(PathBuf::from(it.next().ok_or("--out-dir needs a path")?));
            }
            "--golden-dir" => {
                a.golden_dir = Some(PathBuf::from(it.next().ok_or("--golden-dir needs a path")?));
            }
            "--size" => {
                let v = it.next().ok_or("--size needs WxH")?;
                let (w, h) = v.split_once('x').ok_or("--size WxH")?;
                a.size = Size::new(
                    w.parse().map_err(|_| "--size width")?,
                    h.parse().map_err(|_| "--size height")?,
                );
            }
            "--locale" => a.locale = it.next().ok_or("--locale needs a value")?,
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
    } else if all_scenes().contains(&name) {
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
        let strings = de_ui::i18n::Strings::load(de_ui::i18n::Locale::from_code(&args.locale));
        let Some(px) = render_scene(scene, args.size, &strings) else {
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
            let path = dir.join(format!(
                "{scene}-{}x{}-{}.png",
                args.size.w, args.size.h, args.locale
            ));
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
            let path = dir.join(format!(
                "{scene}-{}x{}-{}.png",
                args.size.w, args.size.h, args.locale
            ));
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
            let path = dir.join(format!(
                "{scene}-{}x{}-{}.png",
                args.size.w, args.size.h, args.locale
            ));
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
        let strings = de_ui::i18n::Strings::load(de_ui::i18n::Locale::from_code(&args.locale));
        if let Some(px) = render_scene(&scenes[0], args.size, &strings) {
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
    for c in px.data.as_chunks::<4>().0 {
        buf.push(
            u32::from(c[3]) << 24 | u32::from(c[0]) << 16 | u32::from(c[1]) << 8 | u32::from(c[2]),
        );
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
