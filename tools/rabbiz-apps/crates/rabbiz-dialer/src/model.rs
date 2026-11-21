//! Pure dialer logic — number formatting, key map, contact matching,
//! recents model and the simulated call state machine. No I/O, no Wayland;
//! everything here is unit-tested.
//!
//! Conventions mirror the removed shell-owned QML dialer
//! (`shell/apps/dialer/Dialer.qml`, git history) so behaviour stayed
//! identical for the user across the port.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Keypad
// ---------------------------------------------------------------------------

/// What the small sub-label under a digit is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubLabel {
    /// Nothing (symbols * #).
    None,
    /// T9 letters, e.g. "ABC".
    Letters(&'static str),
    /// Material Icons Round codepoint (verified against the bundled TTF).
    Icon(&'static str),
}

#[derive(Debug, Clone, Copy)]
pub struct DialKey {
    /// The digit/symbol inserted into the number.
    pub digit: &'static str,
    pub sub: SubLabel,
}

/// 4x3 pad, Android/M3 order. `1` carries the voicemail glyph, `0` the "+"
/// second-label, matching the shell dialer.
pub const KEYS: [DialKey; 12] = [
    DialKey {
        digit: "1",
        sub: SubLabel::Icon("\u{e0d9}"),
    }, // voicemail
    DialKey {
        digit: "2",
        sub: SubLabel::Letters("ABC"),
    },
    DialKey {
        digit: "3",
        sub: SubLabel::Letters("DEF"),
    },
    DialKey {
        digit: "4",
        sub: SubLabel::Letters("GHI"),
    },
    DialKey {
        digit: "5",
        sub: SubLabel::Letters("JKL"),
    },
    DialKey {
        digit: "6",
        sub: SubLabel::Letters("MNO"),
    },
    DialKey {
        digit: "7",
        sub: SubLabel::Letters("PQRS"),
    },
    DialKey {
        digit: "8",
        sub: SubLabel::Letters("TUV"),
    },
    DialKey {
        digit: "9",
        sub: SubLabel::Letters("WXYZ"),
    },
    DialKey {
        digit: "*",
        sub: SubLabel::None,
    },
    DialKey {
        digit: "0",
        sub: SubLabel::Letters("+"),
    },
    DialKey {
        digit: "#",
        sub: SubLabel::None,
    },
];

/// In-call DTMF pad: `* 0 #` bottom row like the stock dialer.
pub const DTMF_KEYS: [&str; 12] = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "*", "0", "#"];

/// Keep only dialable characters: digits, `*`, `#`, and a leading `+`.
/// Port of `digitsOnly()` in Dialer.qml.
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

/// Display formatting: Thai-style `3-3-4` grouping (`081-234-5678`), same
/// as `formatNumber()` in Dialer.qml. Strings that are not plain digits
/// (leading `+`, `*`, `#`) are returned unchanged.
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
// Contacts — one row per (contact, phone) pair so a contact with two numbers
// matches and dials either. The resolved list comes from the shared
// contacts store (`contacts.rs` reads `$XDG_DATA_HOME/rabbiz-contacts/
// contacts.json` written by rabbiz-contacts) when that file exists and
// parses; otherwise the built-in demo list below keeps demo mode working.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Contact {
    pub name: String,
    pub number: String,
    /// Display label for the number ("Mobile", "Work", … — free-form from
    /// the store, display-cased).
    pub label: String,
    /// Avatar tint, sRGB.
    pub color: [u8; 4],
    pub favorite: bool,
}

fn bc(
    name: &'static str,
    number: &'static str,
    label: &'static str,
    color: [u8; 4],
    favorite: bool,
) -> Contact {
    Contact {
        name: name.to_string(),
        number: number.to_string(),
        label: label.to_string(),
        color,
        favorite,
    }
}

