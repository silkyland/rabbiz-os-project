//! Pure contacts model: the on-disk record shape, demo seed data, sorting,
//! search matching, avatar tints and the UTF-8 text buffer used by the form
//! fields. Everything here is host-testable without a Wayland session.
//!
//! The on-disk schema is owned by `store.rs` — keep the serde field names
//! there and here in lockstep (they are the same struct).

use std::time::{SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------------
// Records (schema v1 — see store.rs for the file contract)
// ---------------------------------------------------------------------------

/// One labelled phone number. `label` is a free-form lowercase-ish string
/// ("mobile", "work", …); readers must tolerate any value.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Phone {
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub number: String,
}

/// One labelled email address.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Email {
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub address: String,
}

/// A contact as stored in `contacts.json`. Field names are the wire format —
/// do not rename without a schema version bump.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Contact {
    /// `"c-<unix-nanos>"`.
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub phones: Vec<Phone>,
    #[serde(default)]
    pub emails: Vec<Email>,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub favorite: bool,
    /// Unix seconds.
    pub created: i64,
    /// Unix seconds.
    pub updated: i64,
}

impl Contact {
    #[must_use]
    pub fn new(id: String, name: &str, now: i64) -> Self {
        Self {
            id,
            name: name.trim().to_string(),
            phones: Vec::new(),
            emails: Vec::new(),
            note: String::new(),
            favorite: false,
            created: now,
            updated: now,
        }
    }

    /// First phone with a non-empty number — the "call/message" target.
    #[must_use]
    pub fn primary_phone(&self) -> Option<&Phone> {
        self.phones.iter().find(|p| !p.number.trim().is_empty())
    }

    /// First email with a non-empty address.
    #[must_use]
    pub fn primary_email(&self) -> Option<&Email> {
        self.emails.iter().find(|e| !e.address.trim().is_empty())
    }

    /// Save-validation contract: a contact needs a name or at least one
    /// dialable number — a nameless, numberless row is useless in every
    /// consumer (dialer lookup, SMS picker, this list).
    #[must_use]
    pub fn is_meaningful(&self) -> bool {
        !self.name.trim().is_empty() || self.phones.iter().any(|p| !p.number.trim().is_empty())
    }

    /// Drop empty phone/email rows (form buffers may leave blanks behind).
    pub fn prune_empty(&mut self) {
        self.phones
            .retain(|p| !p.number.trim().is_empty() || !p.label.trim().is_empty());
        self.emails
            .retain(|e| !e.address.trim().is_empty() || !e.label.trim().is_empty());
        self.phones.retain(|p| !p.number.trim().is_empty());
        self.emails.retain(|e| !e.address.trim().is_empty());
        self.name = self.name.trim().to_string();
        self.note = self.note.trim().to_string();
    }
}

// ---------------------------------------------------------------------------
// Sorting & search
// ---------------------------------------------------------------------------

/// Display order contract (shared with any other reader): favorites first,
/// then case-insensitive name, then id for stability. Thai has no case —
/// `to_lowercase` is a no-op there and correct for Latin.
pub fn sort_contacts(list: &mut [Contact]) {
    list.sort_by(|a, b| {
        b.favorite
            .cmp(&a.favorite)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.id.cmp(&b.id))
    });
}

/// Live search: name, any phone number (raw or formatted digits) or any
/// email address. Substring match, case-insensitive.
#[must_use]
pub fn contact_matches(ct: &Contact, query: &str) -> bool {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return true;
    }
    if ct.name.to_lowercase().contains(&q) {
        return true;
    }
    ct.phones
        .iter()
        .any(|p| p.number.to_lowercase().contains(&q) || digits_only(&p.number).contains(&q))
        || ct
            .emails
            .iter()
            .any(|e| e.address.to_lowercase().contains(&q))
}

/// Indices of contacts matching `query`, preserving sorted order.
#[must_use]
pub fn matching_indices(contacts: &[Contact], query: &str) -> Vec<usize> {
    contacts
        .iter()
        .enumerate()
        .filter(|(_, c)| contact_matches(c, query))
        .map(|(i, _)| i)
        .collect()
}

// ---------------------------------------------------------------------------
// Number helpers (ported from rabbiz-dialer model.rs so display matches the
// dialer's 3-3-4 Thai grouping)
// ---------------------------------------------------------------------------

/// Keep only dialable characters: digits, `*`, `#`, and a leading `+`.
#[must_use]
pub fn digits_only(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for (i, c) in s.chars().enumerate() {
        if c.is_ascii_digit() || c == '*' || c == '#' || (c == '+' && i == 0 && out.is_empty()) {
            out.push(c);
        }
    }
    out
}

