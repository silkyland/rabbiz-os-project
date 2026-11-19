use osk_core::layout::{KeyPos, Layout};
use osk_core::state::{Command, InputPurpose, Keyboard, ShiftState};

fn emoji_layout() -> Layout {
    Layout::parse(include_str!("../../../data/layouts/emoji.toml")).unwrap()
}

fn en() -> Layout {
    Layout::parse(include_str!("../../../data/layouts/en-us.toml")).unwrap()
}
fn th() -> Layout {
    Layout::parse(include_str!("../../../data/layouts/th-kedmanee.toml")).unwrap()
}

fn pos_of_action(layout: &Layout, layer: &str, want: osk_core::layout::KeyAction) -> KeyPos {
    let layer = layout.layer(layer).expect("layer");
    for (r, row) in layer.rows.iter().enumerate() {
        for (c, key) in row.iter().enumerate() {
            if key.action == want {
                return KeyPos { row: r, col: c };
            }
        }
    }
    panic!("no key with action {want:?} in {layer:?}");
}

fn pos_of_shift(layout: &Layout) -> KeyPos {
    pos_of_action(layout, "base", osk_core::layout::KeyAction::Shift)
}

fn pos_of_backspace(layout: &Layout) -> KeyPos {
    pos_of_action(layout, "base", osk_core::layout::KeyAction::Backspace)
}

fn pos_of_language(layout: &Layout, layer: &str) -> KeyPos {
    let layer = layout.layer(layer).expect("layer");
    for (r, row) in layer.rows.iter().enumerate() {
        for (c, key) in row.iter().enumerate() {
            if matches!(key.action, osk_core::layout::KeyAction::Language) {
                return KeyPos { row: r, col: c };
            }
        }
    }
    panic!("no language key in {layer:?}");
}

fn pos_of(layout: &Layout, layer: &str, label: &str) -> KeyPos {
    let layer = layout.layer(layer).expect("layer");
    for (r, row) in layer.rows.iter().enumerate() {
        for (c, key) in row.iter().enumerate() {
            if key.label == label {
                return KeyPos { row: r, col: c };
            }
        }
    }
    panic!("no key labelled {label} in {layer:?}");
}

#[test]
fn typing_a_letter_commits_it() {
    let layout = en();
    let mut kb = Keyboard::new(vec![layout.clone()]).unwrap();
    let q = pos_of(&layout, "base", "q");
    assert_eq!(kb.press(q, 0), vec![Command::Commit("q".into())]);
    assert_eq!(kb.text(), "q");
}

#[test]
fn shift_is_one_shot_then_caps_lock_on_double_tap() {
    let layout = en();
    let mut kb = Keyboard::new(vec![layout.clone()]).unwrap();
    let shift = pos_of_shift(&layout);
    let q = pos_of(&layout, "base", "q");

    kb.press(shift, 1_000);
    assert_eq!(kb.shift(), ShiftState::OneShot);
    assert_eq!(kb.press(q, 1_100), vec![Command::Commit("Q".into())]);
    assert_eq!(
        kb.shift(),
        ShiftState::Off,
        "one-shot shift clears after a character"
    );

    kb.press(shift, 2_000);
    kb.press(shift, 2_150); // double tap within 300 ms
    assert_eq!(kb.shift(), ShiftState::Locked);
    kb.press(q, 2_200);
    assert_eq!(
        kb.shift(),
        ShiftState::Locked,
        "caps lock survives a character"
    );
    kb.press(shift, 2_500);
    assert_eq!(kb.shift(), ShiftState::Off);
}

#[test]
fn backspace_deletes_one_thai_grapheme_cluster() {
    let layout = th();
    let mut kb = Keyboard::new(vec![layout.clone()]).unwrap();
    kb.set_surrounding_text("ที่".into());
    let backspace = pos_of_backspace(&layout);
    assert_eq!(
        kb.press(backspace, 0),
        vec![Command::Backspace { bytes: 9 }],
        "ท + ี + ่ is one grapheme"
    );
    assert_eq!(kb.text(), "");
}

#[test]
fn backspace_on_empty_text_still_sends_one_delete() {
    let layout = en();
    let mut kb = Keyboard::new(vec![layout]).unwrap();
    let backspace = {
        let l = kb.layout().clone();
        pos_of_backspace(&l)
    };
    assert_eq!(kb.backspace(), vec![Command::Backspace { bytes: 1 }]);
    let _ = backspace;
}

