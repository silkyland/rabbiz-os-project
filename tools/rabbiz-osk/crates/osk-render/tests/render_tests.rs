use osk_core::layout::Layout;
use osk_core::state::ShiftState;
use osk_render::{Renderer, Theme};

fn en() -> Layout {
    Layout::parse(include_str!("../../../data/layouts/en-us.toml")).unwrap()
}

fn th() -> Layout {
    Layout::parse(include_str!("../../../data/layouts/th-kedmanee.toml")).unwrap()
}

fn ink(pixmap: &tiny_skia::Pixmap, background: (u8, u8, u8)) -> usize {
    pixmap
        .pixels()
        .iter()
        .filter(|p| (p.red(), p.green(), p.blue()) != background)
        .count()
}

#[test]
fn base_layer_renders_at_the_documented_size() {
    let mut r = Renderer::new();
    let layout = en();
    let p = r.render(&layout, "base", ShiftState::Off, &Theme::dark(), 1.0);
    assert_eq!(p.width(), 720);
    assert_eq!(p.height(), layout.height_px_for("base") as u32);
    assert!(
        ink(&p, Theme::dark().background) > 50_000,
        "keys and labels painted"
    );
}

#[test]
fn every_layer_keeps_the_same_surface_height() {
    let mut r = Renderer::new();
    let layout = en();
    let letters = r.render(&layout, "base", ShiftState::Off, &Theme::dark(), 1.0);
    for layer in ["numbers", "symbols", "emoji"] {
        let other = r.render(&layout, layer, ShiftState::Off, &Theme::dark(), 1.0);
        assert_eq!(
            other.height(),
            letters.height(),
            "layer {layer} must not resize the surface"
        );
    }
}

#[test]
fn shifted_layer_draws_uppercase_letters() {
    // The scene difference between Off and Locked must be non-zero, otherwise the shift FSM
    // would be invisible on screen.
    let mut r = Renderer::new();
    let layout = en();
    let off = r.render(&layout, "base", ShiftState::Off, &Theme::dark(), 1.0);
    let shifted = r.render(&layout, "base", ShiftState::Locked, &Theme::dark(), 1.0);
    assert_ne!(off.data(), shifted.data());
}

#[test]
fn every_layer_of_both_languages_renders_without_panicking() {
    let mut r = Renderer::new();
    for layout in [en(), th()] {
        for layer in &layout.layers {
            let p = r.render(&layout, &layer.name, ShiftState::Off, &Theme::light(), 1.0);
            assert_eq!(p.width(), layout.geometry.width_px as u32);
            assert!(
                ink(&p, Theme::light().background) > 20_000,
                "{} {}",
                layout.id,
                layer.name
            );
        }
    }
}

#[test]
fn snapshots_can_be_written_to_png() {
    let mut r = Renderer::new();
    let layout = en();
    let dir = std::env::temp_dir().join("osk-render-test");
    let path = dir.join("en-us-base.png");
    osk_render::render_to_png(
        &mut r,
        &layout,
        "base",
        ShiftState::Off,
        &Theme::dark(),
        1.0,
        &path,
    )
    .expect("png");
    let bytes = std::fs::read(&path).expect("written file");
    assert!(
        bytes.starts_with(&[0x89, b'P', b'N', b'G']),
        "valid PNG signature"
    );
    assert!(bytes.len() > 10_000, "not a blank image");
    let _ = std::fs::remove_dir_all(dir);
}
