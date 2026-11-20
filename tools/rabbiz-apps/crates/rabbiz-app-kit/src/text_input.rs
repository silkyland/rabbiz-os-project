//! Pure UTF-8 text-input buffer with Thai grapheme-cluster awareness and
//! IME preedit support.
//!
//! Host-testable with zero platform dependencies and zero `unsafe`.
//! Thai combining marks (สระ/วรรณยุกต์, including U+0E33 SARA AM) and
//! general Unicode combining diacriticals stay attached to their base
//! character during cursor movement, backspace, forward-delete, and
//! password masking.

/// Semantic hint for an editable text field (maps to Wayland
/// `zwp_text_input_v3::content_purpose`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContentPurpose {
    #[default]
    Normal,
    Number,
    Phone,
    Password,
    Search,
    Email,
}

/// Editable UTF-8 text buffer with a char-boundary byte cursor, IME preedit
/// string, and optional character limit.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TextBuffer {
    text: String,
    /// UTF-8 byte offset into `text`, always aligned on a char boundary.
    cursor: usize,
    preedit: String,
    max_chars: Option<usize>,
}

impl TextBuffer {
    /// Creates an empty text buffer with no character cap.
    #[must_use]
    pub fn new() -> Self {
        Self {
            text: String::new(),
            cursor: 0,
            preedit: String::new(),
            max_chars: None,
        }
    }

    /// Creates a text buffer initialized with `s` and the cursor at the end.
    #[must_use]
    pub fn with_text(s: &str) -> Self {
        Self {
            text: s.to_string(),
            cursor: s.len(),
            preedit: String::new(),
            max_chars: None,
        }
    }

    /// Sets a maximum character (Unicode scalar value) limit on the buffer.
    /// Existing text exceeding `n` characters is truncated and the cursor is
    /// clamped.
    #[must_use]
    pub fn with_max_chars(mut self, n: usize) -> Self {
        self.max_chars = Some(n);
        if self.text.chars().count() > n {
            let cut = nth_char_byte_offset(&self.text, n);
            self.text.truncate(cut);
            if self.cursor > self.text.len() {
                self.cursor = self.text.len();
            }
        }
        self
    }

    /// Returns the committed text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Returns the current cursor byte offset (always a valid UTF-8 char
    /// boundary in `self.as_str()`).
    #[must_use]
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Returns the current uncommitted IME preedit text.
    #[must_use]
    pub fn preedit(&self) -> &str {
        &self.preedit
    }

    /// Returns `true` if the committed text is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Clears the committed text, preedit text, and resets the cursor to 0.
    pub fn clear(&mut self) {
        self.text.clear();
        self.preedit.clear();
        self.cursor = 0;
    }

    /// Inserts `s` at the current cursor position, respecting `max_chars` and
    /// clearing any active preedit string.
    pub fn insert(&mut self, s: &str) {
        self.preedit.clear();
        if s.is_empty() {
            return;
        }
        let to_insert = match self.max_chars {
            Some(max) => {
                let current = self.text.chars().count();
                let remaining = max.saturating_sub(current);
                if remaining == 0 {
                    return;
                }
                let end = nth_char_byte_offset(s, remaining);
                &s[..end]
            }
            None => s,
        };
        if to_insert.is_empty() {
            return;
        }
        self.text.insert_str(self.cursor, to_insert);
        self.cursor += to_insert.len();
    }

    /// Replaces the uncommitted IME preedit string.
    pub fn set_preedit(&mut self, s: &str) {
        self.preedit.clear();
        self.preedit.push_str(s);
    }

    /// Commits the current preedit string into the buffer at the cursor and
    /// clears the preedit state.
    pub fn commit_preedit(&mut self) {
        let pending = std::mem::take(&mut self.preedit);
        if !pending.is_empty() {
            self.insert(&pending);
        }
    }

    /// Deletes one grapheme cluster immediately before the cursor.
    ///
    /// Thai combining marks (สระ/วรรณยุกต์) and general Unicode combining
    /// marks are deleted together with their base character in a single step.
    /// Returns `true` if any text was removed.
    pub fn backspace(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        let prev = prev_grapheme(&self.text, self.cursor);
        if prev == self.cursor {
            return false;
        }
        self.text.drain(prev..self.cursor);
        self.cursor = prev;
        true
    }

    /// Deletes one grapheme cluster immediately after the cursor.
    /// Returns `true` if any text was removed.
    pub fn delete_forward(&mut self) -> bool {
        if self.cursor >= self.text.len() {
            return false;
        }
        let next = next_grapheme(&self.text, self.cursor);
        if next == self.cursor {
            return false;
        }
        self.text.drain(self.cursor..next);
        true
    }