/// Built-in demo contacts — identical set to `contactsList` in the shell
/// dialer and to the contacts store's first-run seed. Used only when no
/// `contacts.json` exists or it fails to parse.
#[must_use]
pub fn builtin_contacts() -> Vec<Contact> {
    vec![
        bc(
            "Mom",
            "0865551234",
            "Mobile",
            [0xEC, 0x48, 0x99, 0xFF],
            true,
        ),
        bc(
            "Alex",
            "0812345678",
            "Mobile",
            [0x3B, 0x82, 0xF6, 0xFF],
            true,
        ),
        bc(
            "Sarah Chen",
            "0894442211",
            "Work",
            [0x10, 0xB9, 0x81, 0xFF],
            true,
        ),
        bc(
            "Somchai",
            "0819876543",
            "Mobile",
            [0xF5, 0x9E, 0x0B, 0xFF],
            false,
        ),
        bc(
            "Grab Driver",
            "0927778888",
            "Trip",
            [0x22, 0xC5, 0x5E, 0xFF],
            false,
        ),
        bc(
            "Veridian Bank",
            "021234567",
            "Hotline",
            [0x8B, 0x5C, 0xF6, 0xFF],
            false,
        ),
        bc(
            "Emergency",
            "191",
            "Emergency",
            [0xEF, 0x44, 0x44, 0xFF],
            false,
        ),
        bc(
            "Voicemail",
            "*100",
            "Carrier",
            [0x64, 0x74, 0x8B, 0xFF],
            false,
        ),
    ]
}

/// Exact digit-match first (shell behaviour), then suffix match so a
/// country-code or area-code variant still resolves caller ID. A leading
/// `+`/`66` country prefix is normalized to the local `0` trunk form so
/// `+66 81 …` still matches a contact stored as `081 …`.
#[must_use]
pub fn lookup_in<'a>(contacts: &'a [Contact], number: &str) -> Option<&'a Contact> {
    let raw = digits_only(number);
    if raw.is_empty() {
        return None;
    }
    let bare = raw.strip_prefix('+').unwrap_or(&raw);
    // Candidate dial strings: as-entered, plus `0`-trunk form for +66/66.
    let mut candidates: Vec<String> = vec![bare.to_string()];
    if bare.starts_with("66") && bare.len() > 4 {
        candidates.push(format!("0{}", &bare[2..]));
    } else if bare.starts_with('0') && bare.len() >= 9 {
        // Reverse fold: a local `0…` number also matches a contact stored
        // in `+66…`/`66…` international form.
        candidates.push(format!("66{}", &bare[1..]));
    }
    for target in &candidates {
        for ct in contacts {
            // Strip a stored leading `+`: candidates are always bare.
            let cn = digits_only(&ct.number);
            let cn = cn.strip_prefix('+').unwrap_or(&cn);
            if cn == *target {
                return Some(ct);
            }
        }
    }
    // Suffix match only for real-length numbers to avoid "1" matching "191".
    for target in &candidates {
        if target.len() < 9 {
            continue;
        }
        for ct in contacts {
            let cn = digits_only(&ct.number);
            let cn = cn.strip_prefix('+').unwrap_or(&cn);
            if cn.len() >= 9 && (target.ends_with(cn) || cn.ends_with(target.as_str())) {
                return Some(ct);
            }
        }
    }
    None
}

/// Case-insensitive name/number substring filter for the Contacts tab.
#[must_use]
pub fn contact_matches(ct: &Contact, query: &str) -> bool {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return true;
    }
    ct.name.to_lowercase().contains(&q) || ct.number.contains(&q)
}

// ---------------------------------------------------------------------------
// Call log (recents)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CallDir {
    Missed,
    In,
    Out,
}

impl CallDir {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Missed => "Missed",
            Self::In => "Incoming",
            Self::Out => "Outgoing",
        }
    }

    /// Material Icons Round codepoint.
    #[must_use]
    pub fn icon(self) -> &'static str {
        match self {
            Self::Missed => "\u{e0b4}", // call_missed
            Self::In => "\u{e0b5}",     // call_received
            Self::Out => "\u{e0b2}",    // call_made
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallEntry {
    pub number: String,
    /// Contact name snapshot at insert time ("" when unknown).
    pub name: String,
    pub dir: CallDir,
    /// Unix seconds.
    pub unix: i64,
}

/// Call history, newest first. Persisted as JSON by the app layer.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Recents {
    pub entries: Vec<CallEntry>,
}

impl Recents {
    pub const MAX: usize = 100;

