//! Pure URI-to-Intent router for cross-app handoff in RabbizOS.
//!
//! Host-testable with zero platform dependencies and zero `unsafe`.
//! Parses `tel:`, `sms:`, `file://`, and `rabbiz://` URIs into structured
//! [`Intent`] values and renders them as `(binary_or_ctl, argv)` commands.

use std::path::Path;

/// A structured cross-app navigation or action request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Intent {
    Dial {
        number: String,
    },
    Sms {
        number: String,
        body: Option<String>,
    },
    OpenFile {
        path: String,
        app: &'static str,
    },
    OpenSettings {
        section: Option<String>,
    },
    LaunchApp {
        desktop_id: String,
    },
}

impl Intent {
    /// Converts this intent into `(binary_or_ctl, argv)` suitable for process
    /// spawning or IPC handoff.
    #[must_use]
    pub fn to_command(&self) -> (&'static str, Vec<String>) {
        match self {
            Self::Dial { number } => ("rabbiz-dialer", vec!["--dial".into(), number.clone()]),
            Self::Sms { number, body } => {
                let mut argv = vec!["--to".into(), number.clone()];
                if let Some(text) = body {
                    argv.push("--body".into());
                    argv.push(text.clone());
                }
                ("rabbiz-sms", argv)
            }
            Self::OpenFile { path, app } => (app, vec![path.clone()]),
            Self::OpenSettings { section } => match section {
                Some(sec) => ("rabbiz-settings", vec!["--section".into(), sec.clone()]),
                None => ("rabbiz-settings", Vec::new()),
            },
            Self::LaunchApp { desktop_id } => {
                ("rabbiz-ctl", vec!["launch".into(), desktop_id.clone()])
            }
        }
    }
}

/// Resolves the RabbizOS viewer/editor binary name for a file path based on
/// its case-insensitive extension.
#[must_use]
pub fn resolve_mime_app(path: &str) -> &'static str {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    match ext.as_deref() {
        Some("pdf") => "rabbiz-pdf",
        Some("png" | "jpg" | "jpeg" | "webp" | "gif") => "rabbiz-photos",
        Some("mp3" | "flac" | "ogg" | "wav" | "m4a" | "opus") => "rabbiz-music",
        Some("txt" | "md") => "rabbiz-notes",
        _ => "rabbiz-files",
    }
}

/// Parses a URI into an [`Intent`], returning `None` for empty, unknown, or
/// malformed URIs.
#[must_use]
pub fn parse_uri(uri: &str) -> Option<Intent> {
    let trimmed = uri.trim();
    let (scheme, rest) = trimmed.split_once(':')?;
    if scheme.is_empty() {
        return None;
    }

    if scheme.eq_ignore_ascii_case("tel") {
        let number = strip_whitespace(rest);
        if number.is_empty() {
            return None;
        }
        return Some(Intent::Dial { number });
    }

    if scheme.eq_ignore_ascii_case("sms") {
        let (num_raw, query_opt) = match rest.split_once('?') {
            Some((n, q)) => (n, Some(q)),
            None => (rest, None),
        };
        let number = strip_whitespace(num_raw);
        if number.is_empty() {
            return None;
        }
        let body = query_opt.and_then(extract_sms_body);
        return Some(Intent::Sms { number, body });
    }

    if scheme.eq_ignore_ascii_case("file") {
        let raw_path = rest.strip_prefix("//")?;
        let path = percent_decode(raw_path, false);
        if path.is_empty() {
            return None;
        }
        let app = resolve_mime_app(&path);
        return Some(Intent::OpenFile { path, app });
    }

    if scheme.eq_ignore_ascii_case("rabbiz") {
        let target = rest.strip_prefix("//")?;
        if target == "settings" || target == "settings/" {
            return Some(Intent::OpenSettings { section: None });
        }
        if let Some(sec) = target.strip_prefix("settings/") {
            let sec = sec.trim_end_matches('/');
            if sec.is_empty() {
                return Some(Intent::OpenSettings { section: None });
            }
            return Some(Intent::OpenSettings {
                section: Some(sec.to_string()),
            });
        }
        if let Some(id) = target.strip_prefix("app/") {
            let id = id.trim();
            if id.is_empty() || id.contains('/') || id.chars().any(char::is_whitespace) {
                return None;
            }
            return Some(Intent::LaunchApp {
                desktop_id: id.to_string(),
            });
        }
        return None;
    }

    None
}

