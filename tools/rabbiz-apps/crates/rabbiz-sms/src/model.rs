//! Pure message/conversation model — no UI, no I/O. Everything here is
//! unit-tested: UTF-8-safe editing, Thai-aware text ops, phone formatting,
//! run grouping and local-time formatting.

/// Delivery state of an outgoing message. Incoming messages carry `Sent`
/// (they arrived) and the field is ignored for them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MsgState {
    /// Saved locally, modem send not attempted yet (or modem offline).
    Queued,
    /// Accepted by the modem backend.
    Sent,
    /// Send attempted and rejected (bad number, modem error).
    #[default]
    Failed,
}

/// One SMS. `peer` is the counterpart number/address exactly as entered;
/// display formatting lives in [`format_phone`]. `outgoing` = sent by us.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Message {
    pub id: u64,
    pub peer: String,
    pub body: String,
    pub outgoing: bool,
    /// Unix seconds.
    pub ts: i64,
    #[serde(default)]
    pub state: MsgState,
}

impl Message {
    #[must_use]
    pub fn new(id: u64, peer: &str, body: &str, outgoing: bool, ts: i64) -> Self {
        Self {
            id,
            peer: peer.trim().to_string(),
            body: body.to_string(),
            outgoing,
            ts,
            state: if outgoing {
                MsgState::Queued
            } else {
                MsgState::Sent
            },
        }
    }
}

/// Messages closer together than this (same peer + direction) render as one
/// bubble run with a single timestamp — the Google Chat / M3 conversation
/// pattern (10-minute window).
pub const RUN_GAP_SECS: i64 = 600;

/// Does `cur` continue the bubble run started by `prev`?
#[must_use]
pub fn same_run(prev: &Message, cur: &Message) -> bool {
    prev.peer == cur.peer
        && prev.outgoing == cur.outgoing
        && cur.ts >= prev.ts
        && cur.ts - prev.ts <= RUN_GAP_SECS
}

/// Threading key: digits-only so "+66 81…", "081…" and "081-…" land in one
/// conversation. Non-numeric peers (e.g. alphanumeric sender ids) keep their
/// trimmed text.
#[must_use]
pub fn thread_key(peer: &str) -> String {
    let digits: String = peer.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() >= 3 {
        // Fold Thai international (+66…) into national (0…) so both spellings
        // of the same handset land in one thread.
        if digits.len() == 11 && digits.starts_with("66") {
            return format!("0{}", &digits[2..]);
        }
        digits
    } else {
        peer.trim().to_string()
    }
}

/// Remove `bytes` worth of UTF-8 text from the end, never splitting a
/// character. `zwp_text_input_v3.delete_surrounding_text` reports lengths in
/// bytes, but we treat the value defensively: back off to a char boundary,
/// and if the result would not shrink the string (count smaller than the
/// last char's UTF-8 length), remove the last whole char instead. Thai
/// combining sequences may still split at codepoint level — the bundled
/// fonts have no grapheme segmentation available without new deps.
pub fn delete_before(s: &mut String, bytes: usize) {
    if s.is_empty() || bytes == 0 {
        return;
    }
    let mut target = s.len().saturating_sub(bytes);
    while target > 0 && !s.is_char_boundary(target) {
        target -= 1;
    }
    if target == s.len() {
        // `bytes` smaller than the trailing char: still remove that char.
        s.pop();
    } else {
        s.truncate(target);
    }
}

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

