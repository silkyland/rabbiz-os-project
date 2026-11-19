//! i18n catalogs (M2-T7): `Strings` is a flat key→text map parsed from the
//! bundled `locales/<lang>.toml`. Screens take `&Strings` so EN/TH switch is
//! a data swap, and Thai shaping comes free from the bundled Noto Sans Thai.

use std::collections::HashMap;

const EN_TOML: &str = include_str!("../../locales/en.toml");
const TH_TOML: &str = include_str!("../../locales/th.toml");

/// Supported locales (contract: English + Thai at M2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Locale {
    En,
    Th,
}

impl Locale {
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Th => "th",
        }
    }

    /// Parse `en`/`th` (falls back to English for anything else).
    #[must_use]
    pub fn from_code(s: &str) -> Self {
        match s {
            "th" | "th-TH" | "th_TH" => Self::Th,
            _ => Self::En,
        }
    }
}

/// Flat `"section.key" -> text` catalog.
#[derive(Debug, Clone)]
pub struct Strings {
    map: HashMap<String, String>,
    pub locale: Locale,
}

impl Strings {
    #[must_use]
    pub fn load(locale: Locale) -> Self {
        let src = match locale {
            Locale::En => EN_TOML,
            Locale::Th => TH_TOML,
        };
        let mut map = HashMap::new();
        let doc: toml::Value = toml::from_str(src).expect("locale toml parses");
        if let toml::Value::Table(root) = doc {
            for (section, v) in root {
                if let toml::Value::Table(inner) = v {
                    for (key, val) in inner {
                        if let toml::Value::String(s) = val {
                            map.insert(format!("{section}.{key}"), s);
                        }
                    }
                }
            }
        }
        Self { map, locale }
    }

    /// Lookup with English fallback, then the raw key as last resort.
    #[must_use]
    pub fn get<'a>(&'a self, key: &'a str) -> &'a str {
        self.map.get(key).map_or(key, String::as_str)
    }
}

impl Default for Strings {
    fn default() -> Self {
        Self::load(Locale::En)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn en_and_th_load() {
        let en = Strings::load(Locale::En);
        let th = Strings::load(Locale::Th);
        assert_eq!(en.get("power.restart"), "Restart");
        assert_eq!(th.get("power.restart"), "รีสตาร์ท");
        assert_eq!(th.get("common.done"), "เสร็จสิ้น");
    }

    #[test]
    fn catalogs_cover_same_keys() {
        let en = Strings::load(Locale::En);
        let th = Strings::load(Locale::Th);
        for k in en.map.keys() {
            assert!(th.map.contains_key(k), "th missing {k}");
        }
        for k in th.map.keys() {
            assert!(en.map.contains_key(k), "en missing {k}");
        }
    }

    #[test]
    fn unknown_key_returns_key() {
        let s = Strings::default();
        assert_eq!(s.get("nope.nada"), "nope.nada");
    }
}
