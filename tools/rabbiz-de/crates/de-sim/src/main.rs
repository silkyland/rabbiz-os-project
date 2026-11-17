//! Host simulator: build one screen scene and write it as a PNG.
//!
//! This is the first runnable end-to-end slice: `de-sim -> de-ui -> de-paint`.
//! A windowed runner arrives with M0's host work; the PNG path is what the
//! seam tests and the Stranger probe use.

use std::path::PathBuf;
use std::process::ExitCode;

use de_api::Size;

#[derive(Debug, PartialEq)]
struct Args {
    scene: String,
    width: u32,
    height: u32,
    out: PathBuf,
    list: bool,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            scene: "brand".to_string(),
            width: 720,
            height: 1600,
            out: PathBuf::new(),
            list: false,
        }
    }
}

fn parse_args<I: IntoIterator<Item = String>>(args: I) -> Result<Args, String> {
    let mut parsed = Args::default();
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
            "--out" => parsed.out = PathBuf::from(it.next().ok_or("--out needs a value")?),
            "--list-scenes" => parsed.list = true,
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    if parsed.width == 0 || parsed.height == 0 {
        return Err("width and height must be non-zero".to_string());
    }
    if parsed.out.as_os_str().is_empty() {
        parsed.out = PathBuf::from(format!("build/de/{}.png", parsed.scene));
    }
    Ok(parsed)
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

    let Some(scene) = de_ui::scene(&args.scene, Size::new(args.width, args.height)) else {
        eprintln!(
            "de-sim: unknown scene '{}' (known: {})",
            args.scene,
            de_ui::SCENES.join(", ")
        );
        return ExitCode::from(2);
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
    fn rejects_unknown_argument() {
        assert!(parse_args(["--nope".to_string()]).is_err());
    }
}