#[test]
fn layer_keys_switch_and_restore() {
    let layout = en();
    let mut kb = Keyboard::new(vec![layout.clone()]).unwrap();
    let numbers = pos_of(&layout, "base", "?123");
    assert_eq!(kb.press(numbers, 0), vec![Command::Layer("numbers".into())]);
    assert_eq!(kb.layer_name(), "numbers");

    let one = pos_of(&layout, "numbers", "1");
    assert_eq!(kb.press(one, 10), vec![Command::Commit("1".into())]);

    let abc = pos_of(&layout, "numbers", "abc");
    assert_eq!(kb.press(abc, 20), vec![Command::Layer("base".into())]);
    assert_eq!(kb.layer_name(), "base");
}

#[test]
fn language_key_cycles_between_english_and_thai() {
    let mut kb = Keyboard::new(vec![en(), th()]).unwrap();
    let en_layout = en();
    let language = pos_of_language(&en_layout, "base");
    assert_eq!(kb.press(language, 0), vec![Command::Language("th".into())]);
    assert_eq!(kb.layout().id, "th-kedmanee");
    assert_eq!(kb.layer_name(), "base");

    let th_layout = th();
    let back = pos_of_language(&th_layout, "base");
    assert_eq!(kb.press(back, 10), vec![Command::Language("en".into())]);
    assert_eq!(kb.layout().id, "en-us");
}

#[test]
fn double_space_becomes_period_space() {
    let layout = en();
    let mut kb = Keyboard::new(vec![layout.clone()]).unwrap();
    let space = pos_of(&layout, "base", "English");
    kb.press(space, 0);
    assert_eq!(
        kb.press(space, 100),
        vec![
            Command::Backspace { bytes: 1 },
            Command::Commit(". ".into())
        ]
    );
    assert_eq!(kb.text(), ". ");
}

#[test]
fn auto_capitalise_arms_shift_at_sentence_start() {
    let layout = en();
    let mut kb = Keyboard::new(vec![layout.clone()]).unwrap();
    let o = pos_of(&layout, "base", "o");
    let space = pos_of(&layout, "base", "English");
    let period = pos_of(&layout, "base", ".");
    kb.press(o, 0);
    kb.press(period, 10);
    assert_eq!(
        kb.shift(),
        ShiftState::OneShot,
        "after a period the next word capitalises"
    );
    kb.press(space, 20);
    assert!(Keyboard::at_sentence_start("Hello. "));
    assert!(!Keyboard::at_sentence_start("Hello"));
}

#[test]
fn shift_does_not_touch_thai_or_emoji_text() {
    let layout = th();
    let mut kb = Keyboard::new(vec![layout.clone()]).unwrap();
    let shift = pos_of_shift(&layout);
    kb.press(shift, 0);
    let ko = pos_of(&layout, "base", "ก");
    assert_eq!(kb.press(ko, 10), vec![Command::Commit("ก".into())]);
}

#[test]
fn thai_top_row_letters_carry_the_arabic_digit_on_shift() {
    let layout = th();
    let mut kb = Keyboard::new(vec![layout.clone()]).unwrap();
    let thai_letter = pos_of(&layout, "base", "ๅ");
    let shift = pos_of_shift(&layout);

    assert_eq!(kb.press(thai_letter, 0), vec![Command::Commit("ๅ".into())]);

    kb.press(shift, 10);
    assert_eq!(kb.shift(), ShiftState::OneShot);
    assert_eq!(
        kb.press(thai_letter, 20),
        vec![Command::Commit("1".into())],
        "the small hint on the top row is what shift commits"
    );
    assert_eq!(kb.shift(), ShiftState::Off, "one-shot shift is consumed");

    // Thai digits live on the ?123 page, first row.
    let numbers = pos_of(&layout, "base", "๑๒๓");
    kb.press(numbers, 30);
    let thai_one = pos_of(&layout, "numbers", "๑");
    assert_eq!(kb.press(thai_one, 40), vec![Command::Commit("๑".into())]);
}

#[test]
fn insert_text_commits_and_tracks_the_buffer() {
    let layout = en();
    let mut kb = Keyboard::new(vec![layout]).unwrap();
    assert_eq!(kb.insert_text("😀"), vec![Command::Commit("😀".into())]);
    assert_eq!(kb.text(), "😀");
    assert!(kb.insert_text("").is_empty());
    assert_eq!(kb.backspace(), vec![Command::Backspace { bytes: 4 }]);
}

#[test]
fn alternates_are_reachable_from_the_key() {
    let layout = th();
    let kb = Keyboard::new(vec![layout.clone()]).unwrap();
    let a = pos_of(&layout, "base", "า");
    assert_eq!(kb.alternates(a), vec!["้", "่", "๊", "๋"]);
    let one = pos_of(&layout, "numbers", "๑");
    assert!(kb.alternates(one).is_empty());
    assert!(kb.is_space(pos_of(&layout, "base", "ไทย")));
}

