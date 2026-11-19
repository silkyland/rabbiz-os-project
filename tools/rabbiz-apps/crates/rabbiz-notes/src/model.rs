//! Pure note model: titles, search, edited-time formatting and the UTF-8
//! text buffer used by both the editor and the search field. Everything here
//! is host-testable without a Wayland session.
//!
//! Thai text rules that matter: never slice a `String` at a non-char-boundary,
//! and treat combining marks (Thai vowels/tone marks, generic Mn ranges) as
//! part of the preceding base char so backspace/delete/movement never split a
//! grapheme cluster.

use std::time::{SystemTime, UNIX_EPOCH};

/// One persisted note. `title` is derived from `body` (first non-empty line)
/// and stored only for forward compatibility — [`derive_title`] is the single
/// source of truth.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    pub id: u64,
    pub title: String,
    pub body: String,
    /// Seconds since the unix epoch, local-agnostic.
    pub updated: i64,
}

impl Note {
    #[must_use]
    pub fn new(id: u64, body: &str, now: i64) -> Self {
        let body = normalize_body(body);
        Self {
            id,
            title: derive_title(&body),
            body,
            updated: now,
        }
    }

    /// Refresh `title`/`updated` after the body changed.
    pub fn touch(&mut self, now: i64) {
        self.title = derive_title(&self.body);
        self.updated = now;
    }
}

/// Normalize line endings so the renderer (cosmic-text splits on `'\n'`)
/// and every edit op see one convention.
#[must_use]
pub fn normalize_body(s: &str) -> String {
    if s.contains('\r') {
        s.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        s.to_string()
    }
}

/// First non-empty line, whitespace-collapsed, char-boundary-safe truncated
/// to `TITLE_MAX_CHARS`. Empty/whitespace bodies yield "Untitled note"
/// (Google Keep convention).
pub const TITLE_MAX_CHARS: usize = 60;

#[must_use]
pub fn derive_title(body: &str) -> String {
    for line in body.lines() {
        let t = collapse_ws(line);
        if !t.is_empty() {
            return truncate_chars(&t, TITLE_MAX_CHARS);
        }
    }
    "Untitled note".to_string()
}

fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Truncate to at most `max` chars without splitting a char (and without
/// leaving a trailing partial grapheme cluster — we only cut on char
/// boundaries, which is sufficient for titles).
#[must_use]
pub fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push('…');
    out
}

/// Case-insensitive substring match over title + body. Thai has no case;
/// `to_lowercase` is a no-op there and correct for Latin.
#[must_use]
pub fn note_matches(note: &Note, query: &str) -> bool {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return true;
    }
    note.title.to_lowercase().contains(&q) || note.body.to_lowercase().contains(&q)
}

/// Indices of notes matching `query`, preserving `notes` order (the store
/// keeps notes sorted by `updated` desc).
#[must_use]
pub fn matching_indices(notes: &[Note], query: &str) -> Vec<usize> {
    notes
        .iter()
        .enumerate()
        .filter(|(_, n)| note_matches(n, query))
        .map(|(i, _)| i)
        .collect()
}

/// Current unix time in seconds.
#[must_use]
pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Edited-time formatting (local wall clock via libc, same approach as
// rabbiz-sms so all apps agree on "Today"/"Yesterday"/"12 Nov").
// ---------------------------------------------------------------------------

/// Local wall-clock parts for a unix timestamp (device TZ via libc).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalTime {
    pub year: i32,
    pub month: u32, // 1..=12
    pub day: u32,
    pub hour: u32,
    pub min: u32,
    pub yday: u32, // 0-based day of year
}

#[must_use]
pub fn local_time(ts: i64) -> LocalTime {
    // SAFETY: `t` is a valid time_t; localtime_r writes into `tm` which we
    // own. tm_zone/tm_gmtoff are not read.
    unsafe {
        let t = ts as libc::time_t;
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&t, &mut tm).is_null() {
            return LocalTime {
                year: 1970,
                month: 1,
                day: 1,
                hour: 0,
                min: 0,
                yday: 0,
            };
        }
        LocalTime {
            year: tm.tm_year + 1900,
            month: (tm.tm_mon + 1) as u32,
            day: tm.tm_mday as u32,
            hour: tm.tm_hour as u32,
            min: tm.tm_min as u32,
            yday: tm.tm_yday as u32,
        }
    }
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Compact edited-time label for a list row:
/// same day → "14:32", yesterday → "Yesterday", this year → "12 Nov",
/// older → "12 Nov 2025". (Keep-style recency cues without relative noise.)
#[must_use]
pub fn format_edited(updated: i64, now: i64) -> String {
    let a = local_time(updated);
    let b = local_time(now);
    if a.year == b.year && a.yday == b.yday {
        return format!("{:02}:{:02}", a.hour, a.min);
    }
    if civil_day(b.year, b.month, b.day) - civil_day(a.year, a.month, a.day) == 1 {
        return "Yesterday".to_string();
    }
    let mon = MONTHS[(a.month as usize).saturating_sub(1).min(11)];
    if a.year == b.year {
        format!("{} {}", a.day, mon)
    } else {
        format!("{} {} {}", a.day, mon, a.year)
    }
}

