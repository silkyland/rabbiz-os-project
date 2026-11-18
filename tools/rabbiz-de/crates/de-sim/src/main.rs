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
        "lock" => {
            let wp = de_paint::wallpaper::wallpaper(None, "sand", size);
            let blurred = de_paint::glass::blurred_backdrop(&wp);
            let status = sim_status();
            let items = vec![de_ui::panels::Notification {
                id: 1,
                app: "Chats".into(),
                title: "Alex".into(),
                body: "See you at 6".into(),
                when: "now".into(),
                icon: None,
            }];
            let m = de_ui::lock::LockModel {
                status: &status,
                clock: "09:41",
                date: "Mon, Jun 9",
                charging: s.get("lock.charging"),
                notifications: &items,
                hidden_notifications: 0,
                fingerprint: true,
                flashlight: Some(false),
                quick: Some(de_ui::lock::QuickAction::Camera),
                backdrop: &blurred,
            };
            Some(de_ui::lock::render(size, s, t, &m))
        }
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
                "rabbiz-calculator",
                "rabbiz-files",
                "rabbiz-browser",
                "rabbiz-settings",
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
            let card = de_ui::recents::card_rect(size);
            let settings = de_ui::settings::render_index(size, s, t, "09:41");
            let preview = de_paint::wallpaper::cover_scale(&settings, card.size());
            let mut recent = de_ui::recents::RecentEntry::new("rabbiz-settings", s.get("settings.title"));
            recent.snapshot = Some(std::sync::Arc::new(de_ui::recents::round_snapshot(preview, card)));
            let m = de_ui::recents::RecentsModel::new(vec![
                recent,
                de_ui::recents::RecentEntry::new("rabbiz-files", "Files"),
                de_ui::recents::RecentEntry::new("rabbiz-browser", "Browser"),
            ]);
            let home = render_scene("home", size, s)?;
            let backdrop = de_paint::glass::blurred_backdrop(&home);
            Some(de_ui::recents::render_with_backdrop(
                size, s, t, &sim_status(), &m, Some(&backdrop),
            ))
        }
        "shade" => Some(de_ui::shade::render(size, s, t, "09:41")),
        "panel-notifications" | "panel-quick" => {
            let wp = de_paint::wallpaper::wallpaper(None, "sand", size);
            let blurred = de_paint::glass::blurred_backdrop(&wp);
            let controls = {
            if name == "panel-quick" {
                let tile = |id: &'static str, label: &str, glyph: &'static str, on: bool, detail: &str| {
                    de_ui::panels::QuickTile {
                        id,
                        label: label.into(),
                        glyph,
                        on,
                        available: true,
                        detail: detail.into(),
                    }
                };
                let m = de_ui::panels::QuickModel {
                    tiles: vec![
                        tile("wifi", s.get("qs.wifi"), "\u{e63e}", true, "DATA-HORIZON"),
                        tile("bt", s.get("qs.bluetooth"), "\u{e1a7}", false, s.get("qs.off")),
                        tile("air", s.get("qs.airplane"), "\u{e195}", false, s.get("qs.off")),
                        tile("dnd", s.get("qs.dnd"), "\u{e644}", true, s.get("qs.on")),
                        tile("torch", s.get("qs.flashlight"), "\u{e3e7}", false, ""),
                        tile("rot", s.get("qs.rotate"), "\u{e1c1}", true, ""),
                        tile("saver", s.get("qs.battery_saver"), "\u{efde}", false, ""),
                        tile("loc", s.get("qs.location"), "\u{e0c8}", true, ""),
                        tile("hot", s.get("qs.hotspot"), "\u{e1e2}", false, ""),
                        tile("shot", s.get("qs.screenshot"), "\u{e412}", false, ""),
                        tile("rec", s.get("qs.record"), "\u{e04b}", false, ""),
                        tile("nfc", "NFC", "\u{e1bb}", false, ""),
                    ],
                    brightness: 0.62,
                    volume: 0.35,
                    brightness_available: true, volume_available: true, feedback: None,
                };
                de_ui::panels::render_quick(size, s, t, &blurred, "16:11", "Mon, 16 Nov", &m)
            } else {
                let n = |id, app: &str, title: &str, body: &str, when: &str| de_ui::panels::Notification {
                    id,
                    app: app.into(),
                    title: title.into(),
                    body: body.into(),
                    when: when.into(),
                    icon: None,
                };
                let items = vec![
                    n(1, "Chats", "ESport win", "Wittoon: sent a photo", "26 min"),
                    n(2, "Mail", "Nokpirab Express", "Thank you for playing with us. Your parcel ISX315632 is on its way.", "57 min"),
                    n(3, "Files", "Download finished", "rabbiz-de-release.tar.zst", "13:21"),
                ];
                de_ui::panels::render_notifications(size, s, t, &blurred, "16:11", "Mon, 16 Nov", &items)
            }
            };
            // The panel texture already bakes the frosted sheet beneath the
            // controls; the sim shows it fully open.
            Some(controls)
        }
        "status" => {
            // Full house, modelled on a real dual-SIM Android status bar:
            // AIS (SIM 1, VoLTE) + TRUE-H (SIM 2, 4G+), vibrate ringer,
            // alarm set, wifi, BT, camera dot, battery 48.
            let info = de_ui::chrome::StatusInfo {
                clock: "09:41".into(),
                battery_pct: Some(48),
                charging: false,
                battery_full: false,
                wifi: Some(true),
                wifi_strength: Some(72),
                cellular: vec![
                    de_api::services::CellularSlot {
                        carrier: "AIS".into(),
                        data_type: None,
                        volte: true,
                        strength: Some(4),
                    },
                    de_api::services::CellularSlot {
                        carrier: "TRUE-H".into(),
                        data_type: Some("4G+".into()),
                        volte: false,
                        strength: Some(3),
                    },
                ],
                ringer: de_api::services::RingerMode::Vibrate,
                alarm: true,
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
        "power" => Some(de_ui::power::render(
            size,
            s,
            t,
            &de_ui::power::PowerModel { armed: None },
        )),
        "search" => {
            let rows = vec![
                de_ui::search::SearchRow {
                    kind: de_ui::search::SearchKind::App,
                    id: "rabbiz-calculator".into(),
                    title: "Calculator".into(),
                    subtitle: "Utilities".into(),
                    icon: None,
                },
                de_ui::search::SearchRow {
                    kind: de_ui::search::SearchKind::App,
                    id: "rabbiz-calendar".into(),
                    title: "Calendar".into(),
                    subtitle: "Productivity".into(),
                    icon: None,
                },
                de_ui::search::SearchRow {
                    kind: de_ui::search::SearchKind::Action,
                    id: "rabbiz-camera".into(),
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
        "screenshot" => {
            let wp = de_paint::wallpaper::wallpaper(None, "sand", size);
            let toast = de_ui::screenshot::toast(t, Some(&wp), "a21s-screenshot.png", s);
            let r = de_ui::screenshot::toast_rect(size);
            let mut c = de_paint::Canvas::new(size);
            c.blit(&wp, de_api::Point::new(0, 0));
            c.blit(&toast, de_api::Point::new(r.x, r.y));
            Some(c.into_pixels())
        }
        "error" => Some(de_ui::error::render(size, s, t, "com.example.app")),
        "first_run" => Some(de_ui::first_run::render(size, s, t)),
        "settings" => Some(de_ui::settings::render_index(size, s, t, "09:41")),
        "morph-open" | "morph-close" => Some(morph_strip(size, s, name == "morph-open")),
        _ => None,
    }
}

fn morph_strip(size: Size, strings: &de_ui::i18n::Strings, opening: bool) -> Pixels {
    use de_api::Rect;
    use de_design::motion::{Transition, APP_CONTENT_FADE_MS};
    use de_paint::{animation::AnimationClock, Canvas};
    use de_ui::home::HomeHit;
    use de_ui::launch_morph::{morph_frame, morph_icon, MorphAnimator, MorphPose};

    let theme = &de_design::tokens::DEFAULT;
    let home = render_scene("home", size, strings).expect("home fixture");
    let mut app = Canvas::new(size);
    app.blit(
        &de_ui::settings::render_index(size, strings, theme, "09:41"),
        de_api::Point::ZERO,
    );
    // The demonstration client uses the Settings content and lets the shell
    // draw the gesture pill above it, as it does for a real Wayland window.
    let bar_h = de_ui::chrome::NAV_H;
    app.fill_rect(Rect::new(0, size.h - bar_h, size.w, bar_h), theme.bg);
    let app = app.into_pixels();
    let bar = de_ui::nav_bar::gesture_bar(size);
    let src = de_ui::home::icon_rect(size, HomeHit::Grid(0), 4).expect("grid bounds");
    let dst = Rect::new(0, 0, size.w, size.h);
    let icon = morph_icon(theme, &de_ui::chrome::IconEntry::new("app0", "App 0"), src);
    let clock = AnimationClock::new();
    let mut animator = MorphAnimator::new(if opening { 0.0 } else { 1.0 });
    animator.retarget(opening);
    let timing = Transition::AppWindow.timing();
    let motion = if opening {
        timing.enter()
    } else {
        timing.exit()
    };
    let step_ms = motion.duration_ms as f32 / 4.0;
    let width = (size.w / 4).max(1);
    let height = (size.h / 4).max(1);
    let mut strip = Canvas::new(Size::new(5 * width, height));
    for i in 0..=4 {
        let progress = animator.advance(if i == 0 { 0.0 } else { step_ms / 1000.0 }, &clock);
        // The synthetic app supplies its first buffer at t=0. A real late
        // buffer is covered by the compositor handshake test.
        let ready = if opening {
            i as f32 * step_ms / APP_CONTENT_FADE_MS as f32
        } else {
            1.0
        };
        let pose = MorphPose::at(progress, src, dst, ready);
        let mut frame = Canvas::new(size);
        frame.blit(
            &morph_frame(&home, theme, pose, &icon, &app),
            de_api::Point::ZERO,
        );
        frame.blit(&bar, de_api::Point::new(0, size.h - bar.size.h));
        strip.blit_scaled(
            &frame.into_pixels(),
            Rect::new(i * width, 0, width, height),
            1.0,
        );
    }
    strip.into_pixels()
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
        "panel-notifications",
        "panel-quick",
        "osd",
        "power",
        "search",
        "screenshot",
        "error",
        "first_run",
        "settings",
        "morph-open",
        "morph-close",
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