    /// Insert at the top. Consecutive repeats of the same number+direction
    /// within 10 minutes are merged (Android bumps the existing row).
    /// `name` is the caller-resolved contact name snapshot ("" when unknown)
    /// — resolution lives in `contacts.rs` so this stays pure.
    pub fn add(&mut self, number: &str, name: &str, dir: CallDir, unix: i64) {
        if let Some(first) = self.entries.first_mut() {
            if first.number == number && first.dir == dir && unix - first.unix < 600 {
                first.unix = unix;
                return;
            }
        }
        self.entries.insert(
            0,
            CallEntry {
                number: number.to_string(),
                name: name.to_string(),
                dir,
                unix,
            },
        );
        self.entries.truncate(Self::MAX);
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Same demo seed the shell dialer ships, used only when no persisted
    /// log exists yet.
    #[must_use]
    pub fn demo_seed(now: i64) -> Self {
        let day = 86_400;
        let mut r = Self::default();
        let seeds: [(&str, &str, CallDir, i64); 5] = [
            ("Alex", "0812345678", CallDir::Missed, now - 3_600),
            ("Mom", "0865551234", CallDir::In, now - day - 7_200),
            (
                "Grab Driver",
                "0927778888",
                CallDir::Out,
                now - day - 10_800,
            ),
            (
                "Sarah Chen",
                "0894442211",
                CallDir::In,
                now - 3 * day - 5_400,
            ),
            (
                "Veridian Bank",
                "021234567",
                CallDir::Out,
                now - 4 * day - 3_600,
            ),
        ];
        for (name, number, dir, unix) in seeds {
            r.entries.push(CallEntry {
                number: number.to_string(),
                name: name.to_string(),
                dir,
                unix,
            });
        }
        r
    }
}

// ---------------------------------------------------------------------------
// Relative timestamps ("09:41", "Yesterday", "Mon", "12/03")
// ---------------------------------------------------------------------------

/// Label for a past call. Uses libc local time via the caller-supplied
/// `now`; pure day arithmetic keeps this testable.
#[must_use]
pub fn when_label(now_unix: i64, then_unix: i64) -> String {
    // Day index in UTC is fine for "same day / yesterday" bucketing.
    let day = |t: i64| t.div_euclid(86_400);
    let diff = day(now_unix) - day(then_unix);
    if diff <= 0 {
        return hhmm(then_unix);
    }
    if diff == 1 {
        return "Yesterday".to_string();
    }
    if diff < 7 {
        return weekday_name(then_unix);
    }
    format!("{}/{}", day_month(then_unix).0, day_month(then_unix).1)
}

#[allow(unsafe_code)]
fn hhmm(unix: i64) -> String {
    // Local time via libc.
    let t = unix as libc::time_t;
    // SAFETY: `tm` is a zeroed, owned `libc::tm`; `localtime_r` is reentrant
    // and only reads `&t` and writes into `tm`. `tm_zone` is never read.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&t, &mut tm) };
    format!("{:02}:{:02}", tm.tm_hour, tm.tm_min)
}

#[allow(unsafe_code)]
fn day_month(unix: i64) -> (i32, i32) {
    let t = unix as libc::time_t;
    // SAFETY: `tm` is a zeroed, owned `libc::tm`; `localtime_r` is reentrant
    // and only reads `&t` and writes into `tm`. `tm_zone` is never read.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&t, &mut tm) };
    (tm.tm_mday, tm.tm_mon + 1)
}

#[allow(unsafe_code)]
fn weekday_name(unix: i64) -> String {
    const NAMES: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    let t = unix as libc::time_t;
    // SAFETY: `tm` is a zeroed, owned `libc::tm`; `localtime_r` is reentrant
    // and only reads `&t` and writes into `tm`. `tm_zone` is never read.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&t, &mut tm) };
    NAMES[tm.tm_wday.clamp(0, 6) as usize].to_string()
}

// ---------------------------------------------------------------------------
// Call state machine (simulated progression + mmcli bookkeeping)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallPhase {
    /// Outgoing call placed, waiting for the far end.
    Dialing,
    /// Ringing (outgoing ringback or incoming ring).
    Ringing,
    /// Connected; `secs` counts up.
    Active,
}

/// Timing for the simulated call progression. Mirrors Telephony.qml's demo
/// timers (dialing 1.5 s → ringing 2 s → active; unanswered incoming drops
/// after 30 s).
pub const DIALING_S: f32 = 1.5;
pub const RINGING_S: f32 = 2.0;
pub const INCOMING_RING_TIMEOUT_S: f32 = 30.0;