/// Thai-style `3-3-4` grouping (`081-234-5678`), same as the dialer's
/// `format_number`. Non-digit-led strings are returned unchanged.
#[must_use]
pub fn format_number(d: &str) -> String {
    let s = d;
    if s.is_empty() {
        return String::new();
    }
    let first = s.chars().next().unwrap_or('0');
    if !first.is_ascii_digit() {
        return s.to_string();
    }
    let len = s.chars().count();
    if len <= 3 {
        return s.to_string();
    }
    if len <= 6 {
        return format!("{}-{}", &s[..3], &s[3..]);
    }
    if len <= 10 {
        return format!("{}-{}-{}", &s[..3], &s[3..6], &s[6..]);
    }
    format!("{}-{}-{}-{}", &s[..3], &s[3..6], &s[6..10], &s[10..])
}

// ---------------------------------------------------------------------------
// Avatar tint — the shared app-kit palette (`ui::avatar_tint`), so a contact
// looks identical in the dialer, contacts and sms apps.
// ---------------------------------------------------------------------------

#[must_use]
pub fn hash_color(seed: &str) -> [u8; 4] {
    rabbiz_app_kit::ui::avatar_tint(seed)
}

/// Stable per-contact tint: seeded by name, falling back to primary number.
#[must_use]
pub fn avatar_tint(ct: &Contact) -> [u8; 4] {
    if ct.name.trim().is_empty() {
        if let Some(p) = ct.primary_phone() {
            return hash_color(&p.number);
        }
        return hash_color(&ct.id);
    }
    hash_color(&ct.name)
}

// ---------------------------------------------------------------------------
// Time + ids
// ---------------------------------------------------------------------------

#[must_use]
pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// `"c-<unix-nanos>"` — collision-proof for human editing speeds.
#[must_use]
pub fn fresh_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("c-{nanos}")
}

// ---------------------------------------------------------------------------
// Demo seed — the same 8 contacts the QML shell dialer and the Rust dialer
// ship (`rabbiz_dialer::model::CONTACTS` / Dialer.qml `contactsList`). Used
// only on first run when no contacts.json exists, so a fresh device and the
// dialer agree out of the box. Once the user edits, the file is theirs.
// ---------------------------------------------------------------------------

#[must_use]
pub fn demo_seed(now: i64) -> Vec<Contact> {
    // (name, number, label, favorite)
    const SEED: [(&str, &str, &str, bool); 8] = [
        ("Mom", "0865551234", "mobile", true),
        ("Alex", "0812345678", "mobile", true),
        ("Sarah Chen", "0894442211", "work", true),
        ("Somchai", "0819876543", "mobile", false),
        ("Grab Driver", "0927778888", "trip", false),
        ("Veridian Bank", "021234567", "hotline", false),
        ("Emergency", "191", "emergency", false),
        ("Voicemail", "*100", "carrier", false),
    ];
    SEED.iter()
        .enumerate()
        .map(|(i, (name, number, label, fav))| {
            // Deterministic ids for the seed so re-seeding is idempotent.
            let mut c = Contact::new(format!("c-seed-{i}"), name, now);
            c.phones.push(Phone {
                label: (*label).to_string(),
                number: (*number).to_string(),
            });
            c.favorite = *fav;
            c
        })
        .collect()
}

// ---------------------------------------------------------------------------
// TextBuf — byte-cursor text buffer for the form fields (name, phone
// number/label, email, note) and the search field. Cursor stays on a char
// boundary; backspace/delete work in grapheme-ish clusters so Thai combining
// marks (สระ/วรรณยุกต์) never split. Same rules as rabbiz-notes model.rs.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct TextBuf {
    pub text: String,
    /// Byte offset of the cursor (char-boundary aligned).
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

    #[must_use]
    pub fn from_text(s: &str) -> Self {
        Self {
            text: s.to_string(),
            cursor: s.len(),
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    pub fn set(&mut self, s: &str) {
        self.text = s.to_string();
        self.cursor = self.text.len();
    }

    pub fn insert_str(&mut self, s: &str) {
        if s.is_empty() {
            return;
        }
        self.text.insert_str(self.cursor, s);
        self.cursor += s.len();
    }

    /// Delete up to `max_bytes` before the cursor in whole clusters.
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

    pub fn delete_grapheme_back(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        let p = prev_grapheme(&self.text, self.cursor);
        self.text.drain(p..self.cursor);
        self.cursor = p;
        true
    }

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

    pub fn line_home(&mut self) {
        self.cursor = self.text[..self.cursor.min(self.text.len())]
            .rfind('\n')
            .map_or(0, |i| i + 1);
    }

    pub fn line_end(&mut self) {
        let c = self.cursor.min(self.text.len());
        self.cursor = self.text[c..].find('\n').map_or(self.text.len(), |i| c + i);
    }
}

impl Default for TextBuf {
    fn default() -> Self {
        Self::new()
    }
}

// --- grapheme-ish boundary helpers (Thai marks stay glued to base char) ---

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
        | 0x20D0..=0x20FF
        | 0xFE00..=0xFE0F    // variation selectors
        | 0xFE20..=0xFE2F)
}