fn strip_whitespace(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

fn extract_sms_body(query: &str) -> Option<String> {
    for pair in query.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            if k.eq_ignore_ascii_case("body") {
                let decoded = percent_decode(v, true);
                if !decoded.is_empty() {
                    return Some(decoded);
                }
            }
        }
    }
    None
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Decodes `%XX` byte escapes (and `+` as space when `plus_as_space` is true).
/// Invalid percent sequences are preserved verbatim.
fn percent_decode(input: &str, plus_as_space: bool) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if plus_as_space && bytes[i] == b'+' {
            out.push(b' ');
            i += 1;
        } else if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                out.push((hi << 4) | lo);
                i += 3;
                continue;
            }
            out.push(b'%');
            i += 1;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tel_strips_whitespace() {
        let intent = parse_uri("tel: +66 81 234 5678 ").unwrap();
        assert_eq!(
            intent,
            Intent::Dial {
                number: "+66812345678".into()
            }
        );
        assert_eq!(
            intent.to_command(),
            (
                "rabbiz-dialer",
                vec!["--dial".into(), "+66812345678".into()]
            )
        );
    }

    #[test]
    fn parse_sms_with_and_without_body() {
        let plain = parse_uri("sms:081 234 5678").unwrap();
        assert_eq!(
            plain,
            Intent::Sms {
                number: "0812345678".into(),
                body: None,
            }
        );
        assert_eq!(
            plain.to_command(),
            ("rabbiz-sms", vec!["--to".into(), "0812345678".into()])
        );

        let with_body = parse_uri("sms:0812345678?foo=1&body=Hello%20World+from%21Rabbiz").unwrap();
        assert_eq!(
            with_body,
            Intent::Sms {
                number: "0812345678".into(),
                body: Some("Hello World from!Rabbiz".into()),
            }
        );
        assert_eq!(
            with_body.to_command(),
            (
                "rabbiz-sms",
                vec![
                    "--to".into(),
                    "0812345678".into(),
                    "--body".into(),
                    "Hello World from!Rabbiz".into(),
                ]
            )
        );
    }

    #[test]
    fn parse_file_and_mime_resolution_case_insensitive() {
        let cases = [
            ("file:///home/rabbiz/report.PDF", "rabbiz-pdf"),
            ("file:///tmp/photo.png", "rabbiz-photos"),
            ("file:///tmp/photo.JPG", "rabbiz-photos"),
            ("file:///tmp/photo.jpeg", "rabbiz-photos"),
            ("file:///tmp/photo.WeBp", "rabbiz-photos"),
            ("file:///tmp/anim.gif", "rabbiz-photos"),
            ("file:///music/track.mp3", "rabbiz-music"),
            ("file:///music/track.FLAC", "rabbiz-music"),
            ("file:///music/track.ogg", "rabbiz-music"),
            ("file:///music/track.wav", "rabbiz-music"),
            ("file:///music/track.m4a", "rabbiz-music"),
            ("file:///music/track.opus", "rabbiz-music"),
            ("file:///notes/todo.txt", "rabbiz-notes"),
            ("file:///notes/README.MD", "rabbiz-notes"),
            ("file:///archive/data.tar.gz", "rabbiz-files"),
            ("file:///home/rabbiz/Downloads", "rabbiz-files"),
        ];

        for (uri, expected_app) in cases {
            let intent = parse_uri(uri).unwrap_or_else(|| panic!("failed to parse {uri}"));
            match &intent {
                Intent::OpenFile { path, app } => {
                    assert_eq!(*app, expected_app, "wrong app for {uri}");
                    assert_eq!(intent.to_command(), (expected_app, vec![path.clone()]));
                }
                other => panic!("expected OpenFile for {uri}, got {other:?}"),
            }
        }

        // Percent-encoded space in file path
        let spaced = parse_uri("file:///home/rabbiz/my%20doc.pdf").unwrap();
        assert_eq!(
            spaced,
            Intent::OpenFile {
                path: "/home/rabbiz/my doc.pdf".into(),
                app: "rabbiz-pdf",
            }
        );
    }

    #[test]
    fn parse_rabbiz_settings_and_launch_app() {
        let root = parse_uri("rabbiz://settings").unwrap();
        assert_eq!(root, Intent::OpenSettings { section: None });
        assert_eq!(root.to_command(), ("rabbiz-settings", Vec::new()));

        let trailing = parse_uri("rabbiz://settings/").unwrap();
        assert_eq!(trailing, Intent::OpenSettings { section: None });

        let wifi = parse_uri("rabbiz://settings/wifi").unwrap();
        assert_eq!(
            wifi,
            Intent::OpenSettings {
                section: Some("wifi".into())
            }
        );
        assert_eq!(
            wifi.to_command(),
            ("rabbiz-settings", vec!["--section".into(), "wifi".into()])
        );

        let app = parse_uri("rabbiz://app/rabbiz-notes.desktop").unwrap();
        assert_eq!(
            app,
            Intent::LaunchApp {
                desktop_id: "rabbiz-notes.desktop".into()
            }
        );
        assert_eq!(
            app.to_command(),
            (
                "rabbiz-ctl",
                vec!["launch".into(), "rabbiz-notes.desktop".into()]
            )
        );
    }

    #[test]
    fn malformed_and_empty_uris_return_none() {
        let invalid = [
            "",
            "   ",
            "no-scheme",
            ":missing-scheme",
            "tel:",
            "tel:   ",
            "sms:",
            "sms:?body=hello",
            "file:/missing-double-slash",
            "file://",
            "rabbiz://",
            "rabbiz://unknown",
            "rabbiz://app/",
            "rabbiz://app/bad/id",
            "https://example.com",
        ];
        for uri in invalid {
            assert_eq!(parse_uri(uri), None, "expected None for {uri:?}");
        }
    }
}