    /// Moves the cursor left by one grapheme cluster.
    pub fn move_left(&mut self) {
        self.cursor = prev_grapheme(&self.text, self.cursor);
    }

    /// Moves the cursor right by one grapheme cluster.
    pub fn move_right(&mut self) {
        if self.cursor < self.text.len() {
            self.cursor = next_grapheme(&self.text, self.cursor);
        }
    }

    /// Moves the cursor to the start of the buffer.
    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    /// Moves the cursor to the end of the buffer.
    pub fn move_end(&mut self) {
        self.cursor = self.text.len();
    }

    /// Returns a password-masked representation with one `'•'` per grapheme
    /// cluster.
    #[must_use]
    pub fn display_masked(&self) -> String {
        let mut clusters = 0usize;
        let mut i = 0usize;
        while i < self.text.len() {
            i = next_grapheme(&self.text, i);
            clusters += 1;
        }
        "•".repeat(clusters)
    }
}

/// Returns `true` if `c` is a combining mark that attaches to the preceding
/// base character (Thai vowels/tone marks, general Unicode combining
/// diacriticals, variation selectors, etc.).
#[must_use]
pub fn is_combining_mark(c: char) -> bool {
    let u = c as u32;
    matches!(
        u,
        0x0300..=0x036F      // Combining Diacritical Marks
        | 0x0483..=0x0489    // Cyrillic combining marks
        | 0x0591..=0x05C7    // Hebrew points
        | 0x0610..=0x061A
        | 0x064B..=0x065F    // Arabic harakat
        | 0x0670
        | 0x06D6..=0x06ED
        | 0x0E31             // Thai MAI HAN-AKAT
        | 0x0E33             // Thai SARA AM (spacing combining vowel, e.g. น้ำ)
        | 0x0E34..=0x0E3A    // Thai vowels above/below
        | 0x0E47..=0x0E4E    // Thai tone & diacritic marks above
        | 0x20D0..=0x20FF    // Combining Diacritical Marks for Symbols
        | 0xFE00..=0xFE0F    // Variation Selectors
        | 0xFE20..=0xFE2F    // Combining Half Marks
    )
}

fn is_mark_at(s: &str, i: usize) -> bool {
    s.get(i..)
        .and_then(|t| t.chars().next())
        .is_some_and(is_combining_mark)
}

fn prev_char_boundary(s: &str, mut i: usize) -> usize {
    i = i.min(s.len());
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    if i > 0 {
        i -= 1;
        while i > 0 && !s.is_char_boundary(i) {
            i -= 1;
        }
    }
    i
}

fn next_char_boundary(s: &str, i: usize) -> usize {
    let mut j = (i + 1).min(s.len());
    while j < s.len() && !s.is_char_boundary(j) {
        j += 1;
    }
    j
}

/// Byte offset of the grapheme cluster immediately preceding byte index `i`.
#[must_use]
pub fn prev_grapheme(s: &str, i: usize) -> usize {
    let mut p = prev_char_boundary(s, i);
    while p > 0 && is_mark_at(s, p) {
        p = prev_char_boundary(s, p);
    }
    p
}

/// Byte offset of the grapheme cluster immediately following byte index `i`.
#[must_use]
pub fn next_grapheme(s: &str, i: usize) -> usize {
    let mut n = next_char_boundary(s, i);
    while n < s.len() && is_mark_at(s, n) {
        n = next_char_boundary(s, n);
    }
    n
}