fn is_mark_at(s: &str, i: usize) -> bool {
    s.get(i..)
        .and_then(|t| t.chars().next())
        .is_some_and(is_mark)
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

#[must_use]
pub fn prev_grapheme(s: &str, i: usize) -> usize {
    let mut p = prev_boundary(s, i);
    while p > 0 && is_mark_at(s, p) {
        p = prev_boundary(s, p);
    }
    p
}

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
    fn sort_favorites_then_name() {
        let mut v = vec![
            Contact::new("c-3".into(), "zed", 0),
            Contact::new("c-1".into(), "Amy", 0),
            Contact::new("c-2".into(), "bob", 0),
        ];
        v[1].favorite = true;
        v[2].favorite = true;
        sort_contacts(&mut v);
        let order: Vec<&str> = v.iter().map(|c| c.name.as_str()).collect();
        // favorites first sorted by name (case-insensitive), then rest.
        assert_eq!(order, vec!["Amy", "bob", "zed"]);
    }

    #[test]
    fn sort_is_case_insensitive() {
        let mut v = vec![
            Contact::new("c-1".into(), "zoe", 0),
            Contact::new("c-2".into(), "Amy", 0),
            Contact::new("c-3".into(), "bob", 0),
        ];
        sort_contacts(&mut v);
        let order: Vec<&str> = v.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(order, vec!["Amy", "bob", "zoe"]);
    }

    #[test]
    fn search_matches_name_phone_email() {
        let mut c = Contact::new("c-1".into(), "สมชาย ใจดี", 0);
        c.phones.push(Phone {
            label: "mobile".into(),
            number: "081-234-5678".into(),
        });
        c.emails.push(Email {
            label: "home".into(),
            address: "Somchai@Example.com".into(),
        });
        assert!(contact_matches(&c, "สมชาย"));
        assert!(contact_matches(&c, "081234")); // digits run inside formatted number
        assert!(contact_matches(&c, "234-5678"));
        assert!(contact_matches(&c, "example.com"));
        assert!(!contact_matches(&c, "mom"));
    }

    #[test]
    fn format_number_groups() {
        assert_eq!(format_number("191"), "191");
        assert_eq!(format_number("0812345678"), "081-234-5678");
        assert_eq!(format_number("+66812345678"), "+66812345678");
        assert_eq!(format_number("*100"), "*100");
    }

    #[test]
    fn digits_only_keeps_plus_and_symbols() {
        assert_eq!(digits_only("+66 81 234 5678"), "+66812345678");
        assert_eq!(digits_only("12+34"), "1234");
        assert_eq!(digits_only("*100"), "*100");
    }

    #[test]
    fn thai_combining_marks_delete_as_cluster() {
        let mut b = TextBuf::from_text("aกิb"); // ิ = SARA I (0E34, below mark)
        b.cursor = "aกิ".len();
        assert!(b.delete_grapheme_back());
        assert_eq!(b.as_str(), "ab");
        assert_eq!(b.cursor, 1);
    }

    #[test]
    fn textbuf_utf8_safe() {
        let mut b = TextBuf::new();
        b.insert_str("สวัสดี");
        assert!(b.text.is_char_boundary(b.cursor));
        b.left(); // last grapheme cluster is "ดี" (ด + ี mark)
        assert_eq!(b.cursor, "สวัส".len());
        b.insert_str("X");
        assert_eq!(b.as_str(), "สวัสXดี");
    }

    #[test]
    fn meaningful_requires_name_or_number() {
        let mut c = Contact::new("c-1".into(), "  ", 0);
        assert!(!c.is_meaningful());
        c.phones.push(Phone {
            label: String::new(),
            number: "123".into(),
        });
        assert!(c.is_meaningful());
        c.name = "Ann".into();
        assert!(c.is_meaningful());
    }

    #[test]
    fn prune_drops_blank_rows() {
        let mut c = Contact::new("c-1".into(), " Ann ", 0);
        c.phones.push(Phone {
            label: String::new(),
            number: "  ".into(),
        });
        c.phones.push(Phone {
            label: "mobile".into(),
            number: "081".into(),
        });
        c.prune_empty();
        assert_eq!(c.phones.len(), 1);
        assert_eq!(c.name, "Ann");
    }

    #[test]
    fn demo_seed_has_eight_favorites_three() {
        let seed = demo_seed(1_700_000_000);
        assert_eq!(seed.len(), 8);
        assert_eq!(seed.iter().filter(|c| c.favorite).count(), 3);
        assert!(seed.iter().all(|c| c.id.starts_with("c-")));
        // Same set the dialer ships.
        assert!(seed
            .iter()
            .any(|c| c.name == "Mom" && c.primary_phone().unwrap().number == "0865551234"));
        assert!(seed.iter().any(|c| c.name == "Voicemail"));
    }
}
