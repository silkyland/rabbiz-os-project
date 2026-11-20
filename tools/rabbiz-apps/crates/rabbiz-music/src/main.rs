//! rabbiz-music — Music app for RabbizOS (Rust/native Wayland build).
//!
//! Library: audio files under `~/Music` + `~/Downloads` (see `library.rs`).
//! Playback: one persistent `mpv --idle=yes --input-ipc-server` process
//! driven over JSON-lines IPC (see `player.rs` and `playback.rs`) — real
//! position, duration, pause and end-of-file state, real `seek`, real
//! transport. Volume follows the PipeWire/PulseAudio default sink
//! (`--ao=pulse,pipewire`); the app never touches it.
//!
//! Layout follows Material 3 media-player conventions on a phone: scrolling
//! track list, now-playing card pinned above the gesture bar with art tile,
//! seek bar (drag + rubber band), elapsed/total times and prev/play/next
//! transport. Hit-testing uses the last rendered size (`vp`), never a
//! hardcoded panel size (see `app.rs` and `render.rs`).

mod app;
mod headless;
mod library;
mod playback;
mod player;
mod render;

pub(crate) use playback::Player;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--mcp") {
        headless::serve_mcp();
    }
    if let Some(tokens) = headless::cli_tokens(&args) {
        match headless::parse_cli(&tokens) {
            Ok(cli) => std::process::exit(headless::run_cli(&cli)),
            Err(e) => {
                eprintln!("rabbiz-music: {e}");
                eprintln!("{}", headless::HELP);
                std::process::exit(2);
            }
        }
    }
    rabbiz_app_kit::run_app(app::MusicApp::new());
}
