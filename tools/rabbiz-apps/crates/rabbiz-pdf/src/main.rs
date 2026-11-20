//! RabbizOS PDF — native PDF reader on `hayro` (pure-Rust CPU rasterizer,
//! no poppler/mupdf on the rootfs).
//!
//! Screens: file picker (~/Documents, ~/Downloads, ~, ~/Desktop) → page
//! viewer (fit-width, double-tap zoom, drag pan, swipe paging, prev/next +
//! zoom buttons) → page overview grid. Rendering is queued one page per
//! event/tick so the software rasterizer never blocks a frame for long.

mod app;
mod doc;
mod headless;
mod view;

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    // Headless modes run before the GUI is touched at all — they must win
    // over the `%f` file-arg path below.
    if argv.iter().any(|a| a == "--mcp") {
        headless::serve_mcp();
    }
    if let Some(i) = argv.iter().position(|a| a == "--cli") {
        std::process::exit(headless::run_cli(&argv[i + 1..]));
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
        // `rabbiz-pdf <file.pdf>`: the desktop entry passes the document
        // via `%f`; the Wayland runner does not consume argv, so grab it
        // here before `run_app`.
        Some(_) if argv.len() == 1 => {
            let initial = doc::file_arg(argv.iter().map(String::as_str));
            app::run(initial);
        }
        Some(_) => {
            eprintln!("error: too many arguments\n");
            eprint!("{}", headless::help_text());
            std::process::exit(2);
        }
        None => {
            app::run(None);
        }
    }
}