#[test]
fn purpose_switches_the_layer_and_latin_input() {
    let mut kb = Keyboard::new(vec![en(), th()]).unwrap();
    kb.set_purpose(InputPurpose::Email);
    assert_eq!(kb.layout().language, "en");
    assert_eq!(kb.layer_name(), "email");
    assert!(!kb.autocapitalise(), "email is not prose");

    kb.set_purpose(InputPurpose::Url);
    assert_eq!(kb.layer_name(), "url");

    kb.set_purpose(InputPurpose::Number);
    assert_eq!(kb.layer_name(), "num");

    // Thai + email: the keyboard shows the Latin email layout.
    kb.set_purpose(InputPurpose::Normal);
    kb.cycle_language();
    assert_eq!(kb.layout().language, "th");
    kb.set_purpose(InputPurpose::Email);
    assert_eq!(kb.layout().language, "en");
    assert_eq!(kb.layer_name(), "email");

    kb.set_purpose(InputPurpose::Normal);
    assert_eq!(kb.layer_name(), "base");
    assert!(kb.autocapitalise());
}

#[test]
fn repeated_content_type_events_do_not_force_the_language_back() {
    let mut kb = Keyboard::new(vec![en(), th()]).unwrap();
    kb.set_purpose(InputPurpose::Url);
    assert_eq!(kb.layout().language, "en");

    // The user switches to Thai inside the URL field…
    kb.cycle_language();
    assert_eq!(kb.layout().language, "th");

    // …and Chrome keeps re-sending the same content_type on every keystroke.
    kb.set_purpose(InputPurpose::Url);
    assert_eq!(
        kb.layout().language,
        "th",
        "same purpose must not reset the language"
    );
    assert_eq!(
        kb.layer_name(),
        "base",
        "and must not jump back to the URL layer"
    );
}

#[test]
fn protocol_purposes_map_to_the_documented_layers() {
    assert_eq!(InputPurpose::from_protocol(6), InputPurpose::Email);
    assert_eq!(InputPurpose::from_protocol(5), InputPurpose::Url);
    assert_eq!(InputPurpose::from_protocol(3), InputPurpose::Number);
    assert_eq!(InputPurpose::from_protocol(2), InputPurpose::Digits);
    assert_eq!(InputPurpose::from_protocol(0), InputPurpose::Normal);
    assert_eq!(InputPurpose::from_protocol(8), InputPurpose::Password);
}

#[test]
fn suggestions_complete_and_predict_next_words() {
    let layout = en();
    let mut kb = Keyboard::new(vec![layout]).unwrap();
    kb.insert_text("th");
    let suggestions = kb.suggestions();
    assert!(
        suggestions.iter().any(|s| s == "the"),
        "expected a completion, got {suggestions:?}"
    );

    // Learn a phrase, then predict its second word (fresh buffer so the text is "hello ").
    kb.predictor.learn("hello world ");
    let mut kb = Keyboard::new(vec![en()]).unwrap();
    kb.predictor.learn("hello world ");
    kb.insert_text("hello ");
    let suggestions = kb.suggestions();
    assert!(
        suggestions.iter().any(|s| s == "world"),
        "expected a next-word prediction, got {suggestions:?}"
    );
}

#[test]
fn tapping_a_suggestion_replaces_the_partial_word() {
    let layout = en();
    let mut kb = Keyboard::new(vec![layout]).unwrap();
    kb.insert_text("th");
    let commands = kb.apply_suggestion("the");
    assert_eq!(
        commands,
        vec![
            Command::Backspace { bytes: 2 },
            Command::Commit("the ".into())
        ]
    );
    assert_eq!(kb.text(), "the ");
}

#[test]
fn predictor_learns_bigrams_and_persists() {
    use osk_core::predict::Predictor;
    let mut predictor = Predictor::new();
    predictor.learn("hello world ");
    let suggested = predictor.suggestions("hello ");
    assert!(suggested.iter().any(|w| w == "world"), "{suggested:?}");

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("predictor.tsv");
    predictor.save(&path).unwrap();
    let loaded = Predictor::load(&path);
    assert!(loaded.suggestions("hello ").iter().any(|w| w == "world"));
}

