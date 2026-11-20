//! Wave 6 universal search and command router (pure `std`, L0 host-testable).
//!
//! Combines a safe recursive-descent inline calculator, settings shortcut
//! matching, contact lookup (by name or phone digits), and tiered app ranking
//! into a single deterministic result list for `screens::search`.

/// Maximum byte length accepted by [`eval_math_expr`].
const MAX_EXPR_BYTES: usize = 128;

/// Maximum parenthesis / unary nesting depth accepted by [`eval_math_expr`].
const MAX_EXPR_DEPTH: usize = 32;

/// An installed application entry searchable by the universal router.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchAppItem {
    pub desktop_id: String,
    pub name: String,
}

/// A contact entry searchable by name or phone number digits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchContactItem {
    pub name: String,
    pub phone: String,
}

/// A ranked universal search result item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchResult {
    /// Inline arithmetic calculation result.
    Calculation {
        expression: String,
        formatted: String,
    },
    /// Direct jump into a RabbizOS Settings section.
    SettingJump {
        title: String,
        section: &'static str,
    },
    /// Matching contact for quick call / SMS actions.
    Contact { name: String, phone: String },
    /// Matching application launcher entry.
    App { desktop_id: String, name: String },
}

/// Safely evaluate an inline arithmetic expression.
///
/// Supports `+`, `-`, `*`, `/`, parentheses `( )`, unary `-`/`+`, decimal
/// numbers, and trailing `%` (both standalone like `50%` → `0.5` and
/// additive/subtractive like `100 + 7%` → `107`).
///
/// Returns `None` if:
/// - the input is empty, longer than 128 bytes, or has trailing/invalid tokens;
/// - no operator is present (so plain numbers like `"12345"` or phone numbers
///   like `"0812345678"` do not trigger the calculator);
/// - parentheses are unbalanced or nesting depth exceeds 32;
/// - division by zero occurs or any intermediate/final result is non-finite.
#[must_use]
pub fn eval_math_expr(input: &str) -> Option<f64> {
    if input.len() > MAX_EXPR_BYTES {
        return None;
    }
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut parser = MathParser::new(trimmed.as_bytes());
    let val = parser.parse_expr()?;
    parser.skip_ws();
    if !parser.is_eof() || !parser.has_operator || !val.is_finite() {
        return None;
    }
    // Avoid returning negative zero.
    Some(if val == 0.0 { 0.0 } else { val })
}

/// Format a finite calculation result cleanly: integers have no `.0` suffix,
/// and fractional values have trailing zeros stripped.
#[must_use]
pub fn format_calc_result(v: f64) -> String {
    if !v.is_finite() {
        return String::new();
    }
    let v = if v == 0.0 { 0.0 } else { v };
    let abs = v.abs();

    if abs > 0.0 && (abs >= 1e15 || abs < 1e-9) {
        return format!("{v}");
    }

    // Round to 10 fractional digits to suppress IEEE-754 representation dust
    // such as `0.1 + 0.2 = 0.30000000000000004`.
    let mut s = format!("{v:.10}");
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    if s == "-0" {
        return "0".to_string();
    }
    s
}

/// Parsed term value together with whether it represents a bare percentage
/// literal/factor (enabling `100 + 7% == 107` and `100 - 10% == 90`).
#[derive(Debug, Clone, Copy)]
struct TermVal {
    value: f64,
    is_percent: bool,
}

struct MathParser<'a> {
    bytes: &'a [u8],
    pos: usize,
    depth: usize,
    has_operator: bool,
}

