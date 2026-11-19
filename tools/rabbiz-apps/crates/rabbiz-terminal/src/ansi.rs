//! ANSI/VT escape stripping and a minimal line assembler for command output.
//!
//! Scope is deliberately narrow (docs/BASE-APPS.md: "ANSI escapes stripped"):
//! CSI / OSC / DCS-family sequences are dropped, `\r` rewinds the cursor
//! column (so `foo\rbar` renders `bar`), `\b`/DEL step back, `\t` expands to
//! 8-column stops. No color, no cursor addressing — a real VT parser is the
//! "full PTY session" later milestone.

/// Remove ANSI/VT escape sequences from `input`, returning plain text.
///
/// Handles CSI (`ESC [ … final`), OSC (`ESC ] … BEL|ST`), DCS/SOS/PM/APC
/// (`ESC P|X|^|_ … ST`), two-byte escapes, charset selects (`ESC ( B`), and
/// the C1 forms `0x9B`/`0x9D`. All other bytes — including UTF-8 — pass
/// through untouched.
#[must_use]
pub fn strip_ansi(input: &str) -> String {
    let b = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == 0x1B {
            i = skip_esc(b, i);
        } else if c == 0xC2 && i + 1 < b.len() && b[i + 1] == 0x9B {
            // C1 CSI (U+009B encodes as C2 9B — match the pair, never the
            // bare continuation byte, which is valid inside UTF-8).
            let mut j = i + 2;
            while j < b.len() && !(0x40..=0x7E).contains(&b[j]) {
                j += 1;
            }
            i = (j + 1).min(b.len());
        } else if c == 0xC2 && i + 1 < b.len() && b[i + 1] == 0x9D {
            // C1 OSC (U+009D): until BEL or ST.
            let mut j = i + 2;
            while j < b.len() {
                if b[j] == 0x07 {
                    j += 1;
                    break;
                }
                if b[j] == 0x1B && j + 1 < b.len() && b[j + 1] == b'\\' {
                    j += 2;
                    break;
                }
                j += 1;
            }
            i = j;
        } else {
            out.push(c);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Given `b[i] == ESC`, return the index just past the whole sequence.
fn skip_esc(b: &[u8], i: usize) -> usize {
    let n = b.len();
    if i + 1 >= n {
        return n; // lone trailing ESC: drop it
    }
    match b[i + 1] {
        b'[' => {
            // CSI: params (0x30-0x3F) + intermediates (0x20-0x2F) + final (0x40-0x7E).
            let mut j = i + 2;
            while j < n && !(0x40..=0x7E).contains(&b[j]) {
                j += 1;
            }
            (j + 1).min(n)
        }
        b']' => skip_osc(b, i + 2),
        b'P' | b'X' | b'^' | b'_' => skip_dcs(b, i + 2),
        b'(' | b')' | b'*' | b'+' | b'-' | b'.' | b'/' => {
            // Charset/designate: ESC + introducer + one final byte.
            (i + 3).min(n)
        }
        _ => {
            // Fs/Fe two-byte escape (ESC c, ESC 7, ESC =, ESC >, …).
            i + 2
        }
    }
}

/// OSC payload ends at BEL or ST (`ESC \`).
fn skip_osc(b: &[u8], mut j: usize) -> usize {
    while j < b.len() {
        if b[j] == 0x07 {
            return j + 1;
        }
        if b[j] == 0x1B && j + 1 < b.len() && b[j + 1] == b'\\' {
            return j + 2;
        }
        j += 1;
    }
    j
}

/// DCS/SOS/PM/APC payload ends at ST, CAN or SUB.
fn skip_dcs(b: &[u8], mut j: usize) -> usize {
    while j < b.len() {
        if b[j] == 0x18 || b[j] == 0x1A {
            return j + 1;
        }
        if b[j] == 0x1B && j + 1 < b.len() && b[j + 1] == b'\\' {
            return j + 2;
        }
        j += 1;
    }
    j
}

/// Assembles raw command output into logical lines.
///
/// A tiny cursor-column model: `\n` finishes a line, `\r` returns to column 0
/// (subsequent characters overwrite in place, like a real terminal), `\b` and
/// DEL step back one column, `\t` pads to the next multiple-of-8 column.
/// Other C0 controls are dropped.
pub struct OutputParser {
    line: Vec<char>,
    col: usize,
}

impl OutputParser {
    #[must_use]
    pub fn new() -> Self {
        Self {
            line: Vec::new(),
            col: 0,
        }
    }

    /// Feed raw output; completed lines are appended to `sink`.
    pub fn feed(&mut self, input: &str, sink: &mut Vec<String>) {
        let clean = strip_ansi(input);
        for ch in clean.chars() {
            match ch {
                '\n' => self.end_line(sink),
                '\r' => self.col = 0,
                '\u{8}' | '\u{7F}' => self.col = self.col.saturating_sub(1),
                '\t' => {
                    let n = 8 - (self.col % 8);
                    for _ in 0..n {
                        self.put(' ');
                    }
                }
                c if c.is_control() => {}
                c => self.put(c),
            }
        }
    }

    /// Flush any partial line at end of output.
    pub fn finish(&mut self, sink: &mut Vec<String>) {
        if !self.line.is_empty() || self.col > 0 {
            self.end_line(sink);
        }
    }

    fn put(&mut self, c: char) {
        if self.col < self.line.len() {
            self.line[self.col] = c;
        } else {
            while self.line.len() < self.col {
                self.line.push(' ');
            }
            self.line.push(c);
        }
        self.col += 1;
    }

    fn end_line(&mut self, sink: &mut Vec<String>) {
        sink.push(self.line.iter().collect());
        self.line.clear();
        self.col = 0;
    }
}

impl Default for OutputParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed_all(input: &str) -> Vec<String> {
        let mut p = OutputParser::new();
        let mut out = Vec::new();
        p.feed(input, &mut out);
        p.finish(&mut out);
        out
    }

    #[test]
    fn strips_csi_colors_and_cursor_moves() {
        assert_eq!(strip_ansi("\x1b[31mred\x1b[0m \x1b[2K\x1b[1Gok"), "red ok");
        assert_eq!(strip_ansi("\x1b[?25lhidden\x1b[?25h"), "hidden");
    }

    #[test]
    fn strips_osc_and_dcs() {
        assert_eq!(strip_ansi("a\x1b]0;title\x07b"), "ab");
        assert_eq!(
            strip_ansi("a\x1b]8;;http://x\x1b\\link\x1b]8;;\x1b\\b"),
            "alinkb"
        );
        assert_eq!(strip_ansi("x\x1bP1;2|payload\x1b\\y"), "xy");
        assert_eq!(strip_ansi("x\x1b(cy"), "xy");
        assert_eq!(strip_ansi("x\x1bM\x1b7y"), "xy");
    }

    #[test]
    fn strips_c1_forms() {
        assert_eq!(strip_ansi("a\u{9B}31mb"), "ab");
        assert_eq!(strip_ansi("a\u{9D}0;t\u{7}b"), "ab");
    }

    #[test]
    fn utf8_continuation_0x9b_is_not_c1() {
        // 'ɛ' (U+025B) encodes as C9 9B — the 9B byte must not trigger C1 CSI.
        assert_eq!(strip_ansi("ɛx"), "ɛx");
    }

    #[test]
    fn keeps_utf8_and_newlines() {
        assert_eq!(strip_ansi("héllo\nwörld"), "héllo\nwörld");
    }

    #[test]
    fn plain_lines_split() {
        assert_eq!(feed_all("one\ntwo\nthree"), vec!["one", "two", "three"]);
    }

    #[test]
    fn cr_overwrites_like_a_terminal() {
        // Progress-style output collapses to the final frame.
        assert_eq!(feed_all("10%\r20%\r100%\n"), vec!["100%"]);
        // Short overwrite keeps the tail, like a real screen.
        assert_eq!(feed_all("abcdef\rXY\n"), vec!["XYcdef"]);
        // CRLF is one break, not two.
        assert_eq!(feed_all("a\r\nb\r\n"), vec!["a", "b"]);
    }

    #[test]
    fn backspace_and_tabs() {
        assert_eq!(feed_all("ab\x08c\n"), vec!["ac"]);
        assert_eq!(feed_all("a\tb\n"), vec!["a       b"]);
    }

    #[test]
    fn ansi_mid_stream_does_not_eat_text() {
        assert_eq!(
            feed_all("\x1b[1mbold\x1b[0m plain\x1b[K\n"),
            vec!["bold plain"]
        );
    }
}