/// Days since 1970-01-01 for a civil date (Howard Hinnant's algorithm).
fn civil_day(y: i32, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y } as i64;
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

// ---------------------------------------------------------------------------
// TextBuf — a byte-cursor text buffer shared by the note editor and the
// single-line search field. Cursor is always a char boundary.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct TextBuf {
    pub text: String,
    /// Byte offset of the cursor into `text` (char-boundary aligned).
    pub cursor: usize,
}

impl TextBuf {
    #[must_use]
    pub fn new() -> Self {
        Self {
            text: String::new(),
            cursor: 0,
        }
    }

    /// Test-only constructor: production code builds buffers via `new`/`set`.
    #[cfg(test)]
    #[must_use]
    pub fn from_text(s: &str) -> Self {
        let text = normalize_body(s);
        let cursor = text.len();
        Self { text, cursor }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Replace contents, cursor to end.
    pub fn set(&mut self, s: &str) {
        self.text = normalize_body(s);
        self.cursor = self.text.len();
    }

    /// Insert `s` at the cursor (any string, including newlines and Thai).
    pub fn insert_str(&mut self, s: &str) {
        let s = normalize_body(s);
        if s.is_empty() {
            return;
        }
        self.text.insert_str(self.cursor, &s);
        self.cursor += s.len();
    }

    /// Delete up to `max_bytes` of text before the cursor, in whole grapheme
    /// clusters (never leaves an orphan combining mark). Returns true when
    /// anything was deleted. Used for IME `delete_surrounding` and ⌫.
    pub fn delete_before(&mut self, max_bytes: usize) -> bool {
        if self.cursor == 0 || max_bytes == 0 {
            return false;
        }
        let mut end = self.cursor;
        let mut deleted = 0usize;
        while end > 0 && deleted < max_bytes {
            let p = prev_grapheme(&self.text, end);
            deleted += end - p;
            end = p;
        }
        if end == self.cursor {
            return false;
        }
        self.text.drain(end..self.cursor);
        self.cursor = end;
        true
    }

    /// Delete exactly one grapheme cluster before the cursor.
    pub fn delete_grapheme_back(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        let p = prev_grapheme(&self.text, self.cursor);
        self.text.drain(p..self.cursor);
        self.cursor = p;
        true
    }

    /// Delete the grapheme cluster after the cursor.
    pub fn delete_grapheme_fwd(&mut self) -> bool {
        if self.cursor >= self.text.len() {
            return false;
        }
        let n = next_grapheme(&self.text, self.cursor);
        self.text.drain(self.cursor..n);
        true
    }

    pub fn left(&mut self) {
        self.cursor = prev_grapheme(&self.text, self.cursor);
    }

    pub fn right(&mut self) {
        if self.cursor < self.text.len() {
            self.cursor = next_grapheme(&self.text, self.cursor);
        }
    }

    /// Start of the current `'\n'`-delimited line.
    pub fn line_home(&mut self) {
        self.cursor = self.text[..self.cursor.min(self.text.len())]
            .rfind('\n')
            .map_or(0, |i| i + 1);
    }

    /// End of the current `'\n'`-delimited line (before the `'\n'`).
    pub fn line_end(&mut self) {
        let c = self.cursor.min(self.text.len());
        self.cursor = self.text[c..].find('\n').map_or(self.text.len(), |i| c + i);
    }

    /// Byte offset of the start of line `line_i` (0-based, `'\n'` splits).
    #[must_use]
    pub fn line_start(&self, line_i: usize) -> usize {
        let mut line = 0usize;
        for (i, b) in self.text.bytes().enumerate() {
            if b == b'\n' {
                line += 1;
                if line == line_i {
                    return i + 1;
                }
            }
        }
        self.text.len()
    }

    /// `(line_i, byte_index_within_line)` for the cursor — the inverse of
    /// the cosmic-text `Cursor` the editor hit-test returns.
    #[must_use]
    pub fn cursor_line_col(&self) -> (usize, usize) {
        let c = self.cursor.min(self.text.len());
        let mut line = 0usize;
        let mut start = 0usize;
        for (i, b) in self.text[..c].bytes().enumerate() {
            if b == b'\n' {
                line += 1;
                start = i + 1;
            }
        }
        (line, c - start)
    }

    /// Set the cursor from `(line_i, byte_index_within_line)`; clamps into
    /// range and onto a grapheme boundary.
    pub fn set_cursor_line_col(&mut self, line_i: usize, index: usize) {
        let start = self.line_start(line_i);
        // Find the end of that line.
        let end = self.text[start..]
            .find('\n')
            .map_or(self.text.len(), |i| start + i);
        let mut c = (start + index).min(end).min(self.text.len());
        while c > start && !self.text.is_char_boundary(c) {
            c -= 1;
        }
        while c < end && is_mark_at(&self.text, c) {
            // Don't park the cursor between a base char and its marks.
            c = next_boundary(&self.text, c);
        }
        self.cursor = c;
    }
}

impl Default for TextBuf {
    fn default() -> Self {
        Self::new()
    }
}

// --- grapheme-ish boundary helpers -----------------------------------------

/// True for combining marks we keep glued to their base char: Thai
/// above/below marks and tone marks, plus the common generic Mn/Me ranges.
/// (Deliberately conservative — a missed exotic mark only means one extra
/// keypress, never corrupted text.)
#[must_use]
pub fn is_mark(c: char) -> bool {
    let u = c as u32;
    matches!(u,
        0x0300..=0x036F      // combining diacriticals
        | 0x0483..=0x0489
        | 0x0591..=0x05C7    // Hebrew points
        | 0x0610..=0x061A
        | 0x064B..=0x065F    // Arabic harakat
        | 0x0670
        | 0x06D6..=0x06ED
        | 0x0E31             // Thai MAI HAN-AKAT
        | 0x0E34..=0x0E3A    // Thai vowels above/below
        | 0x0E47..=0x0E4E    // Thai marks above (tone marks etc.)
        | 0x20D0..=0x20FF    // combining marks for symbols
        | 0xFE00..=0xFE0F    // variation selectors
        | 0xFE20..=0xFE2F)
}

fn is_mark_at(s: &str, i: usize) -> bool {
    s.get(i..)
        .and_then(|t| t.chars().next())
        .is_some_and(is_mark)
}

/// Previous char boundary strictly before `i` (i clamped to len).
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

/// Next char boundary strictly after `i`.
fn next_boundary(s: &str, i: usize) -> usize {
    let mut j = (i + 1).min(s.len());
    while j < s.len() && !s.is_char_boundary(j) {
        j += 1;
    }
    j
}

/// Start of the grapheme cluster ending at `i` (skips trailing combining
/// marks so "ก้" deletes as one unit).
#[must_use]
pub fn prev_grapheme(s: &str, i: usize) -> usize {
    let mut p = prev_boundary(s, i);
    while p > 0 && is_mark_at(s, p) {
        p = prev_boundary(s, p);
    }
    p
}

/// End of the grapheme cluster starting at `i` (includes following combining
/// marks).
#[must_use]
pub fn next_grapheme(s: &str, i: usize) -> usize {
    let mut n = next_boundary(s, i);
    while n < s.len() && is_mark_at(s, n) {
        n = next_boundary(s, n);
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_first_nonempty_line() {
        assert_eq!(derive_title("Hello\nworld"), "Hello");
        assert_eq!(derive_title("\n\n  hi there  \nmore"), "hi there");
        assert_eq!(derive_title("   \n\t\n"), "Untitled note");
        assert_eq!(derive_title(""), "Untitled note");
    }

    #[test]
    fn title_truncates_on_char_boundary() {
        // Thai chars are 3 bytes; cutting mid-char must never happen.
        let long = "ก".repeat(80);
        let t = derive_title(&long);
        assert!(t.chars().count() <= TITLE_MAX_CHARS + 1);
        assert!(t.ends_with('…'));
        assert!(long.starts_with(&t[..t.len() - '…'.len_utf8()]));
    }

    #[test]
    fn search_is_case_insensitive_and_unicode() {
        let n = Note::new(1, "สวัสดีชาวโลก hello", 0);
        assert!(note_matches(&n, "HELLO"));
        assert!(note_matches(&n, "สวัสดี"));
        assert!(note_matches(&n, "  "));
        assert!(!note_matches(&n, "xyz"));
    }

    #[test]
    fn matching_indices_preserve_order() {
        let notes = vec![
            Note::new(1, "alpha", 3),
            Note::new(2, "beta", 2),
            Note::new(3, "alphabet", 1),
        ];
        assert_eq!(matching_indices(&notes, "alph"), vec![0, 2]);
        assert_eq!(matching_indices(&notes, ""), vec![0, 1, 2]);
    }

    #[test]
    fn edited_time_labels() {
        // 2024-01-15 12:00 local is implementation-defined; assert structural
        // properties instead of exact strings: same-day gives HH:MM,
        // different-day gives a word or date.
        let now = 1_705_320_000; // fixed epoch
        let same = format_edited(now, now);
        assert_eq!(same.len(), 5);
        assert_eq!(&same[2..3], ":");
        let day_before = format_edited(now - 86_400, now);
        assert!(day_before == "Yesterday" || day_before.contains(' '));
        // ~400 days back lands in a previous year → "D Mon YYYY".
        let old = format_edited(now - 400 * 86_400, now);
        assert_eq!(old.matches(' ').count(), 2, "got {old:?}");
        assert!(old.chars().any(|c| c.is_ascii_digit()));
    }

    #[test]
    fn textbuf_insert_and_cursor_utf8() {
        let mut b = TextBuf::new();
        b.insert_str("héllo");
        assert_eq!(b.cursor, "héllo".len());
        b.left();
        b.insert_str("X");
        assert_eq!(b.as_str(), "héllXo");
        assert!(b.text.is_char_boundary(b.cursor));
    }

    #[test]
    fn thai_combining_marks_delete_as_cluster() {
        // "ก้" = U+0E01 + U+0E49 (SARA I? no — MAI EK is 0E48; use real marks)
        let mut b = TextBuf::from_text("aกิb"); // ิ = SARA I (0E34, below mark)
        b.cursor = "aกิ".len(); // after the cluster
        assert!(b.delete_grapheme_back());
        // Must remove BOTH the base and its mark, not just the mark.
        assert_eq!(b.as_str(), "ab");
        assert_eq!(b.cursor, 1);
    }

    #[test]
    fn delete_before_respects_max_bytes_but_whole_clusters() {
        let mut b = TextBuf::from_text("abc");
        assert!(b.delete_before(2)); // deletes "c"? no — up to 2 bytes: "bc"?
                                     // Loop deletes whole graphemes while deleted < max: 'c'(1) then 'b'(1)=2 stop.
        assert_eq!(b.as_str(), "a");

        let mut t = TextBuf::from_text("กขค"); // 3 Thai chars, 9 bytes
        t.delete_before(4); // one 3-byte char deleted (3<4), second (6>=4) → deletes 2?
                            // deleted=3 <4 → continue; deleted=6 >=4 stop → removes 2 chars.
        assert_eq!(t.as_str(), "ก");
    }

    #[test]
    fn movement_never_splits_graphemes() {
        let mut b = TextBuf::from_text("xกิy");
        b.cursor = b.text.len();
        b.left();
        assert_eq!(b.cursor, "xกิ".len()); // skipped the whole cluster
        b.left();
        assert_eq!(b.cursor, 1); // before 'ก'? no: prev grapheme of cluster start is 'x' end=1
        b.left();
        assert_eq!(b.cursor, 0);
        b.right();
        assert_eq!(b.cursor, 1);
        b.right();
        assert_eq!(b.cursor, "xกิ".len());
    }

    #[test]
    fn line_navigation() {
        let mut b = TextBuf::from_text("one\ntwo\nthree");
        b.cursor = "one\ntw".len();
        b.line_home();
        assert_eq!(b.cursor, "one\n".len());
        b.line_end();
        assert_eq!(b.cursor, "one\ntwo".len());
        assert_eq!(b.cursor_line_col(), (1, 3));
        b.set_cursor_line_col(2, 2);
        assert_eq!(b.cursor, "one\ntwo\nth".len());
        // Out-of-range clamps.
        b.set_cursor_line_col(99, 0);
        assert_eq!(b.cursor, b.text.len());
    }

    #[test]
    fn normalize_crlf() {
        assert_eq!(normalize_body("a\r\nb\rc"), "a\nb\nc");
        let b = TextBuf::from_text("a\r\nb");
        assert_eq!(b.as_str(), "a\nb");
    }

    #[test]
    fn empty_delete_is_noop() {
        let mut b = TextBuf::new();
        assert!(!b.delete_grapheme_back());
        assert!(!b.delete_grapheme_fwd());
    }
}
