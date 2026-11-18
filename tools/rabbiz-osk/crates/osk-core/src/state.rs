//! Keyboard state: shift/caps finite state machine, layers, language cycle, and the
//! translation of a key press into commands for the text-delivery layer.

use unicode_segmentation::UnicodeSegmentation;

use crate::emoji::EmojiIndex;
use crate::predict::Predictor;

use crate::layout::{Key, KeyAction, Layout, LayoutError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShiftState {
    Off,
    /// Shift applies to the next character only (Gboard: tap once).
    OneShot,
    /// Caps lock (Gboard: double-tap shift).
    Locked,
}

impl ShiftState {
    pub fn is_shifted(self) -> bool {
        !matches!(self, ShiftState::Off)
    }
}

/// What the keyboard asks the Wayland shell / app to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Insert this text at the cursor (`zwp_input_method_v2.commit_string`, or the
    /// virtual-keyboard fallback when no text-input client is active).
    Commit(String),
    /// Delete this many UTF-8 bytes before the cursor (input-method-v2 lengths are bytes).
    Backspace { bytes: usize },
    /// Move the cursor by this many characters (space-swipe), negative = left.
    MoveCursor(i32),
    /// Enter / go / search — the delivery layer maps this to the field's `enter_key_hint`.
    Enter,
    /// The active layer changed (numbers, symbols, emoji).
    Layer(String),
    /// The whole layout changed because the language was cycled.
    Language(String),
}

/// What the focused field is for (`zwp_text_input_v3.content_type.purpose`). The keyboard
/// follows it the way Gboard does: email/url get their own bottom rows, numeric fields get a
/// number pad, and auto-capitalise is off for anything that is not prose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputPurpose {
    Normal,
    Alpha,
    Digits,
    Number,
    Phone,
    Url,
    Email,
    Name,
    Password,
    Date,
    Time,
    Terminal,
}

impl InputPurpose {
    /// Values come from text-input-unstable-v3 `content_hint`/`content_purpose`.
    pub fn from_protocol(purpose: u32) -> InputPurpose {
        match purpose {
            1 => InputPurpose::Alpha,
            2 => InputPurpose::Digits,
            3 => InputPurpose::Number,
            4 => InputPurpose::Phone,
            5 => InputPurpose::Url,
            6 => InputPurpose::Email,
            7 => InputPurpose::Name,
            8 => InputPurpose::Password,
            9 => InputPurpose::Date,
            10 => InputPurpose::Time,
            12 => InputPurpose::Terminal,
            _ => InputPurpose::Normal,
        }
    }

    fn layer(self) -> Option<&'static str> {
        match self {
            InputPurpose::Digits
            | InputPurpose::Number
            | InputPurpose::Phone
            | InputPurpose::Date
            | InputPurpose::Time => Some("num"),
            InputPurpose::Url => Some("url"),
            InputPurpose::Email => Some("email"),
            _ => None,
        }
    }

    /// URL/email fields are typed in Latin even when the keyboard is in Thai.
    fn latin(self) -> bool {
        matches!(self, InputPurpose::Url | InputPurpose::Email)
    }

    fn prose(self) -> bool {
        matches!(self, InputPurpose::Normal | InputPurpose::Alpha | InputPurpose::Name)
    }
}

/// Group layers of the generated emoji panel, in tab order.
pub const EMOJI_GROUPS: [&str; 9] = [
    "e_smileys", "e_people", "e_nature", "e_food", "e_activity", "e_travel", "e_objects",
    "e_symbols", "e_flags",
];

pub struct Keyboard {
    layouts: Vec<Layout>,
    active: usize,
    layer: String,
    shift: ShiftState,
    last_shift_tap_ms: Option<u64>,
    text: String,
    purpose: InputPurpose,
    pub predictor: Predictor,
    emoji: EmojiIndex,
    emoji_query: Option<String>,
    emoji_group: usize,
    recents: Vec<String>,
    recents_dirty: bool,
    panel_return: Option<(usize, String)>,
}

