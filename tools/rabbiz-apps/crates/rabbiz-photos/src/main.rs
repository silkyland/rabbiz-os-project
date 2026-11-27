//! RabbizOS Photos — grid of thumbnails from ~/Pictures + ~/Downloads with a
//! full-screen viewer (double-tap zoom, drag pan, swipe paging).
//!
//! Decode path: `de-paint`'s resvg pipeline (zune-jpeg / image-webp / gif /
//! PNG) wrapped in a one-image SVG — see the app-kit raster image module. Thumbnails decode
//! lazily into a bounded cache; the viewer decodes on demand through the
//! same pump so the UI never blocks on a 12 MP JPEG.

mod app;
use rabbiz_app_kit::raster_image as decode;
mod headless;
mod layout;
mod photos;
mod viewer;

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    // Headless modes run before the GUI is touched at all.
    if argv.iter().any(|a| a == "--mcp") {
        headless::serve_mcp();
    }
    if argv.first().map(String::as_str) == Some("--cli") {
        std::process::exit(headless::run_cli(&argv[1..]));
    }
    match argv.first().map(String::as_str) {
        Some("-h") | Some("--help") | Some("help") => {
            print!("{}", headless::help_text());
        }
        Some(flag) if flag.starts_with('-') => {
            eprintln!("error: unknown flag '{flag}'\n");
            eprint!("{}", headless::help_text());
            std::process::exit(2);
        }
        // `rabbiz-photos <file-or-dir>`: headless open/list contract.
        Some(path) if argv.len() == 1 => {
            std::process::exit(headless::run_path_arg(std::path::Path::new(path)));
        }
        Some(_) => {
            eprintln!("error: too many arguments\n");
            eprint!("{}", headless::help_text());
            std::process::exit(2);
        }
        None => {
            app::run();
        }
    }
}
