//! Bilingual (`en` / `th`) string catalogs for RabbizOS DE.
//!
//! Embedded at compile time via [`include_str!`] and parsed with a minimal
//! zero-dependency TOML subset parser (`[section]` headers + `key = "value"`).

use std::collections::BTreeMap;
use std::fmt;
use std::sync::OnceLock;

/// Embedded English string catalog (`locales/en.toml`).
pub const EN_TOML: &str = include_str!("../locales/en.toml");

/// Embedded Thai string catalog (`locales/th.toml`).
pub const TH_TOML: &str = include_str!("../locales/th.toml");

/// Supported UI locales.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Locale {
    #[default]
    En,
    Th,
}

impl Locale {
    /// All supported locales in display order.
    pub const ALL: [Locale; 2] = [Locale::En, Locale::Th];

    /// BCP-47 short language code.
    pub const fn code(self) -> &'static str {
        match self {
            Locale::En => "en",
            Locale::Th => "th",
        }
    }

    /// Native display name for language pickers.
    pub const fn native_name(self) -> &'static str {
        match self {
            Locale::En => "English",
            Locale::Th => "ไทย",
        }
    }

    /// Parse a locale tag such as `"en"`, `"en_US.UTF-8"`, `"th"`, or `"th-TH"`.
    pub fn from_code(raw: &str) -> Option<Self> {
        let primary = raw
            .trim()
            .split(['.', '_', '-'])
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        match primary.as_str() {
            "en" => Some(Locale::En),
            "th" => Some(Locale::Th),
            _ => None,
        }
    }
}

/// Error returned when parsing a locale TOML catalog fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for ParseError {}

/// Parsed key-value string catalog for a single locale.
#[derive(Debug, Clone)]
pub struct Strings {
    entries: BTreeMap<String, &'static str>,
}

impl Strings {
    /// Return the cached static catalog for `locale`.
    pub fn for_locale(locale: Locale) -> &'static Self {
        static EN_CATALOG: OnceLock<Strings> = OnceLock::new();
        static TH_CATALOG: OnceLock<Strings> = OnceLock::new();
        match locale {
            Locale::En => EN_CATALOG.get_or_init(|| {
                Self::parse(EN_TOML).expect("embedded locales/en.toml must be valid")
            }),
            Locale::Th => TH_CATALOG.get_or_init(|| {
                Self::parse(TH_TOML).expect("embedded locales/th.toml must be valid")
            }),
        }
    }

    /// Parse a static TOML string containing `[section]` tables and `key = "value"` pairs.
    pub fn parse(src: &'static str) -> Result<Self, ParseError> {
        let mut entries = BTreeMap::new();
        let mut section = String::new();

        for (idx, raw_line) in src.lines().enumerate() {
            let line_no = idx + 1;
            let trimmed = strip_comment(raw_line).trim();
            if trimmed.is_empty() {
                continue;
            }

            if trimmed.starts_with('[') {
                if !trimmed.ends_with(']') {
                    return Err(ParseError {
                        line: line_no,
                        message: "unclosed section header".to_string(),
                    });
                }
                let name = trimmed[1..trimmed.len() - 1].trim();
                if name.is_empty() || !is_valid_ident(name) {
                    return Err(ParseError {
                        line: line_no,
                        message: format!("invalid section name {name:?}"),
                    });
                }
                section = name.to_string();
                continue;
            }

            let Some((raw_key, raw_val)) = trimmed.split_once('=') else {
                return Err(ParseError {
                    line: line_no,
                    message: "expected `key = \"value\"`".to_string(),
                });
            };

            let key = raw_key.trim();
            if key.is_empty() || !is_valid_ident(key) {
                return Err(ParseError {
                    line: line_no,
                    message: format!("invalid key name {key:?}"),
                });
            }

            let value = parse_quoted_slice(raw_val.trim()).map_err(|msg| ParseError {
                line: line_no,
                message: msg.to_string(),
            })?;

            let full_key = if section.is_empty() {
                key.to_string()
            } else {
                format!("{section}.{key}")
            };

            if entries.insert(full_key.clone(), value).is_some() {
                return Err(ParseError {
                    line: line_no,
                    message: format!("duplicate key {full_key:?}"),
                });
            }
        }

        Ok(Self { entries })
    }

    /// Look up a qualified key (e.g. `"lock.touch_to_unlock"`).
    pub fn get(&self, key: &str) -> Option<&'static str> {
        self.entries.get(key).copied()
    }

    /// Number of entries in this catalog.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if the catalog has no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterate over all qualified keys in sorted order.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }

    /// Iterate over all `(key, value)` pairs in sorted order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &'static str)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), *v))
    }
}

/// Look up a localized string for `locale` and `key`, falling back to English
/// or `""` if the key does not exist.
pub fn tr(locale: Locale, key: &str) -> &'static str {
    Strings::for_locale(locale)
        .get(key)
        .or_else(|| Strings::for_locale(Locale::En).get(key))
        .unwrap_or("")
}

