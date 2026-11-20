//! RabbizOS Notes — Google Keep-style list + editor per docs/BASE-APPS.md.
//!
//! List: title + preview + edited time, drag-scroll, search field, FAB.
//! Editor: tap-to-position cursor (cosmic-text hit test), grapheme-safe
//! editing, autosave to `~/.local/share/rabbiz-notes/<id>.json` on every
//! mutation (atomic tmp+rename in `store.rs`).
//!
//! Rendering notes: the body is shaped ONCE per (text,width) into a
//! `de_paint::text::shape::Shaped` (cosmic-text, Glyph wrap so Thai breaks
//! anywhere), rasterized per dirty frame, and blitted through a clip rect —
//! `Canvas::blit` has no scissor, so `blit_clipped` emulates one via
//! `blit_region`. The same trick clips list rows to the scroll viewport.

mod app;
mod cli;
mod model;
mod store;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Headless modes run before the Wayland app: `--cli <cmd>` for the text
    // CLI, `--mcp` for the stdio MCP server. Both share `store.rs`.
    if let Some(i) = args.iter().position(|a| a == "--cli") {
        std::process::exit(cli::run_cli(&args[i + 1..]));
    }
    if args.iter().any(|a| a == "--mcp") {
        rabbiz_app_kit::mcp::serve(cli::TOOLS, "rabbiz-notes", "0.1.0");
    }
    rabbiz_app_kit::run_app(app::NotesApp::new());
}
