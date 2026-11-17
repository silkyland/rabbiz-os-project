//! Host simulator: build one screen scene and write it as a PNG.
//!
//! This is the first runnable end-to-end slice: `de-sim -> de-ui -> de-paint`.
//! A windowed runner arrives with M0's host work; the PNG path is what the
//! seam tests and the Stranger probe use.

use std::path::PathBuf;
use std::process::ExitCode;

use de_api::{Scene, Size};

/// Scenes this host runner can build. `--scene` accepts exactly these names.
const SCENES: [&str; 2] = ["brand", "lock"];

#[derive(Debug, PartialEq)]
struct Args {
    scene: String,
    width: u32,
    height: u32,
    out: PathBuf,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            scene: "brand".to_string(),
            width: 720,
            height: 1600,
            out: PathBuf::from("build/de/brand.png"),
        }
    }
}

fn parse_args<I: IntoIterator<Item = String>>(args: I) -> Result<Args, String> {
    let mut parsed = Args::default();
    let mut out = None;
    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--scene" => parsed.scene = it.next().ok_or("--scene needs a value")?,
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
            "--out" => out = Some(PathBuf::from(it.next().ok_or("--out needs a value")?)),
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    if parsed.width == 0 || parsed.height == 0 {
        return Err("width and height must be non-zero".to_string());
    }
    // `--out` wins; otherwise the file is named after the scene, so
    // `--scene lock` never overwrites `brand.png`.
    parsed.out = out.unwrap_or_else(|| PathBuf::from(format!("build/de/{}.png", parsed.scene)));
    Ok(parsed)
}

/// Build the named screen at `size`. Unknown names are an error, never a
/// silent fallback to the boot frame.
fn build_scene(name: &str, size: Size) -> Result<Scene, String> {
    match name {
        "brand" => Ok(de_ui::boot_frame(size)),
        "lock" => Ok(de_ui::lock_frame(size)),
        other => Err(format!(
            "unknown scene '{other}' (expected one of: {})",
            SCENES.join(", ")
        )),
    }
}

fn main() -> ExitCode {
    let args = match parse_args(std::env::args().skip(1)) {
        Ok(args) => args,
        Err(err) => {
            eprintln!("de-sim: {err}");
            return ExitCode::from(2);
        }
    };

    let scene = match build_scene(&args.scene, Size::new(args.width, args.height)) {
        Ok(scene) => scene,
        Err(err) => {
            eprintln!("de-sim: {err}");
            return ExitCode::from(2);
        }
    };
    let mut paint = de_paint::Renderer::new();
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
    fn lock_scene_defaults_to_its_own_out_path() {
        let args = parse_args(["--scene", "lock"].into_iter().map(String::from)).unwrap();
        assert_eq!(args.scene, "lock");
        assert_eq!(args.out, PathBuf::from("build/de/lock.png"));
    }

    #[test]
    fn builds_the_lock_scene_at_the_requested_size() {
        let scene = build_scene("lock", Size::new(720, 1600)).unwrap();
        assert_eq!(scene.size, Size::new(720, 1600));
        assert_eq!(scene.layers.len(), 1);
        assert_eq!(scene.layers[0].name, "lock");
    }

    #[test]
    fn rejects_an_unknown_scene_and_names_the_valid_ones() {
        let err = build_scene("home", Size::new(720, 1600)).unwrap_err();
        assert!(
            err.contains("'home'"),
            "error must quote the bad name: {err}"
        );
        for scene in SCENES {
            assert!(err.contains(scene), "error must list {scene:?}: {err}");
        }
    }

    #[test]
    fn rejects_unknown_argument() {
        assert!(parse_args(["--nope".to_string()]).is_err());
    }
}