fn is_valid_ident(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn strip_comment(line: &str) -> &str {
    let mut in_double = false;
    let mut in_single = false;
    for (idx, ch) in line.char_indices() {
        match ch {
            '"' if !in_single => in_double = !in_double,
            '\'' if !in_double => in_single = !in_single,
            '#' if !in_double && !in_single => return &line[..idx],
            _ => {}
        }
    }
    line
}

fn parse_quoted_slice(raw: &'static str) -> Result<&'static str, &'static str> {
    let quote = raw.chars().next().ok_or("missing string value")?;
    if quote != '"' && quote != '\'' {
        return Err("value must be a quoted string");
    }
    let rest = &raw[1..];
    let Some(end) = rest.find(quote) else {
        return Err("unclosed quoted string");
    };
    if !rest[end + 1..].trim().is_empty() {
        return Err("unexpected trailing characters after string");
    }
    Ok(&rest[..end])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn is_thai_char(ch: char) -> bool {
        ('\u{0E01}'..='\u{0E5B}').contains(&ch)
    }

    #[test]
    fn en_and_th_parse_cleanly() {
        let en = Strings::parse(EN_TOML).expect("en.toml failed to parse");
        let th = Strings::parse(TH_TOML).expect("th.toml failed to parse");
        assert!(!en.is_empty());
        assert!(!th.is_empty());
    }

    #[test]
    fn en_and_th_have_exact_key_parity() {
        let en = Strings::for_locale(Locale::En);
        let th = Strings::for_locale(Locale::Th);

        let en_keys: BTreeSet<&str> = en.keys().collect();
        let th_keys: BTreeSet<&str> = th.keys().collect();

        let missing_in_th: Vec<&&str> = en_keys.difference(&th_keys).collect();
        let extra_in_th: Vec<&&str> = th_keys.difference(&en_keys).collect();

        assert!(
            missing_in_th.is_empty() && extra_in_th.is_empty(),
            "locale key mismatch: missing_in_th={missing_in_th:?}, extra_in_th={extra_in_th:?}"
        );
    }

    #[test]
    fn all_values_are_non_empty() {
        for locale in Locale::ALL {
            let catalog = Strings::for_locale(locale);
            for (k, v) in catalog.iter() {
                assert!(
                    !v.trim().is_empty(),
                    "empty value for key {k:?} in locale {:?}",
                    locale
                );
                assert!(
                    !v.contains('\u{FFFD}'),
                    "replacement character in key {k:?} for locale {:?}",
                    locale
                );
            }
        }
    }

    #[test]
    fn thai_catalog_contains_valid_thai_characters() {
        let th = Strings::for_locale(Locale::Th);
        for (key, value) in th.iter() {
            assert!(
                value.chars().any(is_thai_char),
                "Thai catalog entry {key:?} = {value:?} contains no Thai script characters"
            );
        }
    }

    #[test]
    fn tr_resolves_known_keys_in_both_locales() {
        let cases = [
            ("lock.touch_to_unlock", "Touch to unlock", "แตะเพื่อปลดล็อก"),
            ("home.good_evening", "Good evening", "สวัสดีตอนเย็น"),
            (
                "search.placeholder",
                "Search apps, files & settings",
                "ค้นหาแอป ไฟล์ และการตั้งค่า",
            ),
            ("recents.clear_all", "Clear all", "ล้างทั้งหมด"),
            ("shade.brightness", "Brightness", "ความสว่าง"),
            ("quick_settings.wifi", "Wi-Fi", "ไวไฟ"),
            ("power_menu.power_off", "Power off", "ปิดเครื่อง"),
            ("settings.title", "Settings", "การตั้งค่า"),
            ("first_run.continue_btn", "Continue", "ดำเนินการต่อ"),
            ("error.close_app", "Close app", "ปิดแอป"),
            (
                "screenshot.saved_to",
                "Saved to Pictures/Screenshots",
                "บันทึกไว้ที่ Pictures/Screenshots",
            ),
            ("status.bar_hint", "status bar · 48px", "แถบสถานะ · 48px"),
        ];

        for (key, expected_en, expected_th) in cases {
            assert_eq!(tr(Locale::En, key), expected_en, "en mismatch for {key}");
            assert_eq!(tr(Locale::Th, key), expected_th, "th mismatch for {key}");
        }
    }

    #[test]
    fn locale_code_roundtrip() {
        assert_eq!(Locale::from_code("en"), Some(Locale::En));
        assert_eq!(Locale::from_code("EN-US"), Some(Locale::En));
        assert_eq!(Locale::from_code("th"), Some(Locale::Th));
        assert_eq!(Locale::from_code("th_TH.UTF-8"), Some(Locale::Th));
        assert_eq!(Locale::from_code("fr"), None);
        assert_eq!(Locale::En.code(), "en");
        assert_eq!(Locale::Th.code(), "th");
        assert_eq!(Locale::Th.native_name(), "ไทย");
    }

    #[test]
    fn parser_rejects_malformed_toml() {
        assert!(Strings::parse("[unclosed").is_err());
        assert!(Strings::parse("no_equals_here").is_err());
        assert!(Strings::parse("k = \"unclosed").is_err());
        assert!(Strings::parse("[a]\nk = \"1\"\nk = \"2\"").is_err());
    }
}
