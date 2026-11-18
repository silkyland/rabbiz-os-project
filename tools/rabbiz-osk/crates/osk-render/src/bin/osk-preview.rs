//! `osk-preview` — writes PNG snapshots of the layouts for review.
//!
//! Usage: `cargo run -p osk-render --bin osk-preview -- <out-dir>`
//! Default out-dir is `build/osk` at the repository root.

use osk_core::layout::Layout;
use osk_core::state::ShiftState;
use osk_render::{render_to_png, Renderer, Theme};
use std::path::{Path, PathBuf};

fn main() -> Result<(), String> {
    let out = match std::env::args().nth(1) {
        Some(p) => PathBuf::from(p),
        None => Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../build/osk")
            .canonicalize()
            .unwrap_or_else(|_| PathBuf::from("build/osk")),
    };
    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;

    let layouts = [
        ("en-us", include_str!("../../../../data/layouts/en-us.toml")),
        (
            "th-kedmanee",
            include_str!("../../../../data/layouts/th-kedmanee.toml"),
        ),
    ];
    let mut renderer = Renderer::new();
    let scale = 2.0;
    for (slug, src) in layouts {
        let layout = Layout::parse(src).map_err(|e| e.to_string())?;
        for layer in &layout.layers {
            for (suffix, theme, shift) in [
                ("dark", Theme::dark(), ShiftState::Off),
                ("light", Theme::light(), ShiftState::Off),
                ("dark-shift", Theme::dark(), ShiftState::OneShot),
            ] {
                let path = out.join(format!("{slug}-{}-{suffix}.png", layer.name));
                render_to_png(
                    &mut renderer,
                    &layout,
                    &layer.name,
                    shift,
                    &theme,
                    scale,
                    &path,
                )?;
                println!("{}", path.display());
            }
        }
    }
    Ok(())
}