fn nth_char_byte_offset(s: &str, n: usize) -> usize {
    s.char_indices().nth(n).map_or(s.len(), |(idx, _)| idx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_insert_cursor_and_backspace() {
        let mut buf = TextBuffer::new();
        assert!(buf.is_empty());
        assert_eq!(buf.cursor(), 0);
        assert!(!buf.backspace());
        assert!(!buf.delete_forward());

        buf.insert("hello");
        assert_eq!(buf.as_str(), "hello");
        assert_eq!(buf.cursor(), 5);
        assert!(!buf.is_empty());

        buf.move_left();
        buf.move_left();
        assert_eq!(buf.cursor(), 3);
        buf.insert("XY");
        assert_eq!(buf.as_str(), "helXYlo");
        assert_eq!(buf.cursor(), 5);

        assert!(buf.backspace());
        assert_eq!(buf.as_str(), "helXlo");
        assert_eq!(buf.cursor(), 4);

        assert!(buf.delete_forward());
        assert_eq!(buf.as_str(), "helXo");
        assert_eq!(buf.cursor(), 4);

        buf.move_home();
        assert_eq!(buf.cursor(), 0);
        buf.move_end();
        assert_eq!(buf.cursor(), buf.as_str().len());

        buf.clear();
        assert!(buf.is_empty());
        assert_eq!(buf.cursor(), 0);
    }

    #[test]
    fn utf8_and_emoji_safety() {
        let mut buf = TextBuffer::with_text("hi😀🌍!");
        assert!(buf.as_str().is_char_boundary(buf.cursor()));
        assert_eq!(buf.cursor(), buf.as_str().len());

        buf.move_left(); // before '!'
        assert_eq!(buf.cursor(), "hi😀🌍".len());
        assert!(buf.backspace()); // removes '🌍'
        assert_eq!(buf.as_str(), "hi😀!");
        assert!(buf.as_str().is_char_boundary(buf.cursor()));

        buf.move_left(); // before '😀'
        assert_eq!(buf.cursor(), 2);
        buf.insert("✨");
        assert_eq!(buf.as_str(), "hi✨😀!");
        assert!(buf.delete_forward()); // removes '😀'
        assert_eq!(buf.as_str(), "hi✨!");
    }

    #[test]
    fn thai_combining_marks_single_backspace_cluster_deletion() {
        // ก้ (base + tone mark U+0E49)
        let mut buf = TextBuffer::with_text("ก้");
        assert!(buf.backspace());
        assert_eq!(buf.as_str(), "");
        assert_eq!(buf.cursor(), 0);

        // ที่ (base + SARA II U+0E35 + MAI EK U+0E48)
        let mut buf = TextBuffer::with_text("aที่b");
        buf.move_left(); // before 'b'
        assert!(buf.backspace());
        assert_eq!(buf.as_str(), "ab");
        assert_eq!(buf.cursor(), 1);

        // น้ำ (base + MAI THO U+0E49 + SARA AM U+0E33)
        let mut buf = TextBuffer::with_text("น้ำ");
        assert!(buf.backspace());
        assert_eq!(buf.as_str(), "");
        assert_eq!(buf.cursor(), 0);

        // Forward delete & left/right navigation across Thai clusters
        let mut buf = TextBuffer::with_text("ก้ที่น้ำ");
        buf.move_home();
        buf.move_right();
        assert_eq!(buf.cursor(), "ก้".len());
        assert!(buf.delete_forward()); // deletes "ที่" as one cluster
        assert_eq!(buf.as_str(), "ก้น้ำ");
        buf.move_right();
        assert_eq!(buf.cursor(), buf.as_str().len());
        buf.move_left();
        assert_eq!(buf.cursor(), "ก้".len());
    }

    #[test]
    fn unicode_combining_diacriticals_delete_as_cluster() {
        // 'e' + COMBINING ACUTE ACCENT (U+0301)
        let mut buf = TextBuffer::with_text("cafe\u{0301}");
        assert!(buf.backspace());
        assert_eq!(buf.as_str(), "caf");
    }

    #[test]
    fn preedit_and_commit() {
        let mut buf = TextBuffer::with_text("ab");
        buf.move_left();
        buf.set_preedit("สวัส");
        assert_eq!(buf.preedit(), "สวัส");
        assert_eq!(buf.as_str(), "ab");

        buf.commit_preedit();
        assert_eq!(buf.preedit(), "");
        assert_eq!(buf.as_str(), "aสวัสb");

        // Inserting directly clears any stale preedit
        buf.set_preedit("pending");
        buf.insert("!");
        assert_eq!(buf.preedit(), "");
        assert_eq!(buf.as_str(), "aสวัส!b");
    }

    #[test]
    fn max_chars_cap() {
        let mut buf = TextBuffer::new().with_max_chars(5);
        buf.insert("abc");
        assert_eq!(buf.as_str(), "abc");
        buf.insert("defgh");
        assert_eq!(buf.as_str(), "abcde");
        assert_eq!(buf.cursor(), 5);

        // Further inserts are ignored once full
        buf.insert("z");
        assert_eq!(buf.as_str(), "abcde");

        // Pre-populated buffer is truncated when cap is applied
        let truncated = TextBuffer::with_text("123456789").with_max_chars(4);
        assert_eq!(truncated.as_str(), "1234");
        assert_eq!(truncated.cursor(), 4);
    }

    #[test]
    fn display_masked_counts_clusters() {
        assert_eq!(TextBuffer::new().display_masked(), "");
        assert_eq!(TextBuffer::with_text("secret").display_masked(), "••••••");
        assert_eq!(TextBuffer::with_text("ก้ที่น้ำ").display_masked(), "•••");
        assert_eq!(TextBuffer::with_text("a😀b").display_masked(), "•••");
    }

    #[test]
    fn content_purpose_default_is_normal() {
        assert_eq!(ContentPurpose::default(), ContentPurpose::Normal);
        assert_ne!(ContentPurpose::Password, ContentPurpose::Phone);
        assert_ne!(ContentPurpose::Number, ContentPurpose::Search);
        assert_ne!(ContentPurpose::Email, ContentPurpose::Normal);
    }
}
