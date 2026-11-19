//! Line editing, command history and terminal-grid math — all pure functions
//! so they are host-testable without a Wayland session.

/// A single-line editor: text plus a byte-index cursor (always on a char
/// boundary). Readline-style ops: move by char, kill word/line, history.
#[derive(Debug, Clone)]
pub struct LineEdit {
    text: String,
    /// Byte offset of the cursor into `text`.
    cursor: usize,
}

impl LineEdit {
    #[must_use]
    pub fn new() -> Self {
        Self {
            text: String::new(),
            cursor: 0,
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    #[must_use]
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Replace the whole line (history recall).
    pub fn set(&mut self, s: &str) {
        self.text.clear();
        self.text.push_str(s);
        self.cursor = self.text.len();
    }

    /// Insert `s` at the cursor.
    pub fn insert_str(&mut self, s: &str) {
        self.text.insert_str(self.cursor, s);
        self.cursor += s.len();
    }

    /// Delete the char before the cursor. Returns false when at start.
    pub fn backspace(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        let prev = prev_boundary(&self.text, self.cursor);
        self.text.drain(prev..self.cursor);
        self.cursor = prev;
        true
    }

    /// Delete the char after the cursor. Returns false at end.
    pub fn delete_fwd(&mut self) -> bool {
        if self.cursor >= self.text.len() {
            return false;
        }
        let next = next_boundary(&self.text, self.cursor);
        self.text.drain(self.cursor..next);
        true
    }

    pub fn left(&mut self) {
        self.cursor = prev_boundary(&self.text, self.cursor);
    }

    pub fn right(&mut self) {
        if self.cursor < self.text.len() {
            self.cursor = next_boundary(&self.text, self.cursor);
        }
    }

    pub fn home(&mut self) {
        self.cursor = 0;
    }

    pub fn end(&mut self) {
        self.cursor = self.text.len();
    }

    /// Ctrl+W: delete from cursor back over trailing whitespace then the
    /// previous word. Returns true if anything was deleted.
    pub fn kill_word_back(&mut self) -> bool {
        let bytes = self.text.as_bytes();
        let mut i = self.cursor;
        while i > 0 {
            let p = prev_boundary(&self.text, i);
            if !bytes[p].is_ascii_whitespace() {
                break;
            }
            i = p;
        }
        while i > 0 {
            let p = prev_boundary(&self.text, i);
            if bytes[p].is_ascii_whitespace() {
                break;
            }
            i = p;
        }
        if i == self.cursor {
            return false;
        }
        self.text.drain(i..self.cursor);
        self.cursor = i;
        true
    }

    /// Ctrl+U (unix-line-discard): delete from line start to cursor.
    pub fn kill_before_cursor(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        self.text.drain(..self.cursor);
        self.cursor = 0;
        true
    }

    /// Ctrl+K (kill-line): delete from cursor to end.
    pub fn kill_after_cursor(&mut self) -> bool {
        if self.cursor >= self.text.len() {
            return false;
        }
        self.text.truncate(self.cursor);
        true
    }
}

impl Default for LineEdit {
    fn default() -> Self {
        Self::new()
    }
}

fn prev_boundary(s: &str, mut i: usize) -> usize {
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

fn next_boundary(s: &str, i: usize) -> usize {
    let mut j = (i + 1).min(s.len());
    while j < s.len() && !s.is_char_boundary(j) {
        j += 1;
    }
    j
}

/// Command history with up/down recall. `None` position means "editing a
/// fresh line"; the in-progress text is stashed while browsing.
#[derive(Debug, Default)]
pub struct History {
    items: Vec<String>,
    pos: Option<usize>,
    stash: String,
}

impl History {
    const CAP: usize = 200;

    /// Record a submitted command. Duplicates of the last entry are skipped
    /// (shell convention), the list is capped.
    pub fn push(&mut self, cmd: &str) {
        if cmd.is_empty() || self.items.last() == Some(&cmd.to_string()) {
            return;
        }
        self.items.push(cmd.to_string());
        if self.items.len() > Self::CAP {
            let drain = self.items.len() - Self::CAP;
            self.items.drain(..drain);
        }
        self.pos = None;
        self.stash.clear();
    }

    /// Up-arrow: recall older entry. `current` is the in-progress line text;
    /// it is stashed the first time we leave it. Returns the line to edit.
    pub fn up(&mut self, current: &str) -> String {
        if self.items.is_empty() {
            return current.to_string();
        }
        match self.pos {
            None => {
                self.stash = current.to_string();
                self.pos = Some(self.items.len() - 1);
            }
            Some(0) => {}
            Some(p) => self.pos = Some(p - 1),
        }
        self.pos
            .map_or_else(|| current.to_string(), |p| self.items[p].clone())
    }

    /// Down-arrow: recall newer entry, restoring the stashed line at the end.
    pub fn down(&mut self, current: &str) -> String {
        match self.pos {
            None => current.to_string(),
            Some(p) if p + 1 < self.items.len() => {
                self.pos = Some(p + 1);
                self.items[p + 1].clone()
            }
            Some(_) => {
                self.pos = None;
                self.stash.clone()
            }
        }
    }
}

/// Approximate terminal cell width of a char (wcwidth-lite).
///
/// No monospace font is bundled in `de-paint` yet, so column math uses
/// display-width estimates: ASCII = 1, common East-Asian/wide ranges = 2,
/// combining marks and controls = 0.
#[must_use]
pub fn char_width(c: char) -> usize {
    let u = c as u32;
    if u == 0 || (u < 32) || (0x7F..0xA0).contains(&u) {
        return 0;
    }
    if (0x0300..=0x036F).contains(&u)
        || (0x0483..=0x0489).contains(&u)
        || (0x20D0..=0x20FF).contains(&u)
        || (0xFE20..=0xFE2F).contains(&u)
    {
        return 0; // combining marks
    }
    if u == 0x09 {
        return 4; // tab renders as a small gap in the input row
    }
    let wide = (0x1100..=0x115F).contains(&u)
        || (0x2E80..=0x303E).contains(&u)
        || (0x3041..=0x33FF).contains(&u)
        || (0x3400..=0x4DBF).contains(&u)
        || (0x4E00..=0x9FFF).contains(&u)
        || (0xA000..=0xA4CF).contains(&u)
        || (0xAC00..=0xD7A3).contains(&u)
        || (0xF900..=0xFAFF).contains(&u)
        || (0xFE30..=0xFE4F).contains(&u)
        || (0xFF00..=0xFF60).contains(&u)
        || (0xFFE0..=0xFFE6).contains(&u)
        || (0x1F300..=0x1FAFF).contains(&u)
        || (0x20000..=0x2FFFD).contains(&u)
        || (0x30000..=0x3FFFD).contains(&u);
    if wide {
        2
    } else {
        1
    }
}

/// Greedy wrap of `text` into rows of at most `cols` cells.
/// A single char wider than `cols` still gets its own row rather than
/// looping forever.
#[must_use]
pub fn wrap_cols(text: &str, cols: usize) -> Vec<String> {
    if cols == 0 {
        return vec![text.to_string()];
    }
    let mut rows = Vec::new();
    let mut cur = String::new();
    let mut w = 0usize;
    for c in text.chars() {
        let cw = char_width(c);
        if w + cw > cols && !cur.is_empty() {
            rows.push(std::mem::take(&mut cur));
            w = 0;
        }
        cur.push(c);
        w += cw;
    }
    rows.push(cur);
    rows
}

/// Byte range `(start, end)` of `text` that should be visible in an input
/// field `cols` cells wide, keeping the byte-cursor `cursor` on screen with
/// roughly a third of the field showing context after it.
///
/// `cursor` is a byte offset (as maintained by [`LineEdit`]).
#[must_use]
pub fn input_window(text: &str, cursor: usize, cols: usize) -> (usize, usize) {
    let cursor = cursor.min(text.len());
    if cols == 0 {
        return (cursor, cursor);
    }
    // Char list with byte offsets.
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let n = chars.len();
    // Index of the char the cursor sits before (cursor may be at end).
    let cur_pos = chars.partition_point(|(b, _)| *b < cursor).min(n);

    let width_of =
        |a: usize, b: usize| -> usize { chars[a..b].iter().map(|(_, c)| char_width(*c)).sum() };
    if width_of(0, n) <= cols {
        return (0, text.len());
    }
    // Extend the right edge past the cursor with ~1/3 of the field.
    let mut end = cur_pos;
    let mut w = 0usize;
    while end < n && w + char_width(chars[end].1) <= cols / 3 {
        w += char_width(chars[end].1);
        end += 1;
    }
    // Extend the left edge to fill the rest.
    let mut start = end;
    while start > 0 && width_of(start - 1, end) <= cols {
        start -= 1;
    }
    // If the left edge hit the string start with room to spare (cursor near
    // the beginning), widen the right edge to fill the field.
    while end < n && width_of(start, end + 1) <= cols {
        end += 1;
    }
    let byte_start = if start < n {
        chars[start].0
    } else {
        text.len()
    };
    let byte_end = if end < n { chars[end].0 } else { text.len() };
    (byte_start, byte_end)
}

/// Map a character typed while Ctrl is latched to its control character
/// (readline/emacs convention). Returns `None` for chars with no mapping.
#[must_use]
pub fn ctrl_char(c: char) -> Option<char> {
    match c {
        'a'..='z' => char::from_u32(c as u32 - 'a' as u32 + 1),
        'A'..='Z' => char::from_u32(c as u32 - 'A' as u32 + 1),
        '[' => Some('\u{1B}'),
        '\\' => Some('\u{1C}'),
        ']' => Some('\u{1D}'),
        '^' => Some('\u{1E}'),
        '_' => Some('\u{1F}'),
        '?' => Some('\u{7F}'),
        '@' => Some('\0'),
        ' ' => Some('\0'),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_edit_insert_and_backspace() {
        let mut e = LineEdit::new();
        e.insert_str("héllo");
        assert_eq!(e.cursor(), "héllo".len());
        assert!(e.backspace());
        assert_eq!(e.as_str(), "héll");
        e.left();
        e.insert_str("L");
        assert_eq!(e.as_str(), "hélLl");
        e.home();
        assert_eq!(e.cursor(), 0);
        e.end();
        assert!(e.backspace());
        assert_eq!(e.as_str(), "hélL");
    }

    #[test]
    fn delete_fwd_and_moves_respect_utf8() {
        let mut e = LineEdit::new();
        e.set("aéb");
        e.home();
        assert!(e.delete_fwd());
        assert_eq!(e.as_str(), "éb");
        e.left();
        e.right();
        assert_eq!(e.cursor(), "é".len());
    }

    #[test]
    fn word_kill() {
        let mut e = LineEdit::new();
        e.set("foo bar   ");
        e.end();
        assert!(e.kill_word_back());
        assert_eq!(e.as_str(), "foo ");
        // now kill "foo " entirely: whitespace then word
        assert!(e.kill_word_back());
        assert_eq!(e.as_str(), "");
    }

    #[test]
    fn kill_before_and_after() {
        let mut e = LineEdit::new();
        e.set("hello world");
        e.home();
        for _ in 0..5 {
            e.right();
        }
        assert!(e.kill_before_cursor());
        assert_eq!(e.as_str(), " world");
        assert_eq!(e.cursor(), 0);
        assert!(e.kill_after_cursor());
        assert_eq!(e.as_str(), "");
    }

    #[test]
    fn history_up_down_restores_stash() {
        let mut h = History::default();
        h.push("ls");
        h.push("pwd");
        h.push("ls"); // dup of earlier but not last — allowed? last is "pwd" so kept
        assert_eq!(h.up("draft"), "ls");
        assert_eq!(h.up("draft"), "pwd");
        assert_eq!(h.up("draft"), "ls"); // oldest
        assert_eq!(h.up("draft"), "ls"); // stays at oldest
        assert_eq!(h.down("draft"), "pwd");
        assert_eq!(h.down("draft"), "ls");
        assert_eq!(h.down("draft"), "draft"); // stash restored
        assert_eq!(h.down("draft"), "draft"); // fresh line again
    }

    #[test]
    fn history_empty_is_noop() {
        let mut h = History::default();
        assert_eq!(h.up("x"), "x");
        assert_eq!(h.down("x"), "x");
    }

    #[test]
    fn wrap_cols_basic() {
        assert_eq!(wrap_cols("abcdef", 3), vec!["abc", "def"]);
        assert_eq!(wrap_cols("ab cd", 3), vec!["ab ", "cd"]);
        assert_eq!(wrap_cols("", 5), vec![""]);
        assert_eq!(wrap_cols("abcdef", 0), vec!["abcdef"]);
    }

    #[test]
    fn wrap_cols_wide_chars() {
        // '界' is width 2.
        assert_eq!(wrap_cols("a界b", 3), vec!["a界", "b"]);
        // A width-2 char that doesn't fit moves to the next row.
        assert_eq!(wrap_cols("ab界", 3), vec!["ab", "界"]);
    }

    #[test]
    fn input_window_short_text() {
        assert_eq!(input_window("hello", 5, 20), (0, 5));
    }

    #[test]
    fn input_window_scrolls_to_cursor() {
        let text = "abcdefghijklmnopqrstuvwxyz";
        let (s, e) = input_window(text, text.len(), 10);
        assert!(e - s <= text.len());
        assert!(e > s);
        // cursor at end must be inside the window: last `cols` cells
        assert_eq!(&text[s..e], &text[text.len() - 10..]);
    }

    #[test]
    fn input_window_cursor_at_start() {
        let text = "abcdefghijklmnopqrstuvwxyz";
        let (s, e) = input_window(text, 0, 10);
        assert_eq!((s, e), (0, 10));
    }

    #[test]
    fn input_window_middle_cursor() {
        let text = "0123456789abcdefghijklmnopqrstuvwxyz";
        let (s, e) = input_window(text, 15, 10);
        assert!(s <= 15 && e >= 15);
        assert!(e - s <= 10);
    }

    #[test]
    fn ctrl_mapping() {
        assert_eq!(ctrl_char('c'), Some('\u{3}'));
        assert_eq!(ctrl_char('A'), Some('\u{1}'));
        assert_eq!(ctrl_char('['), Some('\u{1B}'));
        assert_eq!(ctrl_char('?'), Some('\u{7F}'));
        assert_eq!(ctrl_char('1'), None);
    }
}