#[derive(Debug)]
pub struct CallSession {
    /// `Out` for dialed calls, `In` for incoming.
    pub dir: CallDir,
    pub phase: CallPhase,
    /// Seconds spent in the current phase.
    pub phase_t: f32,
    /// Seconds since the call became active.
    pub secs: f32,
    /// Whether an incoming call was answered (decides In vs Missed).
    pub answered: bool,
    pub number: String,
    /// mmcli call object path when the ModemManager backend placed the call.
    pub backend_path: Option<String>,
    pub dtmf_open: bool,
    pub muted: bool,
    pub speaker: bool,
    /// Last DTMF string sent (debug/status echo like Telephony.lastDtmf).
    pub last_dtmf: String,
}

impl CallSession {
    #[must_use]
    pub fn outgoing(number: String, backend_path: Option<String>) -> Self {
        Self {
            dir: CallDir::Out,
            phase: CallPhase::Dialing,
            phase_t: 0.0,
            secs: 0.0,
            answered: true,
            number,
            backend_path,
            dtmf_open: false,
            muted: false,
            speaker: false,
            last_dtmf: String::new(),
        }
    }

    #[must_use]
    pub fn incoming(number: String) -> Self {
        Self {
            dir: CallDir::In,
            phase: CallPhase::Ringing,
            phase_t: 0.0,
            secs: 0.0,
            answered: false,
            number,
            backend_path: None,
            dtmf_open: false,
            muted: false,
            speaker: false,
            last_dtmf: String::new(),
        }
    }

    /// Advance the simulated progression. Returns `true` while the session
    /// still needs ticks (always true — callers end it explicitly).
    pub fn tick(&mut self, dt: f32) {
        self.phase_t += dt;
        if self.phase == CallPhase::Active {
            self.secs += dt;
        } else if self.phase == CallPhase::Dialing && self.phase_t >= DIALING_S {
            self.phase = CallPhase::Ringing;
            self.phase_t = 0.0;
        } else if self.phase == CallPhase::Ringing
            && self.dir == CallDir::Out
            && self.phase_t >= RINGING_S
        {
            self.phase = CallPhase::Active;
            self.phase_t = 0.0;
        }
    }

    /// Incoming ring timed out unanswered → the log records a missed call.
    #[must_use]
    pub fn incoming_missed(&self) -> bool {
        self.dir == CallDir::In && !self.answered && self.phase_t >= INCOMING_RING_TIMEOUT_S
    }

    /// Answer an incoming call.
    pub fn answer(&mut self) {
        if self.dir == CallDir::In && self.phase == CallPhase::Ringing {
            self.phase = CallPhase::Active;
            self.phase_t = 0.0;
            self.answered = true;
        }
    }

    /// Direction recorded in the call log when the session ends.
    #[must_use]
    pub fn log_dir(&self) -> CallDir {
        match self.dir {
            CallDir::Out => CallDir::Out,
            CallDir::In => {
                if self.answered || self.phase == CallPhase::Active {
                    CallDir::In
                } else {
                    CallDir::Missed
                }
            }
            // A session never starts as Missed; kept for exhaustiveness.
            CallDir::Missed => CallDir::Missed,
        }
    }

