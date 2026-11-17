//! Host simulator: build one or all screen scenes, write PNGs, or check
//! against golden reference PNGs.
//!
//! This is the first runnable end-to-end slice: `de-sim -> de-ui -> de-paint`.

mod golden;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use de_api::Size;

#[derive(Debug, Clone, PartialEq)]
enum Mode {
    Render,
    Check(PathBuf),
    Bless(PathBuf),
}

#[derive(Debug, PartialEq)]
struct Args {
    scene: String,
    width: u32,
    height: u32,
    out: PathBuf,
    out_dir: Option<PathBuf>,
    list: bool,
    mode: Mode,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            scene: "brand".to_string(),
            width: 720,
            height: 1600,
            out: PathBuf::new(),
            out_dir: None,
            list: false,
            mode: Mode::Render,
        }
    }
}

fn parse_args<I: IntoIterator<Item = String>>(args: I) -> Result<Args, String> {
    let mut parsed = Args::default();
    let mut scene_set = false;
    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--scene" => {
                parsed.scene = it.next().ok_or("--scene needs a value")?;
                scene_set = true;
            }
            "--width" => {
                parsed.width = it
                    .next()
                    .ok_or("--width needs a value")?
                    .parse()
                    .map_err(|_| "--width must be a number")?
            }
            "--height" => {
                parsed.height = it
                    .next()
                    .ok_or("--height needs a value")?
                    .parse()
                    .map_err(|_| "--height must be a number")?
            }
            "--out" => parsed.out = PathBuf::from(it.next().ok_or("--out needs a value")?),
            "--out-dir" => {
                parsed.out_dir = Some(PathBuf::from(it.next().ok_or("--out-dir needs a value")?))
            }
            "--check" => {
                if !matches!(parsed.mode, Mode::Render) {
                    return Err("--check and --bless are mutually exclusive".to_string());
                }
                parsed.mode = Mode::Check(PathBuf::from(it.next().ok_or("--check needs a value")?));
            }
            "--bless" => {
                if !matches!(parsed.mode, Mode::Render) {
                    return Err("--check and --bless are mutually exclusive".to_string());
                }
                parsed.mode = Mode::Bless(PathBuf::from(it.next().ok_or("--bless needs a value")?));
            }
            "--list-scenes" => parsed.list = true,
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    if parsed.width == 0 || parsed.height == 0 {
        return Err("width and height must be non-zero".to_string());
    }
    if !matches!(parsed.mode, Mode::Render) && !scene_set {
        parsed.scene = "all".to_string();
    }
    if parsed.scene == "all" && !parsed.out.as_os_str().is_empty() {
        return Err("--out cannot be used with --scene all; use --out-dir".to_string());
    }
    if parsed.out.as_os_str().is_empty() {
        if let Some(dir) = &parsed.out_dir {
            parsed.out = dir.join(format!("{}.png", parsed.scene));
        } else {
            parsed.out = PathBuf::from(format!("build/de/{}.png", parsed.scene));
        }
    }
    Ok(parsed)
}

fn resolve_scenes(name: &str) -> Result<Vec<&'static str>, String> {
    if name == "all" {
        return Ok(de_ui::SCENES.to_vec());
    }
    de_ui::SCENES
        .iter()
        .copied()
        .find(|&s| s == name)
        .map(|s| vec![s])
        .ok_or_else(|| {
            format!(
                "unknown scene '{}' (known: {})",
                name,
                de_ui::SCENES.join(", ")
            )
        })
}

fn render_to_dir(
    paint: &mut de_paint::Renderer,
    scenes: &[&str],
    size: Size,
    dir: &Path,
) -> Result<(), String> {
    for &name in scenes {
        let scene = de_ui::scene(name, size)
            .ok_or_else(|| format!("unknown scene '{name}'"))?;
        let pixmap = paint.render(&scene);
        let out = dir.join(format!("{name}.png"));
        de_paint::save_png(&pixmap, &out)?;
        println!(
            "{{\"scene\":\"{}\",\"width\":{},\"height\":{},\"out\":\"{}\"}}",
            name,
            size.w,
            size.h,
            out.display()
        );
    }
    Ok(())
}

fn check_golden(
    paint: &mut de_paint::Renderer,
    scenes: &[&str],
    size: Size,
    golden_dir: &Path,
) -> bool {
    let mut all_ok = true;
    for &name in scenes {
        let Some(scene) = de_ui::scene(name, size) else {
            eprintln!("FAIL {name}: unknown scene");
            all_ok = false;
            continue;
        };
        let pixmap = paint.render(&scene);
        let golden_path = golden_dir.join(format!("{name}.png"));
        match golden::compare_against_png(&pixmap, &golden_path) {
            Ok(report) => {
                let pct = report.differing_ratio * 100.0;
                if report.passed() {
                    println!(
                        "PASS {name} (diff={pct:.4}%, mae={:.4}/255)",
                        report.mae_255
                    );
                } else {
                    eprintln!(
                        "FAIL {name} (diff={pct:.4}% [{}/{}], mae={:.4}/255)",
                        report.differing_pixels, report.total_pixels, report.mae_255
                    );
                    all_ok = false;
                }
            }
            Err(err) => {
                eprintln!("FAIL {name}: {err}");
                all_ok = false;
            }
        }
    }
    all_ok
}