/// "HH:MM" 24-hour local time.
#[must_use]
pub fn format_time(ts: i64) -> String {
    let t = local_time(ts);
    format!("{:02}:{:02}", t.hour, t.min)
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Day separator label: "Today", "Yesterday", or "12 Nov" / "12 Nov 2025".
#[must_use]
pub fn day_label(ts: i64, now: i64) -> String {
    let a = local_time(ts);
    let b = local_time(now);
    if a.year == b.year && a.yday == b.yday {
        return "Today".to_string();
    }
    if days_between(&a, &b) == 1 {
        return "Yesterday".to_string();
    }
    let mon = MONTHS[(a.month as usize).saturating_sub(1).min(11)];
    if a.year == b.year {
        format!("{} {}", a.day, mon)
    } else {
        format!("{} {} {}", a.day, mon, a.year)
    }
}

/// Whole days from `a` to `b` in local time (approximation via civil days).
fn days_between(a: &LocalTime, b: &LocalTime) -> i64 {
    civil_day(b.year, b.month, b.day) - civil_day(a.year, a.month, a.day)
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

/// Display form of a peer address. Thai mobiles (`081…`–`099…`, 10 digits)
/// render as `+66 81 234 5678`; other digit strings get light 3-3-3 grouping;
/// anything else is shown as entered.
#[must_use]
pub fn format_phone(raw: &str) -> String {
    let t = raw.trim();
    if t.is_empty() {
        return String::new();
    }
    let digits: String = t.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return t.to_string();
    }
    if digits.len() == 10 && digits.starts_with('0') {
        let nat = &digits[1..]; // 9 digits
                                // Mobile prefixes 06/08/09 (and 02/3x landlines fall through to
                                // generic grouping — still readable).
        return format!("+66 {} {} {}", &nat[0..2], &nat[2..5], &nat[5..9]);
    }
    if digits.len() == 11 && digits.starts_with("66") {
        let nat = &digits[2..];
        return format!("+66 {} {} {}", &nat[0..2], &nat[2..5], &nat[5..9]);
    }
    // Generic: group in threes from the right.
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

/// `(thread_key, display_peer, last_msg_index)` per conversation,
/// most-recent-activity first. Shared by the GUI list view and the
/// headless `--cli`/`--mcp` surfaces so all three agree on threading.
#[must_use]
pub fn threads(msgs: &[Message]) -> Vec<(String, String, usize)> {
    let mut map: Vec<(String, String, usize)> = Vec::new();
    for (i, m) in msgs.iter().enumerate() {
        let key = thread_key(&m.peer);
        if let Some(e) = map.iter_mut().find(|(k, _, _)| *k == key) {
            if m.ts >= msgs[e.2].ts {
                e.1 = m.peer.clone();
                e.2 = i;
            }
        } else {
            map.push((key, m.peer.clone(), i));
        }
    }
    map.sort_by_key(|(_, _, i)| std::cmp::Reverse(msgs[*i].ts));
    map
}

/// Short state caption drawn inside/under an outgoing bubble.
#[must_use]
pub fn state_label(s: MsgState) -> &'static str {
    match s {
        MsgState::Queued => "Queued",
        MsgState::Sent => "Sent",
        MsgState::Failed => "Failed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(id: u64, peer: &str, outgoing: bool, ts: i64) -> Message {
        Message::new(id, peer, "x", outgoing, ts)
    }

    #[test]
    fn delete_before_never_splits_utf8() {
        // "สวัสดี" — Thai chars are 3 bytes each.
        let mut s = "สวัสดี".to_string(); // 15 bytes
        delete_before(&mut s, 3);
        assert_eq!(s, "สวัสด");
        delete_before(&mut s, 1); // mid-char byte count → still removes one char
        assert_eq!(s, "สวัส");
        delete_before(&mut s, 99);
        assert!(s.is_empty());
        // ASCII sanity.
        let mut a = "hello".to_string();
        delete_before(&mut a, 2);
        assert_eq!(a, "hel");
        // Combining sequence: bytes land inside a multi-char grapheme but
        // still on a codepoint boundary — removes whole codepoints only.
        let mut t = "ก้".to_string(); // base + tone mark (6 bytes)
        delete_before(&mut t, 3);
        assert!(t.is_char_boundary(t.len()));
        assert_eq!(t, "ก");
    }

    #[test]
    fn delete_before_zero_is_noop() {
        let mut s = "abc".to_string();
        delete_before(&mut s, 0);
        assert_eq!(s, "abc");
    }

    #[test]
    fn thread_key_normalizes_phone_formats() {
        assert_eq!(thread_key("081-234-5678"), thread_key("+66 81 234 5678"));
        assert_eq!(thread_key(" 0812345678 "), "0812345678");
        assert_eq!(thread_key("abc"), "abc");
    }

    #[test]
    fn same_run_respects_direction_gap_and_peer() {
        let a = msg(1, "0812345678", true, 1000);
        let b = msg(2, "0812345678", true, 1000 + RUN_GAP_SECS);
        let c = msg(3, "0812345678", true, 1000 + RUN_GAP_SECS + 1);
        let d = msg(4, "0812345678", false, 1001);
        let e = msg(5, "0899999999", true, 1001);
        assert!(same_run(&a, &b));
        assert!(!same_run(&a, &c));
        assert!(!same_run(&a, &d));
        assert!(!same_run(&a, &e));
        // Out-of-order timestamps don't crash or group.
        let old = msg(6, "0812345678", true, 500);
        assert!(!same_run(&a, &old));
    }

    #[test]
    fn format_phone_thai_mobile() {
        assert_eq!(format_phone("0812345678"), "+66 81 234 5678");
        assert_eq!(format_phone("+66812345678"), "+66 81 234 5678");
        assert_eq!(format_phone(" 081-234-5678 "), "+66 81 234 5678");
    }

    #[test]
    fn format_phone_generic_and_text() {
        assert_eq!(format_phone(""), "");
        assert_eq!(format_phone("hello"), "hello");
        assert_eq!(format_phone("12345"), "12 345");
        assert_eq!(format_phone("+15551234567"), "15 551 234 567");
    }

    #[test]
    fn time_and_day_labels() {
        // 2026-01-02 03:04:05 UTC — local parts depend on TZ, so assert shape.
        let ts = 1_767_326_645;
        let t = format_time(ts);
        assert_eq!(t.len(), 5);
        assert_eq!(&t[2..3], ":");
        let now = ts;
        assert_eq!(day_label(ts, now), "Today");
        assert_eq!(day_label(ts - 86_400, now), "Yesterday");
        assert_ne!(day_label(ts - 200 * 86_400, now), "Today");
    }
}