impl<'a> MathParser<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            pos: 0,
            depth: 0,
            has_operator: false,
        }
    }

    fn is_eof(&self) -> bool {
        self.pos >= self.bytes.len()
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn skip_ws(&mut self) {
        while let Some(b) = self.peek() {
            if b.is_ascii_whitespace() {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn parse_expr(&mut self) -> Option<f64> {
        let mut acc = self.parse_term()?.value;
        loop {
            self.skip_ws();
            let op = match self.peek() {
                Some(b'+') => b'+',
                Some(b'-') => b'-',
                _ => break,
            };
            self.pos += 1;
            self.has_operator = true;
            let rhs = self.parse_term()?;
            let delta = if rhs.is_percent {
                acc * rhs.value
            } else {
                rhs.value
            };
            acc = if op == b'+' { acc + delta } else { acc - delta };
            if !acc.is_finite() {
                return None;
            }
        }
        Some(acc)
    }

    fn parse_term(&mut self) -> Option<TermVal> {
        let mut lhs = self.parse_unary()?;
        loop {
            self.skip_ws();
            let op = match self.peek() {
                Some(b'*') => b'*',
                Some(b'/') => b'/',
                _ => break,
            };
            self.pos += 1;
            self.has_operator = true;
            let rhs = self.parse_unary()?;
            let next = if op == b'*' {
                lhs.value * rhs.value
            } else {
                if rhs.value == 0.0 {
                    return None;
                }
                lhs.value / rhs.value
            };
            if !next.is_finite() {
                return None;
            }
            lhs = TermVal {
                value: next,
                is_percent: false,
            };
        }
        Some(lhs)
    }

    fn parse_unary(&mut self) -> Option<TermVal> {
        self.skip_ws();
        match self.peek() {
            Some(b'+') => {
                self.pos += 1;
                self.depth += 1;
                if self.depth > MAX_EXPR_DEPTH {
                    return None;
                }
                let inner = self.parse_unary()?;
                self.depth -= 1;
                Some(inner)
            }
            Some(b'-') => {
                self.pos += 1;
                self.has_operator = true;
                self.depth += 1;
                if self.depth > MAX_EXPR_DEPTH {
                    return None;
                }
                let inner = self.parse_unary()?;
                self.depth -= 1;
                Some(TermVal {
                    value: -inner.value,
                    is_percent: inner.is_percent,
                })
            }
            _ => self.parse_postfix(),
        }
    }

    fn parse_postfix(&mut self) -> Option<TermVal> {
        let mut val = self.parse_primary()?;
        let mut is_percent = false;
        loop {
            self.skip_ws();
            if self.peek() == Some(b'%') {
                self.pos += 1;
                self.has_operator = true;
                val /= 100.0;
                if !val.is_finite() {
                    return None;
                }
                is_percent = true;
            } else {
                break;
            }
        }
        Some(TermVal {
            value: val,
            is_percent,
        })
    }

    fn parse_primary(&mut self) -> Option<f64> {
        self.skip_ws();
        match self.peek()? {
            b'(' => {
                self.pos += 1;
                self.depth += 1;
                if self.depth > MAX_EXPR_DEPTH {
                    return None;
                }
                let val = self.parse_expr()?;
                self.skip_ws();
                if self.peek() != Some(b')') {
                    return None;
                }
                self.pos += 1;
                self.depth -= 1;
                Some(val)
            }
            b'0'..=b'9' | b'.' => self.parse_number(),
            _ => None,
        }
    }

    fn parse_number(&mut self) -> Option<f64> {
        let start = self.pos;
        let mut saw_digit = false;
        let mut saw_dot = false;
        while let Some(b) = self.peek() {
            match b {
                b'0'..=b'9' => {
                    saw_digit = true;
                    self.pos += 1;
                }
                b'.' if !saw_dot => {
                    saw_dot = true;
                    self.pos += 1;
                }
                b'.' => return None,
                _ => break,
            }
        }
        if !saw_digit {
            return None;
        }
        let s = std::str::from_utf8(&self.bytes[start..self.pos]).ok()?;
        let n = s.parse::<f64>().ok()?;
        n.is_finite().then_some(n)
    }
}

struct SettingShortcut {
    title: &'static str,
    section: &'static str,
    keywords: &'static [&'static str],
}

const SETTING_SHORTCUTS: &[SettingShortcut] = &[
    SettingShortcut {
        title: "Wi-Fi",
        section: "wifi",
        keywords: &["wifi", "wi-fi", "network", "internet"],
    },
    SettingShortcut {
        title: "Bluetooth",
        section: "bluetooth",
        keywords: &["bluetooth"],
    },
    SettingShortcut {
        title: "Display & Theme",
        section: "display",
        keywords: &["display", "dark", "light", "theme", "brightness"],
    },
    SettingShortcut {
        title: "Sound & Volume",
        section: "sound",
        keywords: &["sound", "volume", "audio"],
    },
    SettingShortcut {
        title: "Battery & Power",
        section: "battery",
        keywords: &["battery", "power"],
    },
    SettingShortcut {
        title: "Lock & Security",
        section: "security",
        keywords: &["lock", "security"],
    },
    SettingShortcut {
        title: "Keyboard & Input",
        section: "keyboard",
        keywords: &["keyboard", "input"],
    },
    SettingShortcut {
        title: "About Device",
        section: "about",
        keywords: &["about", "device", "version"],
    },
];

/// Match built-in Settings shortcuts case-insensitively against `query`.
#[must_use]
pub fn match_settings(query: &str) -> Vec<SearchResult> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let q = trimmed.to_lowercase();
    let words: Vec<&str> = q.split_whitespace().collect();

    SETTING_SHORTCUTS
        .iter()
        .filter(|entry| {
            entry.title.to_lowercase().contains(&q)
                || entry
                    .keywords
                    .iter()
                    .any(|kw| kw.contains(&q) || (kw.len() >= 3 && words.iter().any(|w| *w == *kw)))
        })
        .map(|entry| SearchResult::SettingJump {
            title: entry.title.to_string(),
            section: entry.section,
        })
        .collect()
}

/// Rank universal search results across calculator, apps, settings, and contacts.
///
/// Empty or whitespace-only queries return an empty list. Non-empty queries are
/// ordered by priority:
/// 1. Inline `Calculation` (if `query` is a valid math expression)
/// 2. Exact and prefix `App` matches
/// 3. `SettingJump` shortcuts
/// 4. `Contact` matches (by name or phone digits)
/// 5. Substring `App` matches
///
/// The returned list is truncated to `limit`.
#[must_use]
pub fn rank_results(
    query: &str,
    apps: &[SearchAppItem],
    contacts: &[SearchContactItem],
    limit: usize,
) -> Vec<SearchResult> {
    let trimmed = query.trim();
    if trimmed.is_empty() || limit == 0 {
        return Vec::new();
    }

    let q = trimmed.to_lowercase();
    let mut results = Vec::new();

    // 1. Inline calculation.
    if let Some(val) = eval_math_expr(trimmed) {
        results.push(SearchResult::Calculation {
            expression: trimmed.to_string(),
            formatted: format_calc_result(val),
        });
    }

    // Partition apps into exact, prefix, and substring matches without duplicates.
    let mut exact_apps = Vec::new();
    let mut prefix_apps = Vec::new();
    let mut substring_apps = Vec::new();
    for app in apps {
        let name_lower = app.name.to_lowercase();
        let id_lower = app.desktop_id.to_lowercase();
        if name_lower == q || id_lower == q {
            exact_apps.push(app);
        } else if name_lower.starts_with(&q) || id_lower.starts_with(&q) {
            prefix_apps.push(app);
        } else if name_lower.contains(&q) || id_lower.contains(&q) {
            substring_apps.push(app);
        }
    }

    // 2. Exact / prefix app matches.
    for app in exact_apps.into_iter().chain(prefix_apps) {
        results.push(SearchResult::App {
            desktop_id: app.desktop_id.clone(),
            name: app.name.clone(),
        });
    }

    // 3. Settings shortcuts.
    results.extend(match_settings(trimmed));

    // 4. Contact matches (name substring or phone digit substring).
    let phone_query_digits = extract_phone_query_digits(trimmed);
    for contact in contacts {
        let name_match = contact.name.to_lowercase().contains(&q);
        let phone_match = phone_query_digits.as_ref().is_some_and(|q_digits| {
            let phone_digits: String = contact.phone.chars().filter(char::is_ascii_digit).collect();
            phone_digits.contains(q_digits.as_str())
        });
        if name_match || phone_match {
            results.push(SearchResult::Contact {
                name: contact.name.clone(),
                phone: contact.phone.clone(),
            });
        }
    }

    // 5. Substring app matches.
    for app in substring_apps {
        results.push(SearchResult::App {
            desktop_id: app.desktop_id.clone(),
            name: app.name.clone(),
        });
    }

    results.truncate(limit);
    results
}

/// Extract digits from `query` when it looks like a phone number fragment
/// (digits and phone punctuation only, containing at least one digit).
fn extract_phone_query_digits(query: &str) -> Option<String> {
    if !query
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, '+' | '-' | ' ' | '(' | ')' | '.'))
    {
        return None;
    }
    let digits: String = query.chars().filter(char::is_ascii_digit).collect();
    (!digits.is_empty()).then_some(digits)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(actual: Option<f64>, expected: f64) {
        let v = actual.expect("expected Some(f64)");
        assert!((v - expected).abs() < 1e-9, "expected {expected}, got {v}");
    }

    #[test]
    fn math_precedence_and_parentheses() {
        approx_eq(eval_math_expr("2 + 3 * 4"), 14.0);
        approx_eq(eval_math_expr("(2 + 3) * 4"), 20.0);
        approx_eq(eval_math_expr("10 - 6 / 2 + 1"), 8.0);
        approx_eq(eval_math_expr("((1 + 2) * (3 + 4)) / 7"), 3.0);
    }

    #[test]
    fn math_unary_decimals_and_percentages() {
        approx_eq(eval_math_expr("-5 + 12"), 7.0);
        approx_eq(eval_math_expr("-(2 + 3) * 4"), -20.0);
        approx_eq(eval_math_expr("-5"), -5.0);
        approx_eq(eval_math_expr("1250 * 1.07"), 1337.5);
        approx_eq(eval_math_expr("0.1 + 0.2"), 0.3);
        approx_eq(eval_math_expr("50%"), 0.5);
        approx_eq(eval_math_expr("100 + 7%"), 107.0);
        approx_eq(eval_math_expr("200 - 15%"), 170.0);
        approx_eq(eval_math_expr("200 * 10%"), 20.0);
    }

    #[test]
    fn math_rejects_plain_numbers_and_invalid_inputs() {
        assert_eq!(eval_math_expr(""), None);
        assert_eq!(eval_math_expr("   "), None);
        assert_eq!(eval_math_expr("12345"), None);
        assert_eq!(eval_math_expr("0812345678"), None);
        assert_eq!(eval_math_expr("+66812345678"), None);
        assert_eq!(eval_math_expr("(42)"), None);
        assert_eq!(eval_math_expr("10 / 0"), None);
        assert_eq!(eval_math_expr("10 / (5 - 5)"), None);
        assert_eq!(eval_math_expr("(2 + 3"), None);
        assert_eq!(eval_math_expr("2 + 3)"), None);
        assert_eq!(eval_math_expr("2 + foo"), None);
        assert_eq!(eval_math_expr("1.2.3 + 1"), None);

        // Over 128 bytes rejected.
        let long_expr = format!("1{}", " + 1".repeat(40));
        assert!(long_expr.len() > 128);
        assert_eq!(eval_math_expr(&long_expr), None);

        // Nesting depth > 32 rejected.
        let deep = format!("{}1 + 1{}", "(".repeat(33), ")".repeat(33));
        assert_eq!(eval_math_expr(&deep), None);
    }

    #[test]
    fn format_calc_result_clean() {
        assert_eq!(format_calc_result(14.0), "14");
        assert_eq!(format_calc_result(-0.0), "0");
        assert_eq!(format_calc_result(0.1 + 0.2), "0.3");
        assert_eq!(format_calc_result(1337.50), "1337.5");
        assert_eq!(format_calc_result(107.00000000000001), "107");
    }

    #[test]
    fn settings_shortcuts_match_all_keywords() {
        let expected = [
            ("wifi", "wifi"),
            ("WIFI", "wifi"),
            ("bluetooth", "bluetooth"),
            ("dark", "display"),
            ("light", "display"),
            ("theme", "display"),
            ("display", "display"),
            ("sound", "sound"),
            ("volume", "sound"),
            ("battery", "battery"),
            ("power", "battery"),
            ("lock", "security"),
            ("security", "security"),
            ("keyboard", "keyboard"),
            ("about", "about"),
        ];
        for (query, section) in expected {
            let hits = match_settings(query);
            assert!(
                hits.iter().any(
                    |r| matches!(r, SearchResult::SettingJump { section: s, .. } if *s == section)
                ),
                "query {query:?} did not match section {section:?}: {hits:?}"
            );
        }
        assert!(match_settings("   ").is_empty());
        assert!(match_settings("zzzzzz").is_empty());
    }

    #[test]
    fn contacts_match_by_name_and_digits() {
        let contacts = vec![
            SearchContactItem {
                name: "Sarah Chen".into(),
                phone: "081-234-5678".into(),
            },
            SearchContactItem {
                name: "Somchai".into(),
                phone: "+66 89 000 1111".into(),
            },
        ];

        let by_name = rank_results("sarah", &[], &contacts, 10);
        assert_eq!(
            by_name,
            vec![SearchResult::Contact {
                name: "Sarah Chen".into(),
                phone: "081-234-5678".into(),
            }]
        );

        let by_digits = rank_results("081234", &[], &contacts, 10);
        assert_eq!(
            by_digits,
            vec![SearchResult::Contact {
                name: "Sarah Chen".into(),
                phone: "081-234-5678".into(),
            }]
        );

        let by_formatted_digits = rank_results("89 000", &[], &contacts, 10);
        assert_eq!(
            by_formatted_digits,
            vec![SearchResult::Contact {
                name: "Somchai".into(),
                phone: "+66 89 000 1111".into(),
            }]
        );
    }

    #[test]
    fn rank_results_priority_order_and_limit() {
        let apps = vec![
            SearchAppItem {
                desktop_id: "org.example.mywifi".into(),
                name: "My WiFi Monitor".into(),
            },
            SearchAppItem {
                desktop_id: "rabbiz-wifi".into(),
                name: "WiFi Analyzer".into(),
            },
            SearchAppItem {
                desktop_id: "wifi".into(),
                name: "WiFi".into(),
            },
        ];
        let contacts = vec![SearchContactItem {
            name: "WiFi Support".into(),
            phone: "1234".into(),
        }];

        // Empty / whitespace returns empty.
        assert!(rank_results("", &apps, &contacts, 10).is_empty());
        assert!(rank_results("   ", &apps, &contacts, 10).is_empty());
        assert!(rank_results("wifi", &apps, &contacts, 0).is_empty());

        // Priority: exact/prefix Apps -> SettingJump -> Contact -> substring Apps.
        let ranked = rank_results("wifi", &apps, &contacts, 10);
        assert_eq!(
            ranked,
            vec![
                SearchResult::App {
                    desktop_id: "wifi".into(),
                    name: "WiFi".into(),
                },
                SearchResult::App {
                    desktop_id: "rabbiz-wifi".into(),
                    name: "WiFi Analyzer".into(),
                },
                SearchResult::SettingJump {
                    title: "Wi-Fi".into(),
                    section: "wifi",
                },
                SearchResult::Contact {
                    name: "WiFi Support".into(),
                    phone: "1234".into(),
                },
                SearchResult::App {
                    desktop_id: "org.example.mywifi".into(),
                    name: "My WiFi Monitor".into(),
                },
            ]
        );

        // Limit cap respected.
        let capped = rank_results("wifi", &apps, &contacts, 2);
        assert_eq!(capped.len(), 2);
        assert_eq!(capped[0], ranked[0]);
        assert_eq!(capped[1], ranked[1]);

        // Calculation ranks above everything else.
        let calc_apps = vec![SearchAppItem {
            desktop_id: "rabbiz-calc".into(),
            name: "2 + 3".into(),
        }];
        let calc_ranked = rank_results("2 + 3", &calc_apps, &[], 10);
        assert_eq!(
            calc_ranked[0],
            SearchResult::Calculation {
                expression: "2 + 3".into(),
                formatted: "5".into(),
            }
        );
        assert_eq!(
            calc_ranked[1],
            SearchResult::App {
                desktop_id: "rabbiz-calc".into(),
                name: "2 + 3".into(),
            }
        );
    }
}
