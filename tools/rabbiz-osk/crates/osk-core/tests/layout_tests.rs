use osk_core::layout::Layout;
use osk_core::KeyAction;

fn en() -> Layout {
    Layout::parse(include_str!("../../../data/layouts/en-us.toml")).expect("en-us parses")
}

fn th() -> Layout {
    Layout::parse(include_str!("../../../data/layouts/th-kedmanee.toml")).expect("th parses")
}

#[test]
fn english_layout_has_the_gboard_rows() {
    let layout = en();
    assert_eq!(layout.language, "en");
    let base = layout.layer("base").expect("base layer");
    assert_eq!(base.rows.len(), 5);
    assert_eq!(base.rows[0].len(), 10, "number row");
    assert_eq!(base.rows[1].len(), 10, "qwerty row");
    assert_eq!(base.rows[2].len(), 9, "asdf row");
    assert_eq!(base.rows[2][0].action, KeyAction::Text("a".into()));
    assert_eq!(base.insets[2], 0.5, "asdf row is inset half a key");
    let bottom = &base.rows[4];
    let space = bottom
        .iter()
        .find(|k| k.action == KeyAction::Space)
        .expect("space key");
    assert!((space.width - 4.5).abs() < f32::EPSILON);
}

#[test]
fn thai_layout_keeps_the_kedmanee_rows() {
    let layout = th();
    assert_eq!(layout.language, "th");
    let base = layout.layer("base").expect("base layer");
    assert_eq!(base.rows.len(), 5, "Gboard Thai has no separate number row");
    assert_eq!(base.rows[0].len(), 12, "Kedmanee ๅ row");
    assert_eq!(
        base.rows[0][0].action,
        KeyAction::Text("ๅ".into()),
        "the top row is the letter row, as in Gboard Thai"
    );
    assert_eq!(
        base.rows[0][0].shifted.as_deref(),
        Some("1"),
        "the Arabic digit is the shift hint on that row"
    );
    assert_eq!(layout.display_label(&base.rows[0][0], true), "1");
    assert_eq!(layout.display_label(&base.rows[0][0], false), "ๅ");
    assert_eq!(
        layout.layer("numbers").unwrap().rows[0][0].action,
        KeyAction::Text("๑".into()),
        "Thai digits live on the ?123 page"
    );
    assert_eq!(base.rows[1][0].action, KeyAction::Text("ๆ".into()));
    // Thai alternates are attached to the character keys.
    let a = base
        .rows
        .iter()
        .flat_map(|r| r.iter())
        .find(|k| k.action == KeyAction::Text("า".into()))
        .expect("า key");
    assert_eq!(a.alternates, vec!["้", "่", "๊", "๋"]);
}

#[test]
fn purpose_layouts_switch_language_with_an_icon_not_an_emoji() {
    let layout = en();
    for layer in ["email", "url", "num"] {
        let keys = layout.layer(layer).expect(layer);
        let has_language = keys.rows.iter().flat_map(|row| row.iter()).any(|key| {
            key.action == KeyAction::Language && key.icon.as_deref() == Some("language")
        });
        assert!(has_language, "{layer} keeps the globe key");
        let commits_globe = keys
            .rows
            .iter()
            .flat_map(|row| row.iter())
            .any(|key| key.action == KeyAction::Text("🌐".into()));
        assert!(!commits_globe, "{layer} must not type a globe emoji");
    }
}

#[test]
fn rare_thai_letters_live_on_long_press() {
    let layout = th();
    let base = layout.layer("base").unwrap();
    let lookup = |label: &str| {
        base.rows
            .iter()
            .flat_map(|row| row.iter())
            .find(|k| k.label == label)
            .unwrap_or_else(|| panic!("no key {label}"))
            .alternates
            .clone()
    };
    assert_eq!(lookup("ฟ"), vec!["ฤ"]);
    assert_eq!(lookup("ห"), vec!["์", "ฆ"]);
    assert_eq!(lookup("ก"), vec!["๊", "๋", "ฏ"]);
    assert_eq!(lookup("ด"), vec!["็", "ฎ"]);
    assert_eq!(lookup("ส"), vec!["์", "ษ"]);
    // ซ has no base key on Kedmanee (it is Shift+ช), so it rides on ช with ศ.
    assert_eq!(lookup("ช"), vec!["ํ", "ซ", "ศ"]);
}

#[test]
fn key_rects_stay_inside_the_surface_without_overlap() {
    for layout in [en(), th()] {
        for layer in &layout.layers {
            let rows = layout.row_rects(&layer.name).expect("geometry");
            assert_eq!(rows.len(), layer.rows.len());
            for (r, row) in rows.iter().enumerate() {
                let first = row.first().expect("row start").0;
                let last = row.last().expect("row end").0;
                assert!(first.x >= 0.0, "{} row {r} starts left of 0", layer.name);
                assert!(
                    last.x + last.w <= layout.geometry.width_px + 0.01,
                    "{} row {r} overflows the right edge",
                    layer.name
                );
                for pair in row.windows(2) {
                    assert!(
                        pair[0].0.x + pair[0].0.w <= pair[1].0.x + 0.01,
                        "{} row {r} keys overlap",
                        layer.name
                    );
                }
            }
        }
    }
}

#[test]
fn every_row_fills_the_width() {
    let layout = en();
    for layer in &layout.layers {
        let rows = layout.row_rects(&layer.name).expect("geometry");
        for row in &rows {
            let first = row.first().unwrap().0;
            let last = row.last().unwrap().0;
            let right = last.x + last.w;
            // An inset narrows the row symmetrically, so the right edge mirrors the left.
            let inset_px = first.x - layout.geometry.margin_px;
            let expected = layout.geometry.width_px - layout.geometry.margin_px - inset_px;
            assert!(
                (right - expected).abs() < 0.5,
                "row ends at {right}, expected {expected}"
            );
        }
    }
}

#[test]
fn hit_test_finds_the_key_and_misses_the_gap() {
    let layout = en();
    let rows = layout.row_rects("base").unwrap();
    let (rect, pos) = rows[1][4];
    let hit = layout.hit_test("base", rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
    assert_eq!(hit, Some(pos));
    assert_eq!(
        layout.hit_test("base", 1.0, 1.0),
        None,
        "the candidate strip is not a key"
    );
    assert_eq!(layout.hit_test("no-such-layer", 10.0, 10.0), None);
}

#[test]
fn display_label_uppercases_characters_only_when_shifted() {
    let layout = en();
    let base = layout.layer("base").unwrap();
    let q = &base.rows[1][0];
    assert_eq!(layout.display_label(q, false), "q");
    assert_eq!(layout.display_label(q, true), "Q");
    let shift = base.rows[3][0].clone();
    assert_eq!(shift.icon.as_deref(), Some("keyboard_capslock"));
    assert_eq!(layout.display_label(&shift, true), "shift");
}

#[test]
fn unknown_layer_geometry_is_an_error_not_a_panic() {
    let layout = en();
    assert!(layout.row_rects("nope").is_err());
    assert!(layout.row_rects("emoji").is_ok());
}

#[test]
fn malformed_layouts_are_rejected() {
    let missing_action = r#"
id = "x"
name = "x"
language = "en"
[geometry]
width_px = 720.0
key_height_px = 52.0
[keys.bad]
action = "teleport"
[[layer]]
name = "base"
rows = ["bad"]
"#;
    assert!(Layout::parse(missing_action).is_err());

    let no_layers = r#"
id = "x"
name = "x"
language = "en"
[geometry]
width_px = 720.0
key_height_px = 52.0
"#;
    assert!(Layout::parse(no_layers).is_err());
}