/// Two taps closer than this are a double-tap (Gboard uses ~300 ms).
pub const DOUBLE_TAP_MS: u64 = 300;

impl Keyboard {
    pub fn new(layouts: Vec<Layout>) -> Result<Keyboard, LayoutError> {
        let first = layouts
            .first()
            .ok_or_else(|| LayoutError::Invalid("no layouts given".into()))?;
        let layer = first.base_layer().name.clone();
        Ok(Keyboard {
            layouts,
            active: 0,
            layer,
            shift: ShiftState::Off,
            last_shift_tap_ms: None,
            text: String::new(),
            purpose: InputPurpose::Normal,
            predictor: Predictor::new(),
            emoji: EmojiIndex::parse(include_str!("../../../data/emoji.tsv")),
            emoji_query: None,
            emoji_group: 0,
            recents: Vec::new(),
            recents_dirty: false,
            panel_return: None,
        })
    }

    pub fn layout(&self) -> &Layout {
        &self.layouts[self.active]
    }

    pub fn layouts(&self) -> &[Layout] {
        &self.layouts
    }

    pub fn layer_name(&self) -> &str {
        &self.layer
    }

    pub fn shift(&self) -> ShiftState {
        self.shift
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn is_panel(&self) -> bool {
        self.layout().language == "emoji"
    }

    pub fn emoji_query(&self) -> Option<&str> {
        self.emoji_query.as_deref()
    }

    pub fn recents(&self) -> &[String] {
        &self.recents
    }

    pub fn recents_dirty(&self) -> bool {
        self.recents_dirty
    }

    pub fn mark_recents_clean(&mut self) {
        self.recents_dirty = false;
    }

    pub fn load_recents(&mut self, path: &std::path::Path) {
        if let Ok(body) = std::fs::read_to_string(path) {
            self.recents = body
                .lines()
                .filter(|line| !line.trim().is_empty())
                .take(24)
                .map(str::to_string)
                .collect();
            self.refresh_recent_layer();
        }
    }

    pub fn save_recents(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, self.recents.join("
"))
    }

    /// Text shown in the panel while searching for an emoji.
    pub fn strip_text(&self) -> Option<String> {
        self.emoji_query.as_ref().map(|query| format!("Search: {query}"))
    }

    fn pin_recent(&mut self, emoji: &str) {
        self.recents.retain(|old| old != emoji);
        self.recents.insert(0, emoji.to_string());
        self.recents.truncate(24);
        self.recents_dirty = true;
        self.refresh_recent_layer();
    }

    fn refresh_recent_layer(&mut self) {
        let recents = self.recents.clone();
        self.patch_emoji_layer("e_recent", &recents);
    }

    /// Replace the first three rows of an emoji layer, keeping its navigation row.
    fn patch_emoji_layer(&mut self, layer: &str, chars: &[String]) {
        let Some(existing) = self.layout().layer(layer) else {
            return;
        };
        let bottom = existing.rows.last().cloned().unwrap_or_default();
        let mut rows: Vec<Vec<crate::layout::Key>> = Vec::new();
        for chunk in chars.chunks(10).take(3) {
            let mut row: Vec<crate::layout::Key> = chunk
                .iter()
                .map(|c| crate::layout::Layout::text_key(c))
                .collect();
            while row.len() < 10 {
                row.push(crate::layout::Layout::text_key(""));
            }
            rows.push(row);
        }
        while rows.len() < 3 {
            rows.push((0..10).map(|_| crate::layout::Layout::text_key("")).collect());
        }
        rows.push(bottom);
        self.layouts[self.active].replace_layer_rows(layer, rows);
    }

    fn patch_results(&mut self) {
        let query = self.emoji_query.clone().unwrap_or_default();
        let results: Vec<String> = self
            .emoji
            .search(&query)
            .iter()
            .map(|emoji| emoji.char.clone())
            .collect();
        self.patch_emoji_layer("e_results", &results);
    }

    pub fn purpose(&self) -> InputPurpose {
        self.purpose
    }

    /// Follow the focused field's purpose: switch layer and (for URL/email) the Latin layout.
    pub fn set_purpose(&mut self, purpose: InputPurpose) {
        // Chrome re-sends content_type on every text change. Acting again would force the Latin
        // layout right after the user switched to Thai, so only follow *changes* of purpose.
        if self.purpose == purpose {
            return;
        }
        self.purpose = purpose;
        if purpose.latin() {
            self.switch_to_language("en");
        }
        match purpose.layer().and_then(|name| self.layout().layer(name).map(|_| name)) {
            Some(layer) => self.layer = layer.to_string(),
            None => self.layer = self.layout().base_layer().name.clone(),
        }
        self.shift = ShiftState::Off;
        self.last_shift_tap_ms = None;
    }

    fn switch_to_language(&mut self, language: &str) {
        if let Some(index) = self.layouts.iter().position(|l| l.language == language) {
            self.active = index;
            self.layer = self.layouts[index].base_layer().name.clone();
        }
    }

    /// The app's surrounding text, when the compositor tells us (used for auto-capitalise).
    pub fn set_surrounding_text(&mut self, text: String) {
        self.text = text;
        if self.autocapitalise() && Self::at_sentence_start(&self.text) {
            self.shift = ShiftState::OneShot;
        }
    }

    pub fn key(&self, pos: crate::layout::KeyPos) -> Option<&Key> {
        self.layout().key(&self.layer, pos)
    }

    pub fn set_layer(&mut self, name: &str) -> Result<(), LayoutError> {
        if self.layout().layer(name).is_none() {
            return Err(LayoutError::Invalid(format!("unknown layer `{name}`")));
        }
        self.layer = name.to_string();
        Ok(())
    }

    /// Complete a tap on `pos`. `now_ms` is a monotonic clock used for shift double-tap.
    pub fn press(&mut self, pos: crate::layout::KeyPos, now_ms: u64) -> Vec<Command> {
        let key = match self.key(pos) {
            Some(k) => k.clone(),
            None => return Vec::new(),
        };
        match key.action {
            KeyAction::Text(t) => {
                if self.is_panel() {
                    if self.layer == "s_letters" {
                        let mut query = self.emoji_query.clone().unwrap_or_default();
                        query.push_str(&t);
                        self.emoji_query = Some(query);
                        // Keep the letters visible while typing; the search key shows results.
                        self.patch_results();
                        return vec![Command::Layer("s_letters".into())];
                    }
                    if t.is_empty() {
                        return Vec::new();
                    }
                    self.pin_recent(&t);
                    return self.insert_text(&t);
                }
                let mut out = t;
                if self.shift.is_shifted() {
                    // A layout may define what shift commits for a key (Thai digits: `1` → `๑`);
                    // otherwise shift upper-cases it, which is a no-op for digits and Thai.
                    out = key.shifted.clone().unwrap_or_else(|| out.to_uppercase());
                }
                if self.shift == ShiftState::OneShot {
                    self.shift = ShiftState::Off;
                }
                self.last_shift_tap_ms = None;
                self.commit_text(out)
            }
            KeyAction::Space => {
                self.last_shift_tap_ms = None;
                self.commit_text(" ".into())
            }
            KeyAction::Backspace => {
                self.last_shift_tap_ms = None;
                if self.is_panel() {
                    if let Some(query) = self.emoji_query.as_mut() {
                        query.pop();
                        self.patch_results();
                    }
                    return Vec::new();
                }
                self.backspace()
            }
            KeyAction::Return => {
                self.last_shift_tap_ms = None;
                self.text.push('\n');
                vec![Command::Enter]
            }
            KeyAction::Shift => self.tap_shift(now_ms),
            KeyAction::Layer(name) => {
                self.last_shift_tap_ms = None;
                let name = name.clone();
                if self.layout().layer(&name).is_some() {
                    self.layer = name.clone();
                    vec![Command::Layer(name)]
                } else {
                    Vec::new()
                }
            }
            KeyAction::Language => self.cycle_language(),
            KeyAction::Panel(id) => {
                if let Some(index) = self.layouts.iter().position(|layout| layout.id == id) {
                    self.panel_return = Some((self.active, self.layer.clone()));
                    self.active = index;
                    self.layer = self.layouts[index].base_layer().name.clone();
                    self.emoji_group = 0;
                    self.emoji_query = None;
                    self.refresh_recent_layer();
                    // Gboard opens on recents; fall back to the first group.
                    if !self.recents.is_empty() {
                        self.layer = "e_recent".to_string();
                    }
                    vec![Command::Layer(self.layer.clone())]
                } else {
                    Vec::new()
                }
            }
            KeyAction::PanelBack => {
                if let Some((index, layer)) = self.panel_return.take() {
                    self.active = index;
                    self.layer = layer;
                } else if let Some(index) = self.layouts.iter().position(|layout| layout.language == "en") {
                    self.active = index;
                    self.layer = self.layouts[index].base_layer().name.clone();
                }
                self.emoji_query = None;
                vec![Command::Layer(self.layer.clone())]
            }
            KeyAction::EmojiGroupPrev | KeyAction::EmojiGroupNext => {
                if !self.is_panel() {
                    return Vec::new();
                }
                let count = EMOJI_GROUPS.len();
                self.emoji_group = if self.layer == "e_recent" {
                    if matches!(key.action, KeyAction::EmojiGroupPrev) {
                        count - 1
                    } else {
                        0
                    }
                } else if matches!(key.action, KeyAction::EmojiGroupPrev) {
                    (self.emoji_group + count - 1) % count
                } else {
                    (self.emoji_group + 1) % count
                };
                self.layer = EMOJI_GROUPS[self.emoji_group].to_string();
                vec![Command::Layer(self.layer.clone())]
            }
            KeyAction::EmojiSearch => {
                if !self.is_panel() {
                    return Vec::new();
                }
                if self.layer == "s_letters" {
                    // Second tap: show what the query found.
                    self.patch_results();
                    self.layer = "e_results".to_string();
                } else {
                    if self.emoji_query.is_none() {
                        self.emoji_query = Some(String::new());
                    }
                    self.layer = "s_letters".to_string();
                }
                vec![Command::Layer(self.layer.clone())]
            }
        }
    }

    /// Insert text, applying the double-space period rule and re-arming auto-capitalise.
    fn commit_text(&mut self, out: String) -> Vec<Command> {
        if out == " " || out == "." || out == "," {
            // A word just finished: teach the predictor (unigram + bigram), then persist
            // happens in the Wayland layer on a debounce.
            if let Some(word) = self.text.split_whitespace().next_back() {
                self.predictor.learn(&format!("{word} "));
            }
        }
        let mut commands = Vec::new();
        if out == " " && self.text.ends_with(' ') {
            // Gboard's "double-space period": turn the trailing space into ". ".
            commands.push(Command::Backspace { bytes: 1 });
            self.text.pop();
            self.text.push_str(". ");
            commands.push(Command::Commit(". ".into()));
        } else {
            self.text.push_str(&out);
            commands.push(Command::Commit(out));
        }
        if self.autocapitalise() && Self::at_sentence_start(&self.text) {
            self.shift = ShiftState::OneShot;
        }
        commands
    }

    /// Commit opaque text the app chose (emoji, long-press alternate, clipboard).
    /// Keeps the internal buffer and auto-capitalise in sync with what the app now contains.
    pub fn insert_text(&mut self, text: &str) -> Vec<Command> {
        if text.is_empty() {
            return Vec::new();
        }
        self.text.push_str(text);
        if self.autocapitalise() && Self::at_sentence_start(&self.text) {
            self.shift = ShiftState::OneShot;
        }
        vec![Command::Commit(text.to_string())]
    }

    /// Up to three suggestions for the strip (K6): completions + next-word.
    pub fn suggestions(&self) -> Vec<String> {
        self.predictor.suggestions(&self.text)
    }

    /// Tap a suggestion: replace the partial word being typed, insert the pick plus a space.
    pub fn apply_suggestion(&mut self, pick: &str) -> Vec<Command> {
        let mut commands = Vec::new();
        if !self.text.ends_with(' ') {
            if let Some(partial) = self.text.split_whitespace().next_back() {
                if !partial.is_empty() {
                    let bytes = partial.len();
                    self.text.truncate(self.text.len() - bytes);
                    commands.push(Command::Backspace { bytes });
                }
            }
        }
        self.text.push_str(pick);
        self.text.push(' ');
        commands.push(Command::Commit(format!("{pick} ")));
        self.predictor.learn(&format!("{pick} "));
        self.shift = ShiftState::Off;
        self.last_shift_tap_ms = None;
        commands
    }

    /// Long-press choices for the key at `pos` (empty when the key has none).
    pub fn alternates(&self, pos: crate::layout::KeyPos) -> Vec<String> {
        self.key(pos).map(|k| k.alternates.clone()).unwrap_or_default()
    }

    /// True when `pos` is the space key — used for the swipe-to-move-cursor gesture.
    pub fn is_space(&self, pos: crate::layout::KeyPos) -> bool {
        matches!(self.key(pos).map(|k| &k.action), Some(KeyAction::Space))
    }

    /// Backspace: one grapheme cluster (Thai consonant + vowel + tone marks go as one step).
    pub fn backspace(&mut self) -> Vec<Command> {
        let Some(grapheme) = self.text.graphemes(true).next_back() else {
            return vec![Command::Backspace { bytes: 1 }];
        };
        let bytes = grapheme.len();
        let new_len = self.text.len() - bytes;
        self.text.truncate(new_len);
        vec![Command::Backspace { bytes }]
    }

    /// Space-swipe: move the cursor instead of typing.
    pub fn space_swipe(&mut self, steps: i32) -> Vec<Command> {
        if steps == 0 {
            Vec::new()
        } else {
            vec![Command::MoveCursor(steps)]
        }
    }

    pub fn tap_shift(&mut self, now_ms: u64) -> Vec<Command> {
        let double = self
            .last_shift_tap_ms
            .is_some_and(|t| now_ms.saturating_sub(t) <= DOUBLE_TAP_MS);
        self.last_shift_tap_ms = Some(now_ms);
        self.shift = if double {
            ShiftState::Locked
        } else {
            match self.shift {
                ShiftState::Off => ShiftState::OneShot,
                ShiftState::OneShot | ShiftState::Locked => ShiftState::Off,
            }
        };
        Vec::new()
    }

    /// Cycle to the next language among the loaded layouts, switching to its base layer.
    pub fn cycle_language(&mut self) -> Vec<Command> {
        let mut languages: Vec<&str> = Vec::new();
        for layout in &self.layouts {
            if layout.language == "emoji" {
                continue;
            }
            if !languages.contains(&layout.language.as_str()) {
                languages.push(&layout.language);
            }
        }
        if languages.len() < 2 {
            return Vec::new();
        }
        let current_language = self.layouts[self.active].language.clone();
        let idx = languages
            .iter()
            .position(|l| *l == current_language)
            .unwrap_or(0);
        let next = languages[(idx + 1) % languages.len()];
        let next_layout = self
            .layouts
            .iter()
            .position(|l| l.language == next)
            .unwrap_or(0);
        self.active = next_layout;
        self.layer = self.layouts[next_layout].base_layer().name.clone();
        self.shift = ShiftState::Off;
        self.last_shift_tap_ms = None;
        vec![Command::Language(next.to_string())]
    }

    /// Gboard auto-capitalises at sentence start; Thai has no case so it is a no-op there.
    pub fn autocapitalise(&self) -> bool {
        self.layout().language == "en" && self.purpose.prose() && !self.is_panel()
    }

    pub fn at_sentence_start(text: &str) -> bool {
        let t = text.trim_end();
        if t.is_empty() {
            return true;
        }
        let last = t.chars().next_back().unwrap_or(' ');
        matches!(last, '.' | '!' | '?' | '\n')
    }
}