fn main() -> ExitCode {
    let args = match parse_args(std::env::args().skip(1)) {
        Ok(args) => args,
        Err(err) => {
            eprintln!("de-sim: {err}");
            return ExitCode::from(2);
        }
    };

    if args.list {
        for name in de_ui::SCENES {
            println!("{name}");
        }
        return ExitCode::SUCCESS;
    }

    let scenes = match resolve_scenes(&args.scene) {
        Ok(scenes) => scenes,
        Err(err) => {
            eprintln!("de-sim: {err}");
            return ExitCode::from(2);
        }
    };

    let size = Size::new(args.width, args.height);
    let mut paint = de_paint::Renderer::new();

    match &args.mode {
        Mode::Check(golden_dir) => {
            if check_golden(&mut paint, &scenes, size, golden_dir) {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Mode::Bless(golden_dir) => {
            if let Err(err) = render_to_dir(&mut paint, &scenes, size, golden_dir) {
                eprintln!("de-sim: {err}");
                return ExitCode::FAILURE;
            }
            ExitCode::SUCCESS
        }
        Mode::Render => {
            if args.scene == "all" {
                let dir = args
                    .out_dir
                    .unwrap_or_else(|| PathBuf::from("build/de"));
                if let Err(err) = render_to_dir(&mut paint, &scenes, size, &dir) {
                    eprintln!("de-sim: {err}");
                    return ExitCode::FAILURE;
                }
                ExitCode::SUCCESS
            } else {
                let scene = de_ui::scene(&args.scene, size).expect("resolved scene exists");
                let pixmap = paint.render(&scene);
                if let Err(err) = de_paint::save_png(&pixmap, &args.out) {
                    eprintln!("de-sim: {err}");
                    return ExitCode::FAILURE;
                }
                println!(
                    "{{\"scene\":\"{}\",\"width\":{},\"height\":{},\"out\":\"{}\"}}",
                    args.scene,
                    args.width,
                    args.height,
                    args.out.display()
                );
                ExitCode::SUCCESS
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_plan_panel() {
        let args = parse_args(std::iter::empty()).unwrap();
        assert_eq!(args.scene, "brand");
        assert_eq!((args.width, args.height), (720, 1600));
        assert_eq!(args.out, PathBuf::from("build/de/brand.png"));
    }

    #[test]
    fn scene_names_the_default_output() {
        let args = parse_args(["--scene", "lock"].into_iter().map(String::from)).unwrap();
        assert_eq!(args.out, PathBuf::from("build/de/lock.png"));
    }

    #[test]
    fn parses_scene_and_out() {
        let args = parse_args(
            ["--scene", "brand", "--out", "/tmp/x.png"]
                .into_iter()
                .map(String::from),
        )
        .unwrap();
        assert_eq!(args.out, PathBuf::from("/tmp/x.png"));
    }

    #[test]
    fn parses_scene_all_and_out_dir() {
        let args = parse_args(
            ["--scene", "all", "--out-dir", "/tmp/golden"]
                .into_iter()
                .map(String::from),
        )
        .unwrap();
        assert_eq!(args.scene, "all");
        assert_eq!(args.out_dir, Some(PathBuf::from("/tmp/golden")));
    }

    #[test]
    fn check_and_bless_default_to_all_scenes() {
        let check = parse_args(
            ["--check", "/tmp/golden"].into_iter().map(String::from),
        )
        .unwrap();
        assert_eq!(check.scene, "all");
        assert_eq!(check.mode, Mode::Check(PathBuf::from("/tmp/golden")));

        let bless = parse_args(
            ["--bless", "/tmp/golden"].into_iter().map(String::from),
        )
        .unwrap();
        assert_eq!(bless.scene, "all");
        assert_eq!(bless.mode, Mode::Bless(PathBuf::from("/tmp/golden")));
    }

    #[test]
    fn rejects_conflicting_flags() {
        assert!(parse_args(
            ["--scene", "all", "--out", "/tmp/x.png"]
                .into_iter()
                .map(String::from)
        )
        .is_err());
        assert!(parse_args(
            ["--check", "/tmp/a", "--bless", "/tmp/b"]
                .into_iter()
                .map(String::from)
        )
        .is_err());
    }

    #[test]
    fn rejects_unknown_argument() {
        assert!(parse_args(["--nope".to_string()]).is_err());
    }
}