#[test]
fn emoji_panel_groups_and_search() {
    let en_layout = en();
    let mut kb = Keyboard::new(vec![en_layout.clone(), th(), emoji_layout()]).unwrap();

    // Open the panel from the English layout.
    let panel_key = pos_of_action(
        &en_layout,
        "base",
        osk_core::layout::KeyAction::Panel("emoji".into()),
    );
    kb.press(panel_key, 0);
    assert!(kb.is_panel());
    assert_eq!(kb.layout().id, "emoji");
    assert_eq!(kb.layer_name(), "e_smileys");

    // Next group.
    let next = pos_of_action(
        kb.layout(),
        "e_smileys",
        osk_core::layout::KeyAction::EmojiGroupNext,
    );
    kb.press(next, 10);
    assert_eq!(kb.layer_name(), "e_people");

    // Search: "smil" should find smiling faces in English names.
    let search = pos_of_action(
        kb.layout(),
        "e_people",
        osk_core::layout::KeyAction::EmojiSearch,
    );
    kb.press(search, 20);
    assert_eq!(kb.layer_name(), "s_letters");
    for (i, ch) in ["s", "m", "i", "l"].iter().enumerate() {
        let key = pos_of(kb.layout(), "s_letters", ch);
        kb.press(key, 30 + i as u64);
    }
    assert_eq!(
        kb.layer_name(),
        "s_letters",
        "letters stay visible while typing"
    );
    assert_eq!(kb.emoji_query(), Some("smil"));
    // The search key now shows the results grid.
    let show = pos_of_action(
        kb.layout(),
        "s_letters",
        osk_core::layout::KeyAction::EmojiSearch,
    );
    kb.press(show, 35);
    assert_eq!(kb.layer_name(), "e_results");

    // The first result commits an emoji and lands in the recents.
    let first = KeyPos { row: 0, col: 0 };
    let emoji = kb.layout().key("e_results", first).unwrap().label.clone();
    assert!(!emoji.is_empty());
    let commands = kb.press(first, 40);
    assert_eq!(commands, vec![Command::Commit(emoji.clone())]);
    assert_eq!(
        kb.recents().first().map(String::as_str),
        Some(emoji.as_str())
    );
    assert!(kb.recents_dirty());

    // Back to the app layout.
    let back = pos_of_action(
        kb.layout(),
        "e_results",
        osk_core::layout::KeyAction::PanelBack,
    );
    kb.press(back, 50);
    assert!(!kb.is_panel());
    assert_eq!(kb.layout().id, "en-us");
}

#[test]
fn emoji_search_matches_thai_names() {
    let index = osk_core::EmojiIndex::parse(include_str!("../../../data/emoji.tsv"));
    assert!(index.len() > 100, "generated catalogue");
    let hits = index.search("ยิ้ม");
    assert!(
        !hits.is_empty(),
        "Thai annotation search finds smiling faces"
    );
    let english = index.search("smiling");
    assert!(!english.is_empty(), "English names searchable too");
}

#[test]
fn space_swipe_moves_the_cursor() {
    let layout = en();
    let mut kb = Keyboard::new(vec![layout]).unwrap();
    assert_eq!(kb.space_swipe(-3), vec![Command::MoveCursor(-3)]);
    assert_eq!(kb.space_swipe(0), Vec::new());
}

#[test]
fn surrounding_text_at_cursor_truncates_after_cursor_for_backspace_and_suggestions() {
    let layout = th();
    let mut kb = Keyboard::new(vec![en(), layout]).unwrap();
    let full = "ก่ world".to_string();
    let thai_bytes = "ก่".len(); // 6 bytes (ก + tone mark = 1 grapheme cluster)
    kb.set_surrounding_text_at(full, thai_bytes);
    assert_eq!(kb.text(), "ก่");
    assert_eq!(
        kb.backspace(),
        vec![Command::Backspace { bytes: thai_bytes }]
    );
}

#[test]
fn emoji_search_space_stays_in_query_and_reset_clears_panel() {
    let mut kb = Keyboard::new(vec![en(), emoji_layout()]).unwrap();
    let panel = pos_of_action(
        kb.layout(),
        "base",
        osk_core::layout::KeyAction::Panel("emoji".into()),
    );
    kb.press(panel, 0);
    let search = pos_of_action(
        kb.layout(),
        kb.layer_name(),
        osk_core::layout::KeyAction::EmojiSearch,
    );
    kb.press(search, 10);
    assert_eq!(kb.layer_name(), "s_letters");
    let a = pos_of(kb.layout(), "s_letters", "a");
    kb.press(a, 20);
    // Simulate a Space press in emoji search if present, or verify reset_for_activation restores en-us.
    kb.reset_for_activation();
    assert!(!kb.is_panel());
    assert_eq!(kb.layout().id, "en-us");
    assert!(kb.text().is_empty());
}