    /// `mm:ss` elapsed for the active call.
    #[must_use]
    pub fn elapsed_label(&self) -> String {
        let s = self.secs.max(0.0) as u32;
        format!("{:02}:{:02}", s / 60, s % 60)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digits_only_filters() {
        assert_eq!(digits_only("081-234 5678"), "0812345678");
        assert_eq!(digits_only("+66 81 234 5678"), "+66812345678");
        assert_eq!(digits_only("12+34"), "1234");
        assert_eq!(digits_only("*100"), "*100");
        assert_eq!(digits_only("call 191 now"), "191");
    }

    #[test]
    fn format_number_groups() {
        assert_eq!(format_number(""), "");
        assert_eq!(format_number("191"), "191");
        assert_eq!(format_number("0812"), "081-2");
        assert_eq!(format_number("081234"), "081-234");
        assert_eq!(format_number("0812345678"), "081-234-5678");
        assert_eq!(format_number("08123456789"), "081-234-5678-9");
        assert_eq!(format_number("+66812345678"), "+66812345678");
        assert_eq!(format_number("*100"), "*100");
    }

    #[test]
    fn lookup_exact_and_suffix() {
        let contacts = builtin_contacts();
        let name = |n: &str| lookup_in(&contacts, n).map(|c| c.name.as_str());
        assert_eq!(name("0812345678"), Some("Alex"));
        assert_eq!(name("+66812345678"), Some("Alex"));
        assert_eq!(name("66812345678"), Some("Alex"));
        assert_eq!(name("123"), None);
        assert_eq!(name("999999999"), None);
    }

    #[test]
    fn lookup_matches_any_phone_of_a_contact() {
        // A store contact with two numbers resolves through either one.
        let mut contacts = builtin_contacts();
        let mut duo = contacts[1].clone();
        duo.number = "0891112222".to_string();
        contacts.push(duo);
        // Second entry shares the name but a different number — both dial.
        assert_eq!(
            lookup_in(&contacts, "0891112222").map(|c| c.name.as_str()),
            Some("Alex")
        );
        assert_eq!(
            lookup_in(&contacts, "+66 89 111 2222").map(|c| c.name.as_str()),
            Some("Alex")
        );
    }

    #[test]
    fn contact_filter() {
        let contacts = builtin_contacts();
        let mom = contacts.iter().find(|c| c.name == "Mom").unwrap();
        assert!(contact_matches(mom, ""));
        assert!(contact_matches(mom, "mom"));
        assert!(contact_matches(mom, "0865"));
        assert!(!contact_matches(mom, "alex"));
    }

    #[test]
    fn recents_add_merge_and_order() {
        let mut r = Recents::default();
        r.add("0812345678", "Alex", CallDir::Out, 1_000);
        r.add("0812345678", "", CallDir::Out, 1_100); // merged (same dir, <10 min)
        assert_eq!(r.entries.len(), 1);
        assert_eq!(r.entries[0].unix, 1_100);
        r.add("0812345678", "Alex", CallDir::Missed, 1_200); // different dir -> new row
        assert_eq!(r.entries.len(), 2);
        assert_eq!(r.entries[0].dir, CallDir::Missed);
        assert_eq!(r.entries[0].name, "Alex"); // name snapshot passed by caller
        r.clear();
        assert!(r.entries.is_empty());
    }

    #[test]
    fn recents_serde_roundtrip() {
        let mut r = Recents::default();
        r.add("0865551234", "Mom", CallDir::In, 1_700_000_000);
        let s = serde_json::to_string(&r).unwrap();
        let back: Recents = serde_json::from_str(&s).unwrap();
        assert_eq!(back.entries.len(), 1);
        assert_eq!(back.entries[0].dir, CallDir::In);
    }

    #[test]
    fn call_progression() {
        let mut c = CallSession::outgoing("0812345678".into(), None);
        assert_eq!(c.phase, CallPhase::Dialing);
        c.tick(DIALING_S);
        assert_eq!(c.phase, CallPhase::Ringing);
        c.tick(RINGING_S);
        assert_eq!(c.phase, CallPhase::Active);
        c.tick(65.0);
        assert_eq!(c.elapsed_label(), "01:05");
        assert_eq!(c.log_dir(), CallDir::Out);
    }

    #[test]
    fn incoming_missed_vs_answered() {
        let mut c = CallSession::incoming("0865551234".into());
        assert_eq!(c.phase, CallPhase::Ringing);
        c.tick(INCOMING_RING_TIMEOUT_S);
        assert!(c.incoming_missed());
        assert_eq!(c.log_dir(), CallDir::Missed);

        let mut c2 = CallSession::incoming("0865551234".into());
        c2.answer();
        assert_eq!(c2.phase, CallPhase::Active);
        assert!(c2.answered);
        assert_eq!(c2.log_dir(), CallDir::In);
    }

    #[test]
    fn when_label_buckets() {
        let now = 1_800_000_000i64;
        assert!(when_label(now, now - 60).contains(':'));
        assert_eq!(when_label(now, now - 86_400 + 60), "Yesterday");
        // 3 days ago -> weekday name
        let w = when_label(now, now - 3 * 86_400);
        assert!(["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"].contains(&w.as_str()));
        // 30 days ago -> d/m
        let old = when_label(now, now - 30 * 86_400);
        assert!(old.contains('/'));
    }
}
