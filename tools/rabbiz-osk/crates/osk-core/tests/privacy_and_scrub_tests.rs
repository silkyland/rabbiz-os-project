use osk_core::{
    Command, InputPurpose, Keyboard, Layout, PrivacyPolicy, ScrubStep, SpacebarScrubber,
    DEFAULT_STEP_PX,
};

fn en() -> Layout {
    Layout::parse(include_str!("../../../data/layouts/en-us.toml")).unwrap()
}

fn pos_of(layout: &Layout, layer: &str, label: &str) -> osk_core::KeyPos {
    let l = layout.layer(layer).unwrap();
    for (r, row) in l.rows.iter().enumerate() {
        for (c, key) in row.iter().enumerate() {
            if key.label == label {
                return osk_core::KeyPos { row: r, col: c };
            }
        }
    }
    panic!("key `{label}` not found in layer `{layer}`");
}

#[test]
fn pin_purpose_maps_from_protocol_and_opens_num_layer() {
    assert_eq!(InputPurpose::from_protocol(9), InputPurpose::Pin);

    let mut kb = Keyboard::new(vec![en()]).unwrap();
    kb.set_purpose(InputPurpose::Pin);
    assert_eq!(kb.layer_name(), "num");
    assert!(!kb.autocapitalise());
    assert!(kb.privacy_policy().is_sensitive());
    assert!(!kb.privacy_policy().allow_predictions());
    assert!(!kb.privacy_policy().allow_learning());
    assert!(!kb.privacy_policy().allow_key_popup());
    assert!(!kb.privacy_policy().allow_clipboard_strip());
}

#[test]
fn password_mode_suppresses_suggestions_and_learning() {
    let layout = en();
    let mut kb = Keyboard::new(vec![layout.clone()]).unwrap();
    kb.set_purpose(InputPurpose::Password);
    assert_eq!(
        kb.privacy_policy(),
        PrivacyPolicy::from_purpose(InputPurpose::Password)
    );

    // Type a unique secret token followed by a space; must not be learned or suggested.
    for ch in ["z", "x", "q", "v", "k"] {
        kb.press(pos_of(&layout, "base", ch), 0);
    }
    assert!(kb.suggestions().is_empty());
    kb.press(pos_of(&layout, "base", "English"), 10);
    assert!(kb.suggestions().is_empty());

    // Switch back to Normal and verify the secret word was never learned.
    kb.set_purpose(InputPurpose::Normal);
    kb.reset_for_activation();
    kb.insert_text("zxqv");
    assert!(
        !kb.suggestions().iter().any(|s| s == "zxqvk"),
        "password text must never be learned by predictor"
    );
}

#[test]
fn spacebar_scrubber_drives_keyboard_cursor_commands() {
    assert_eq!(DEFAULT_STEP_PX, 22.0);
    let mut kb = Keyboard::new(vec![en()]).unwrap();
    let mut scrubber = SpacebarScrubber::new();

    scrubber.on_down(100.0);
    let steps = scrubber.on_move(150.0);
    assert_eq!(steps, vec![ScrubStep::Right, ScrubStep::Right]);
    let delta: i32 = steps.iter().map(|s| s.delta()).sum();
    assert_eq!(kb.space_swipe(delta), vec![Command::MoveCursor(2)]);

    let back = scrubber.on_move(100.0);
    assert_eq!(back, vec![ScrubStep::Left, ScrubStep::Left]);
    let back_delta: i32 = back.iter().map(|s| s.delta()).sum();
    assert_eq!(kb.space_swipe(back_delta), vec![Command::MoveCursor(-2)]);

    assert!(scrubber.on_up(), "scrub gesture must suppress Space");
}
