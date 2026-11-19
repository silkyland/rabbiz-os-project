//! RabbizOS Calculator — Material 3 keypad app.
//!
//! Design notes (vs the previous implementation):
//! - Flat M3-Expressive-style readout: small live expression above a large
//!   right-aligned result, no display "container" (Google Calculator 9.x did
//!   the same). Errors are inline in the result slot, tinted `danger`.
//! - Classic 4-column keypad: digits on `surface1`, function keys on
//!   `surface2`, operators tinted `accent` on `surface3`, `=` filled accent.
//!   The pending operator gets the filled-accent "live" treatment.
//! - History is a slide-down sheet (BEZIER_ENTER/EXIT, ≤240 ms) listing the
//!   last 20 calculations; tap an entry to reuse its result, Clear empties.
//! - Press feedback is a state-layer fade (~90 ms), all layout is derived
//!   from the live canvas size — no hardcoded 720x1600 anywhere.

use std::collections::VecDeque;
use std::time::Duration;

use de_api::Pixels;
use de_design::motion::{bezier_eval, BEZIER_ENTER, BEZIER_EXIT};
use rabbiz_app_kit::{
    text, ui, AppEvent, AppFrame, AppState, Canvas, Point, Rect, Size, TextInputCtl, Theme,
};

mod cli;
mod mcp;

// ---------------------------------------------------------------------------
// Pure logic: expression model, evaluation, formatting, input rules.
// ---------------------------------------------------------------------------

const MAX_EXPR_CHARS: usize = 48;
const HISTORY_CAP: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Add,
    Sub,
    Mul,
    Div,
}

impl Op {
    fn ch(self) -> char {
        match self {
            Self::Add => '+',
            Self::Sub => '−', // U+2212 minus sign, matches keypad glyph
            Self::Mul => '×',
            Self::Div => '÷',
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyKind {
    Digit(char),
    Dot,
    Percent,
    Negate,
    Clear,
    Backspace,
    Op(Op),
    Equals,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EvalErr {
    DivZero,
    Invalid,
    Overflow,
}

impl EvalErr {
    fn message(self) -> &'static str {
        match self {
            Self::DivZero => "Can't divide by 0",
            Self::Invalid => "Invalid expression",
            Self::Overflow => "Number too large",
        }
    }
}

#[derive(Debug, Clone)]
struct HistEntry {
    expr: String,
    result: String,
}

/// Char classes that can end an operand (i.e. what may precede a binary op).
fn is_operand_end(c: char) -> bool {
    c.is_ascii_digit() || c == '.' || c == '%'
}

fn is_binary_op_char(c: char) -> bool {
    matches!(c, '+' | '−' | '×' | '÷')
}

/// Is the trailing '−' a *binary* minus (preceded by an operand) rather than a
/// pending unary minus (preceded by an operator or nothing)?
fn trailing_minus_is_binary(expr: &str) -> bool {
    let mut it = expr.chars().rev();
    if it.next() != Some('−') {
        return false;
    }
    match it.next() {
        Some(p) => is_operand_end(p),
        None => false,
    }
}

/// Does `expr` end in a pending operator chain (trailing binary ops and/or a
/// pending unary minus, e.g. "2+", "2×−")?
fn has_pending_tail(expr: &str) -> bool {
    strip_pending_ops(expr).len() < expr.len()
}

/// Strip trailing pending operators so "2+−" and "2−" preview as "2".
/// A trailing '−' is always pending — binary ("2−") or unary ("2+−").
fn strip_pending_ops(expr: &str) -> &str {
    let mut s = expr;
    while let Some(last) = s.chars().last() {
        if !matches!(last, '+' | '−' | '×' | '÷') {
            break;
        }
        s = &s[..s.len() - last.len_utf8()];
    }
    s
}

/// Insert/remove a leading unary minus on the *last number token* of `expr`.
/// Returns true when the expression changed.
fn toggle_sign(expr: &mut String) -> bool {
    let mut chars: Vec<char> = expr.chars().collect();
    let n = chars.len();
    if n == 0 {
        return false;
    }
    // Find the start of the trailing numeric token (digits + '.', plus an
    // optional exponent part so "1e-20" toggles as one number).
    let mut i = n;
    while i > 0 && (chars[i - 1].is_ascii_digit() || chars[i - 1] == '.') {
        i -= 1;
    }
    if i == n {
        return false; // ends with an operator / '%'
    }
    // Exponent without sign: "1e20" — token scan stopped at 'e'.
    if i > 0 && (chars[i - 1] == 'e' || chars[i - 1] == 'E') {
        i -= 1;
        while i > 0 && (chars[i - 1].is_ascii_digit() || chars[i - 1] == '.') {
            i -= 1;
        }
    }
    // Exponent with sign: "1e-20" — token scan stopped at '-'.
    if i >= 2
        && (chars[i - 1] == '-' || chars[i - 1] == '+')
        && (chars[i - 2] == 'e' || chars[i - 2] == 'E')
    {
        i -= 2;
        while i > 0 && (chars[i - 1].is_ascii_digit() || chars[i - 1] == '.') {
            i -= 1;
        }
    }
    // A '−' directly before the token is unary iff what precedes *it* is an
    // operator or nothing (operand position).
    if i > 0 && (chars[i - 1] == '−' || chars[i - 1] == '-') {
        let unary = i == 1 || matches!(chars[i - 2], '+' | '−' | '×' | '÷' | '%');
        if unary {
            chars.remove(i - 1);
            *expr = chars.into_iter().collect();
            return true;
        }
    }
    chars.insert(i, '−');
    *expr = chars.into_iter().collect();
    true
}

/// Push one display-form character onto `expr` obeying keypad input rules.
/// Returns true when the expression changed.
fn push_input(expr: &mut String, c: char) -> bool {
    if expr.chars().count() >= MAX_EXPR_CHARS {
        return false;
    }
    match c {
        '0'..='9' => {
            expr.push(c);
            true
        }
        '.' => {
            // Reject a second dot inside the current number; lead with "0."
            // when the dot would start a number.
            let mut seen_dot = false;
            for p in expr.chars().rev() {
                if p.is_ascii_digit() {
                    continue;
                }
                if p == '.' {
                    seen_dot = true;
                    continue;
                }
                break;
            }
            if seen_dot {
                return false;
            }
            let needs_lead = expr.chars().last().is_none_or(|l| !l.is_ascii_digit());
            if needs_lead {
                expr.push_str("0.");
            } else {
                expr.push('.');
            }
            true
        }
        '%' => {
            // Postfix percent only makes sense right after a digit.
            if expr.chars().last().is_some_and(|l| l.is_ascii_digit()) {
                expr.push('%');
                true
            } else {
                false
            }
        }
        '+' | '−' | '×' | '÷' => push_op(expr, c),
        _ => false,
    }
}

fn push_op(expr: &mut String, c: char) -> bool {
    if expr.is_empty() {
        // Only a leading unary minus is allowed.
        if c == '−' {
            expr.push('−');
            return true;
        }
        return false;
    }
    let last = expr.chars().last().unwrap();
    if is_binary_op_char(last) {
        let binary_pending = match last {
            '−' => trailing_minus_is_binary(expr),
            _ => true,
        };
        if binary_pending {
            if c == '−' {
                // "2+" then '−' → unary minus on the next operand.
                expr.push('−');
            } else {
                // Replace the pending operator ("2+" then '×' → "2×").
                expr.pop();
                expr.push(c);
            }
            return true;
        }
        // Trailing '−' is a pending unary minus: "2+−".
        if c == '−' {
            expr.push('−'); // double negation is legal
        } else {
            // Drop the pending negate and replace the real pending op:
            // "2+−" then '×' → "2×".
            expr.pop();
            if let Some(prev) = expr.chars().last() {
                if is_binary_op_char(prev) {
                    expr.pop();
                }
            }
            if expr.is_empty() {
                if c == '−' {
                    expr.push('−');
                }
            } else {
                expr.push(c);
            }
        }
        return true;
    }
    if last == '%' || last == '.' {
        // Operators after '%' are fine ("50%+2"); after '.' too ("2.+3").
        expr.push(c);
        return true;
    }
    if last.is_ascii_digit() {
        expr.push(c);
        return true;
    }
    false
}

fn normalize(s: &str) -> String {
    s.replace('×', "*").replace('÷', "/").replace('−', "-")
}

fn eval_expr(s: &str) -> Result<f64, EvalErr> {
    let norm = normalize(s);
    let b = norm.as_bytes();
    let mut i = 0;
    let v = parse_add(b, &mut i)?;
    // Trailing whitespace can't occur; anything left is malformed.
    if i != b.len() {
        return Err(EvalErr::Invalid);
    }
    if !v.is_finite() {
        return Err(EvalErr::Overflow);
    }
    Ok(v)
}

fn parse_add(b: &[u8], i: &mut usize) -> Result<f64, EvalErr> {
    let mut v = parse_mul(b, i)?;
    while *i < b.len() {
        match b[*i] {
            b'+' => {
                *i += 1;
                v += parse_mul(b, i)?;
            }
            b'-' => {
                *i += 1;
                v -= parse_mul(b, i)?;
            }
            _ => break,
        }
    }
    Ok(v)
}

fn parse_mul(b: &[u8], i: &mut usize) -> Result<f64, EvalErr> {
    let mut v = parse_postfix(b, i)?;
    while *i < b.len() {
        match b[*i] {
            b'*' => {
                *i += 1;
                v *= parse_postfix(b, i)?;
            }
            b'/' => {
                *i += 1;
                let r = parse_postfix(b, i)?;
                if r == 0.0 {
                    return Err(EvalErr::DivZero);
                }
                v /= r;
            }
            _ => break,
        }
    }
    Ok(v)
}

/// Postfix '%' = divide by 100 ("50%" → 0.5).
fn parse_postfix(b: &[u8], i: &mut usize) -> Result<f64, EvalErr> {
    let mut v = parse_unary(b, i)?;
    while *i < b.len() && b[*i] == b'%' {
        *i += 1;
        v *= 0.01;
    }
    Ok(v)
}

fn parse_unary(b: &[u8], i: &mut usize) -> Result<f64, EvalErr> {
    if *i < b.len() && b[*i] == b'-' {
        *i += 1;
        return Ok(-parse_unary(b, i)?);
    }
    let start = *i;
    while *i < b.len() && (b[*i].is_ascii_digit() || b[*i] == b'.') {
        *i += 1;
    }
    // Optional exponent: e/E [+-]? digits — required digits after 'e'.
    if *i < b.len() && (b[*i] == b'e' || b[*i] == b'E') {
        let mut j = *i + 1;
        if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
            j += 1;
        }
        let dstart = j;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        if j > dstart {
            *i = j;
        }
        // "2e" with no digits: leave 'e' unconsumed → trailing garbage →
        // Invalid at top level. But "2e+3" consumed fully above.
    }
    if *i == start {
        return Err(EvalErr::Invalid);
    }
    std::str::from_utf8(&b[start..*i])
        .ok()
        .and_then(|t| t.parse::<f64>().ok())
        .ok_or(EvalErr::Invalid)
}

/// Format a result for display *and* reuse as input (plain or `1.2e3`).
fn format_num(v: f64) -> String {
    if v == 0.0 {
        return "0".into(); // also collapses -0
    }
    let a = v.abs();
    if a >= 1e15 || a < 1e-6 {
        // Scientific notation; trim mantissa zeros: "1.000000e20" → "1e20".
        let s = format!("{v:.6e}");
        if let Some(epos) = s.find('e') {
            let mant = s[..epos].trim_end_matches('0').trim_end_matches('.');
            format!("{mant}{}", &s[epos..])
        } else {
            s
        }
    } else if (v - v.round()).abs() < 1e-9 {
        format!("{}", v.round() as i64)
    } else {
        let s = format!("{v:.8}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

// ---------------------------------------------------------------------------
// Layout (pure, unit-tested). All geometry derives from canvas size.
// ---------------------------------------------------------------------------

const HISTORY_BTN_W: i32 = 168;
const HISTORY_BTN_H: i32 = 48;
const HEADER_H: i32 = 72;
const KEY_GAP: i32 = 12;
const KEY_RADIUS: f32 = 24.0;
const DISPLAY_MIN_H: i32 = 200;

#[derive(Debug, Clone, Copy)]
struct Key {
    rect: Rect,
    kind: KeyKind,
    label: &'static str,
}

#[derive(Debug, Clone)]
struct KeypadLayout {
    display: Rect,
    keys: Vec<Key>,
}

fn keypad_layout(w: i32, h: i32) -> KeypadLayout {
    let m = ui::MARGIN;
    let bottom_edge = h - ui::BOTTOM_SAFE - 16;
    let avail_w = (w - 2 * m).max(0);
    let btn_w = (avail_w - KEY_GAP * 3) / 4;
    // Buttons aim for ≥56 px; on the 720x1600 panel they land ~180 px.
    let room = bottom_edge - (ui::TOP_SAFE + HEADER_H) - KEY_GAP * 4 - DISPLAY_MIN_H;
    let btn_h = (room / 5).clamp(56, (btn_w * 6 / 5).max(56));
    let grid_h = btn_h * 5 + KEY_GAP * 4;
    let grid_top = (bottom_edge - grid_h).max(ui::TOP_SAFE + HEADER_H + DISPLAY_MIN_H);
    let display = Rect::new(
        m,
        ui::TOP_SAFE + HEADER_H,
        avail_w,
        (grid_top - KEY_GAP) - (ui::TOP_SAFE + HEADER_H),
    );

    // kind order per row; labels are display glyphs.
    let rows: [[(KeyKind, &'static str); 4]; 5] = [
        [
            (KeyKind::Clear, "AC"),
            (KeyKind::Negate, "±"),
            (KeyKind::Percent, "%"),
            (KeyKind::Op(Op::Div), "÷"),
        ],
        [
            (KeyKind::Digit('7'), "7"),
            (KeyKind::Digit('8'), "8"),
            (KeyKind::Digit('9'), "9"),
            (KeyKind::Op(Op::Mul), "×"),
        ],
        [
            (KeyKind::Digit('4'), "4"),
            (KeyKind::Digit('5'), "5"),
            (KeyKind::Digit('6'), "6"),
            (KeyKind::Op(Op::Sub), "−"),
        ],
        [
            (KeyKind::Digit('1'), "1"),
            (KeyKind::Digit('2'), "2"),
            (KeyKind::Digit('3'), "3"),
            (KeyKind::Op(Op::Add), "+"),
        ],
        [
            (KeyKind::Backspace, "\u{e14a}"),
            (KeyKind::Digit('0'), "0"),
            (KeyKind::Dot, "."),
            (KeyKind::Equals, "="),
        ],
    ];
    let mut keys = Vec::with_capacity(20);
    for (r, row) in rows.iter().enumerate() {
        for (c, (kind, label)) in row.iter().enumerate() {
            let x = m + c as i32 * (btn_w + KEY_GAP);
            let y = grid_top + r as i32 * (btn_h + KEY_GAP);
            keys.push(Key {
                rect: Rect::new(x, y, btn_w, btn_h),
                kind: *kind,
                label,
            });
        }
    }
    KeypadLayout { display, keys }
}

fn history_btn_rect(_w: i32) -> Rect {
    Rect::new(ui::MARGIN, ui::TOP_SAFE + 10, HISTORY_BTN_W, HISTORY_BTN_H)
}

/// Panel sheet geometry. `p` is the eased open progress (0 closed, 1 open);
/// the sheet slides down from above the top safe area. Returns the on-screen
/// rect (already offset) — all children are positioned relative to it.
fn history_panel(w: i32, h: i32, p: f32) -> Rect {
    let top = ui::TOP_SAFE;
    let ph = h - top - ui::BOTTOM_SAFE - 8;
    let dy = ((1.0 - p) * -(ph as f32 + top as f32)) as i32;
    Rect::new(0, top + dy, w, ph)
}

/// Header (title + clear) and entry row rects, absolute coords matching
/// `history_panel`'s offset rect. Rows are newest-first.
fn history_rows(panel: Rect, count: usize) -> (Rect, Rect, Vec<Rect>) {
    let m = 16;
    let header = Rect::new(m, panel.y + 12, panel.w - 2 * m, 56);
    let clear = Rect::new(header.right() - 96, header.y + 8, 96, 40);
    let avail = panel.bottom() - header.bottom() - 16;
    let n = count.max(1) as i32;
    let pitch = (avail / n).max(1);
    let row_h = (pitch - 8).clamp(36, 72);
    let mut rows = Vec::with_capacity(count);
    for i in 0..count {
        let y = header.bottom() + 8 + i as i32 * pitch;
        rows.push(Rect::new(m, y, panel.w - 2 * m, row_h));
    }
    (header, clear, rows)
}

// ---------------------------------------------------------------------------
// Animation helpers.
// ---------------------------------------------------------------------------

const PRESS_FADE_S: f32 = 0.09;
const HIST_OPEN_S: f32 = 0.24;
const HIST_CLOSE_S: f32 = 0.18;

// ---------------------------------------------------------------------------
// Hit targets.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Hit {
    Key(usize),
    HistRow(usize), // index into history entries, newest-first
    HistClear,
    HistToggle,
    Scrim,
    Panel,
}

impl Hit {
    /// Hits without a meaningful rect are "sticky": the finger counts as
    /// inside for the whole press (scrim/panel background taps).
    fn sticky(self) -> bool {
        matches!(self, Self::Scrim | Self::Panel)
    }
}

// ---------------------------------------------------------------------------
// App state.
// ---------------------------------------------------------------------------

struct CalcApp {
    expr: String,
    preview: String,
    error: Option<EvalErr>,
    value: Option<f64>,
    history: VecDeque<HistEntry>,

    size: Size,
    pressed: Option<(i32, Hit, bool)>, // touch id, hit, finger-inside flag
    press_t: f32,
    flash: Option<(Hit, f32)>, // released hit fading out
    history_open: bool,
    hist_tween: ui::Tween,
}

impl CalcApp {
    fn new() -> Self {
        Self {
            expr: String::new(),
            preview: "0".into(),
            error: None,
            value: None,
            history: VecDeque::new(),
            size: Size::new(720, 1600),
            pressed: None,
            press_t: 1.0,
            flash: None,
            history_open: false,
            hist_tween: ui::Tween::new(0.0, 0.0, 0.001, BEZIER_ENTER),
        }
    }

    // -- expression mutation -------------------------------------------------

    fn recalc(&mut self) {
        self.error = None;
        self.value = None;
        if self.expr.is_empty() {
            self.preview = "0".into();
            return;
        }
        match eval_expr(&self.expr) {
            Ok(v) => {
                self.value = Some(v);
                self.preview = format_num(v);
            }
            Err(e) => {
                // A trailing pending operator isn't an error — preview the
                // stripped expression instead ("2+" previews "2").
                let stripped = strip_pending_ops(&self.expr);
                if stripped.is_empty() {
                    self.preview = "0".into();
                } else if let Ok(v) = eval_expr(stripped) {
                    self.value = Some(v);
                    self.preview = format_num(v);
                } else {
                    self.error = Some(e);
                    self.preview.clear();
                }
            }
        }
    }

    fn input_char(&mut self, c: char) {
        match c {
            '0'..='9' | '.' | '%' | '+' | '−' | '×' | '÷' => {
                if push_input(&mut self.expr, c) {
                    self.recalc();
                }
            }
            '=' => self.commit(),
            _ => {}
        }
    }

    fn backspace(&mut self) {
        if self.expr.pop().is_some() {
            self.recalc();
        }
    }

    fn clear(&mut self) {
        self.expr.clear();
        self.recalc();
    }

    fn negate(&mut self) {
        if toggle_sign(&mut self.expr) {
            self.recalc();
        }
    }

    /// '=' commits the live value into history and continues from the result.
    fn commit(&mut self) {
        let Some(v) = self.value else { return };
        if self.error.is_some() || self.expr.is_empty() {
            return;
        }
        let result = format_num(v);
        self.history.push_back(HistEntry {
            expr: self.expr.clone(),
            result: result.clone(),
        });
        while self.history.len() > HISTORY_CAP {
            self.history.pop_front();
        }
        self.expr = result;
        self.recalc();
    }

    fn reuse_history(&mut self, idx_newest_first: usize) {
        let Some(entry) = self.history.iter().rev().nth(idx_newest_first) else {
            return;
        };
        self.expr = entry.result.clone();
        self.recalc();
        self.close_history();
    }

    // -- history sheet ---------------------------------------------------------

    fn open_history(&mut self) {
        if self.history_open {
            return;
        }
        self.history_open = true;
        let from = self.hist_tween.value();
        self.hist_tween = ui::Tween::new(from, 1.0, HIST_OPEN_S, BEZIER_ENTER);
    }

    fn close_history(&mut self) {
        if !self.history_open && self.hist_tween.done() {
            return;
        }
        self.history_open = false;
        let from = self.hist_tween.value();
        self.hist_tween = ui::Tween::new(from, 0.0, HIST_CLOSE_S, BEZIER_EXIT);
    }

    fn history_progress(&self) -> f32 {
        self.hist_tween.value()
    }

    // -- hit testing -----------------------------------------------------------

    fn history_hit(&self, pos: Point) -> Option<Hit> {
        let p = self.history_progress();
        if p <= 0.5 {
            return None;
        }
        let panel = history_panel(self.size.w, self.size.h, p);
        if panel.contains(pos) {
            let (_, clear, rows) = history_rows(panel, self.history.len());
            if clear.contains(pos) {
                return Some(Hit::HistClear);
            }
            // Rows are stored top→bottom = newest→oldest.
            for (i, r) in rows.iter().enumerate() {
                if r.contains(pos) {
                    return Some(Hit::HistRow(i));
                }
            }
            return Some(Hit::Panel);
        }
        Some(Hit::Scrim)
    }

    fn hit_at(&self, pos: Point) -> Option<Hit> {
        // While the sheet is sliding, swallow touches so nothing lands on a
        // moving target (the 240 ms window is shorter than a deliberate tap).
        if !self.hist_tween.done() {
            return Some(Hit::Panel);
        }
        if let Some(h) = self.history_hit(pos) {
            return Some(h);
        }
        if history_btn_rect(self.size.w).contains(pos) {
            return Some(Hit::HistToggle);
        }
        let layout = keypad_layout(self.size.w, self.size.h);
        layout
            .keys
            .iter()
            .position(|k| k.rect.contains(pos))
            .map(Hit::Key)
    }

    fn hit_rect(&self, hit: Hit) -> Rect {
        match hit {
            Hit::Key(i) => keypad_layout(self.size.w, self.size.h)
                .keys
                .get(i)
                .map_or(Rect::ZERO, |k| k.rect),
            Hit::HistRow(i) => {
                let p = self.history_progress();
                let panel = history_panel(self.size.w, self.size.h, p);
                let (_, _, rows) = history_rows(panel, self.history.len());
                rows.get(i).copied().unwrap_or(Rect::ZERO)
            }
            Hit::HistClear => {
                let p = self.history_progress();
                let panel = history_panel(self.size.w, self.size.h, p);
                let (_, clear, _) = history_rows(panel, self.history.len());
                clear
            }
            Hit::HistToggle => history_btn_rect(self.size.w),
            _ => Rect::ZERO,
        }
    }

    fn hit_inside(&self, hit: Hit, pos: Point) -> bool {
        hit.sticky() || self.hit_rect(hit).contains(pos)
    }

    fn activate(&mut self, hit: Hit) {
        match hit {
            Hit::Key(i) => {
                let layout = keypad_layout(self.size.w, self.size.h);
                let Some(key) = layout.keys.get(i) else {
                    return;
                };
                match key.kind {
                    KeyKind::Digit(d) => self.input_char(d),
                    KeyKind::Dot => self.input_char('.'),
                    KeyKind::Percent => self.input_char('%'),
                    KeyKind::Negate => self.negate(),
                    KeyKind::Clear => self.clear(),
                    KeyKind::Backspace => self.backspace(),
                    KeyKind::Op(op) => {
                        let c = op.ch();
                        self.input_char(c);
                    }
                    KeyKind::Equals => self.commit(),
                }
            }
            Hit::HistRow(i) => self.reuse_history(i),
            Hit::HistToggle => {
                // History pill: toggle the sheet.
                if self.history_open {
                    self.close_history();
                } else {
                    self.open_history();
                }
            }
            Hit::HistClear => {
                self.history.clear();
            }
            Hit::Scrim => self.close_history(),
            Hit::Panel => {}
        }
    }

    // -- frame -----------------------------------------------------------------

    fn frame(&self, dirty: bool, animating: bool) -> AppFrame {
        AppFrame {
            dirty,
            animating,
            quit: false,
        }
    }

    fn tick_anims(&mut self, dt: Duration) -> bool {
        let mut running = false;
        if !self.hist_tween.done() {
            running |= self.hist_tween.tick(dt);
        }
        if let Some((_, t)) = &mut self.flash {
            *t += dt.as_secs_f32() / PRESS_FADE_S;
            if *t >= 1.0 {
                self.flash = None;
            } else {
                running = true;
            }
        }
        if self.pressed.is_some() {
            self.press_t = (self.press_t + dt.as_secs_f32() / PRESS_FADE_S).min(1.0);
            if self.press_t < 1.0 {
                running = true;
            }
        }
        running
    }
}

impl AppState for CalcApp {
    fn app_id(&self) -> &'static str {
        "rabbiz-calculator"
    }
    fn title(&self) -> &'static str {
        "Calculator"
    }

    fn handle_event(&mut self, event: AppEvent, _ti: &TextInputCtl) -> AppFrame {
        match event {
            AppEvent::TouchDown { id, pos } => {
                if self.pressed.is_some() {
                    return self.frame(false, false); // one touch at a time
                }
                let Some(hit) = self.hit_at(pos) else {
                    return self.frame(false, false);
                };
                self.pressed = Some((id, hit, true));
                self.press_t = 0.0;
                self.frame(true, true)
            }
            AppEvent::TouchMove { id, pos } => {
                let Some((tid, hit, inside)) = self.pressed else {
                    return self.frame(false, false);
                };
                if tid != id {
                    return self.frame(false, false);
                }
                let now_inside = self.hit_inside(hit, pos);
                if now_inside != inside {
                    self.pressed = Some((tid, hit, now_inside));
                    self.frame(true, false)
                } else {
                    self.frame(false, false)
                }
            }
            AppEvent::TouchUp { id, pos } => {
                let Some((tid, hit, inside)) = self.pressed.take() else {
                    return self.frame(false, false);
                };
                if tid != id {
                    self.pressed = Some((tid, hit, inside));
                    return self.frame(false, false);
                }
                let still_inside = self.hit_inside(hit, pos);
                if inside && still_inside {
                    self.activate(hit);
                }
                if matches!(hit, Hit::Key(_)) {
                    self.flash = Some((hit, 0.0));
                }
                self.frame(true, true)
            }
            AppEvent::Key { utf8: Some(s), .. } => {
                for ch in s.chars() {
                    match ch {
                        '0'..='9' | '.' | '%' => self.input_char(ch),
                        '+' => self.input_char('+'),
                        '-' | '−' => self.input_char('−'),
                        '*' | '×' | 'x' | 'X' => self.input_char('×'),
                        '/' | '÷' => self.input_char('÷'),
                        '=' | '\r' | '\n' => self.commit(),
                        _ => {}
                    }
                }
                self.frame(true, false)
            }
            AppEvent::Key { keysym: 0xff08, .. } => {
                self.backspace();
                self.frame(true, false)
            }
            AppEvent::TextCommit(s) => {
                for ch in s.chars() {
                    match ch {
                        '0'..='9' | '.' | '%' | '+' | '−' | '×' | '÷' | '=' => {
                            self.input_char(ch)
                        }
                        '-' => self.input_char('−'),
                        '*' | 'x' | 'X' => self.input_char('×'),
                        '/' => self.input_char('÷'),
                        _ => {}
                    }
                }
                self.frame(true, false)
            }
            AppEvent::TextBackspace(n) => {
                for _ in 0..n {
                    self.backspace();
                }
                self.frame(true, false)
            }
            AppEvent::Tick(dt) => {
                let running = self.tick_anims(dt);
                // Dirty while animating so each tick repaints; the final tick
                // also repaints the settled state, then animation stops.
                self.frame(true, running)
            }
            _ => self.frame(false, false),
        }
    }

    fn render(&mut self, canvas: &mut Canvas, theme: &Theme) {
        let w = canvas.size().w;
        let h = canvas.size().h;
        self.size = Size::new(w, h);
        let layout = keypad_layout(w, h);
        let hist_p = self.history_progress();
        let hist_active = hist_p > 0.001 || self.history_open;

        self.draw_readout(canvas, theme, &layout);

        if hist_active {
            self.draw_history(canvas, theme, w, h, hist_p);
        } else {
            self.draw_history_button(canvas, theme, w);
        }

        for (i, key) in layout.keys.iter().enumerate() {
            self.draw_key(canvas, theme, key, i);
        }
    }
}

// ---------------------------------------------------------------------------
// Rendering.
// ---------------------------------------------------------------------------

impl CalcApp {
    fn draw_history_button(&mut self, canvas: &mut Canvas, theme: &Theme, w: i32) {
        let r = history_btn_rect(w);
        let pressed = self
            .pressed
            .map(|(_, h, inside)| inside && h == Hit::HistToggle)
            .unwrap_or(false)
            || self
                .flash
                .map(|(h, t)| h == Hit::HistToggle && t < 1.0)
                .unwrap_or(false);
        let alpha = if pressed {
            if self.pressed.is_some() {
                ease_out(self.press_t)
            } else {
                1.0 - self.flash.map_or(0.0, |(_, t)| t.min(1.0))
            }
        } else {
            0.0
        };
        canvas.fill_rounded_rect(r, (r.h as f32) / 2.0, theme.surface2);
        if alpha > 0.0 {
            canvas.fill_rounded_rect(r, (r.h as f32) / 2.0, ui::state_layer(theme, alpha * 0.14));
        }
        // History icon + label.
        let icon_st = text::LabelStyle::icon(24.0, theme.text2);
        if let Some(px) = text::label("\u{e8b6}", icon_st, None) {
            let iw = px.width() as i32;
            let ih = px.height() as i32;
            canvas.blit(
                &Pixels::new(Size::new(iw, ih), px.data().to_vec()),
                Point::new(r.x + 16, r.y + (r.h - ih) / 2),
            );
        }
        ui::draw_text(
            canvas,
            "History",
            &ui::label(theme.text),
            r.x + 48,
            r.y + (r.h as f32 / 2.0 - 14.0) as i32,
            Some(r.w - 56),
        );
    }

    fn draw_readout(&mut self, canvas: &mut Canvas, theme: &Theme, layout: &KeypadLayout) {
        let d = layout.display;
        if d.h < 40 {
            return;
        }
        let pad = 8;
        // Expression line: small, secondary, right-aligned in the top half,
        // shrinks with length.
        let expr = if self.expr.is_empty() {
            ""
        } else {
            self.expr.as_str()
        };
        if !expr.is_empty() {
            let n = expr.chars().count();
            let size = if n <= 14 {
                30.0
            } else if n <= 22 {
                24.0
            } else if n <= 34 {
                19.0
            } else {
                16.0
            };
            let st = ui::styled(size, 400, theme.text2);
            let expr_r = Rect::new(d.x, d.y, d.w, d.h / 2);
            ui::draw_text_right(canvas, expr, &st, expr_r, pad);
        }
        // Result line: large, right-aligned, shrinks with length.
        let result_rect = Rect::new(d.x, d.y + d.h / 2, d.w, d.h / 2 - pad);
        if let Some(err) = self.error {
            let st = ui::styled(30.0, 600, theme.danger);
            ui::draw_text_right(canvas, err.message(), &st, result_rect, pad);
        } else {
            let n = self.preview.chars().count();
            let size = if n <= 10 {
                58.0
            } else if n <= 16 {
                46.0
            } else if n <= 24 {
                36.0
            } else {
                28.0
            };
            let color = if self.expr.is_empty() {
                theme.text3
            } else {
                theme.text
            };
            let st = ui::styled(size, 600, color);
            ui::draw_text_right(canvas, &self.preview, &st, result_rect, pad);
        }
    }

    fn draw_history(&mut self, canvas: &mut Canvas, theme: &Theme, w: i32, h: i32, p: f32) {
        let panel = history_panel(w, h, p);
        // Scrim over everything else, fading with progress.
        let a = (theme.scrim[3] as f32 * p) as u8;
        canvas.fill_rect(
            Rect::new(0, 0, w, h),
            [theme.scrim[0], theme.scrim[1], theme.scrim[2], a],
        );

        canvas.fill_rounded_rect(panel, 28.0, theme.surface1);
        let count = self.history.len();
        let (_, clear, rows) = history_rows(panel, count);

        // "History" title.
        ui::draw_text(
            canvas,
            "History",
            &ui::title(theme.text),
            clear.x - 220,
            clear.y + 2,
            Some(200),
        );
        // Clear pill.
        let clear_pressed = self
            .pressed
            .map(|(_, hh, inside)| inside && hh == Hit::HistClear)
            .unwrap_or(false);
        canvas.fill_rounded_rect(clear, (clear.h as f32) / 2.0, theme.surface2);
        if clear_pressed {
            canvas.fill_rounded_rect(clear, (clear.h as f32) / 2.0, ui::state_layer(theme, 0.14));
        }
        ui::draw_centered(canvas, "Clear", &ui::label(theme.danger), clear);

        if count == 0 {
            // Honest empty state.
            let cy = panel.y + panel.h / 2 - 60;
            let icon_st = text::LabelStyle::icon(56.0, theme.text3);
            if let Some(px) = text::label("\u{e8b6}", icon_st, None) {
                let iw = px.width() as i32;
                let ih = px.height() as i32;
                canvas.blit(
                    &Pixels::new(Size::new(iw, ih), px.data().to_vec()),
                    Point::new(panel.x + (panel.w - iw) / 2, cy),
                );
            }
            ui::draw_centered(
                canvas,
                "No calculations yet",
                &ui::label(theme.text2),
                Rect::new(panel.x, cy + 76, panel.w, 40),
            );
            ui::draw_centered(
                canvas,
                "Results you compute will show up here",
                &ui::caption(theme.text3),
                Rect::new(panel.x, cy + 118, panel.w, 32),
            );
            return;
        }

        // Rows, newest first (absolute coords, already slide-offset).
        for (i, r) in rows.iter().enumerate() {
            let Some(entry) = self.history.iter().rev().nth(i) else {
                break;
            };
            let rr = *r;
            let pressed = self
                .pressed
                .map(|(_, hh, inside)| inside && hh == Hit::HistRow(i))
                .unwrap_or(false);
            canvas.fill_rounded_rect(rr, 20.0, theme.surface2);
            if pressed {
                canvas.fill_rounded_rect(rr, 20.0, ui::state_layer(theme, 0.12));
            }
            // Expression (small, dim, right aligned) then result (bold).
            let inner = rr.inset(16);
            let expr_r = Rect::new(inner.x, inner.y + 6, inner.w, 26);
            let res_r = Rect::new(inner.x, inner.y + inner.h / 2, inner.w, inner.h / 2 - 4);
            if !entry.expr.is_empty() {
                ui::draw_text_right(
                    canvas,
                    &entry.expr,
                    &ui::styled(19.0, 400, theme.text3),
                    expr_r,
                    0,
                );
            }
            ui::draw_text_right(
                canvas,
                &entry.result,
                &ui::styled(26.0, 600, theme.text),
                res_r,
                0,
            );
        }
    }

    fn draw_key(&mut self, canvas: &mut Canvas, theme: &Theme, key: &Key, idx: usize) {
        let r = key.rect;
        // Container color by role.
        let (bg, fg) = match key.kind {
            KeyKind::Equals => (theme.accent, theme.text_on_accent),
            KeyKind::Op(_) => {
                let pending = self.pending_op() == Some(key.kind);
                if pending {
                    (theme.accent, theme.text_on_accent)
                } else {
                    (theme.surface3, theme.accent)
                }
            }
            KeyKind::Clear | KeyKind::Negate | KeyKind::Percent | KeyKind::Backspace => {
                (theme.surface2, theme.text2)
            }
            KeyKind::Digit(_) | KeyKind::Dot => (theme.surface1, theme.text),
        };
        canvas.fill_rounded_rect(r, KEY_RADIUS.min(r.h as f32 / 2.0), bg);

        // Press / release-flash state layer.
        let mut alpha = 0.0f32;
        if let Some((_, hit, inside)) = self.pressed {
            if inside && hit == Hit::Key(idx) {
                alpha = ease_out(self.press_t) * 0.16;
            }
        }
        if let Some((hit, t)) = self.flash {
            if hit == Hit::Key(idx) && t < 1.0 {
                alpha = alpha.max((1.0 - ease_out(t)) * 0.16);
            }
        }
        if alpha > 0.0 {
            canvas.fill_rounded_rect(
                r,
                KEY_RADIUS.min(r.h as f32 / 2.0),
                ui::state_layer(theme, alpha),
            );
        }

        // Glyph: icon font for backspace, text otherwise.
        if key.kind == KeyKind::Backspace {
            let st = text::LabelStyle::icon((r.h as f32 * 0.42).min(44.0), fg);
            if let Some(px) = text::label(key.label, st, None) {
                let iw = px.width() as i32;
                let ih = px.height() as i32;
                canvas.blit(
                    &Pixels::new(Size::new(iw, ih), px.data().to_vec()),
                    Point::new(r.x + (r.w - iw) / 2, r.y + (r.h - ih) / 2),
                );
            }
        } else {
            let size = if r.h >= 150 { 40.0 } else { 34.0 };
            let st = ui::styled(size, 600, fg);
            ui::draw_centered(canvas, key.label, &st, r);
        }
    }

    /// The operator currently pending at the end of the expression, if any.
    /// "2×−" highlights '×' (the '−' is a pending unary negate).
    fn pending_op(&self) -> Option<KeyKind> {
        if !has_pending_tail(&self.expr) {
            return None;
        }
        let chars: Vec<char> = self.expr.chars().collect();
        let mut k = chars.len();
        while k > 0 {
            k -= 1;
            match chars[k] {
                '+' => return Some(KeyKind::Op(Op::Add)),
                '×' => return Some(KeyKind::Op(Op::Mul)),
                '÷' => return Some(KeyKind::Op(Op::Div)),
                '−' => {
                    // '−' is binary only when preceded by an operand; a
                    // pending unary minus is skipped and the scan continues.
                    if k > 0 && is_operand_end(chars[k - 1]) {
                        return Some(KeyKind::Op(Op::Sub));
                    }
                }
                // Any operand char before an operator means nothing is
                // pending at the tail.
                c if is_operand_end(c) || c == '%' => return None,
                _ => {}
            }
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Small drawing helpers.
// ---------------------------------------------------------------------------

fn ease_out(t: f32) -> f32 {
    bezier_eval(BEZIER_ENTER, t.clamp(0.0, 1.0))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Headless modes take over before the GUI starts; no args → GUI as usual.
    if args.iter().any(|a| a == "--mcp") {
        mcp::serve();
    }
    if let Some(pos) = args.iter().position(|a| a == "--cli") {
        std::process::exit(cli::run(&args[pos + 1..]));
    }
    rabbiz_app_kit::run_app(CalcApp::new());
}

// ---------------------------------------------------------------------------
// Tests (headless, no Wayland).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- evaluator ------------------------------------------------------------

    #[test]
    fn eval_basics() {
        assert_eq!(eval_expr("2+3").unwrap(), 5.0);
        assert_eq!(eval_expr("2+3×4").unwrap(), 14.0);
        assert_eq!(eval_expr("10÷4").unwrap(), 2.5);
        assert_eq!(eval_expr("−3+5").unwrap(), 2.0);
        assert_eq!(eval_expr("2×−3").unwrap(), -6.0);
        assert_eq!(eval_expr("50%").unwrap(), 0.5);
        assert_eq!(eval_expr("200+10%").unwrap(), 200.1);
        assert_eq!(eval_expr("1e3+1").unwrap(), 1001.0);
        assert_eq!(eval_expr("1.5e-2").unwrap(), 0.015);
    }

    #[test]
    fn eval_errors() {
        assert_eq!(eval_expr("5÷0"), Err(EvalErr::DivZero));
        assert_eq!(eval_expr("0÷0"), Err(EvalErr::DivZero));
        assert_eq!(eval_expr(""), Err(EvalErr::Invalid));
        assert_eq!(eval_expr("2+"), Err(EvalErr::Invalid));
        assert_eq!(eval_expr("1.2.3"), Err(EvalErr::Invalid));
        assert_eq!(eval_expr("2e"), Err(EvalErr::Invalid));
    }

    // -- formatting -------------------------------------------------------------

    #[test]
    fn format_basics() {
        assert_eq!(format_num(5.0), "5");
        assert_eq!(format_num(-0.0), "0");
        assert_eq!(format_num(0.1 + 0.2), "0.3");
        assert_eq!(format_num(1e20), "1e20");
        assert_eq!(format_num(1.5e-7), "1.5e-7");
        // Formatted results must re-parse (history reuse / "=" continuation).
        for v in [1e20, 1.5e-7, 123.456, -42.0] {
            let s = format_num(v);
            let back: f64 = s.parse().unwrap();
            assert!((back - v).abs() / v.abs() < 1e-6, "{s} vs {v}");
        }
    }

    // -- input rules ------------------------------------------------------------

    #[test]
    fn input_rules() {
        let mut e = String::new();
        for c in "2+3×4".chars() {
            push_input(&mut e, c);
        }
        assert_eq!(e, "2+3×4");

        // Operator replaces pending operator.
        push_input(&mut e, '+');
        push_input(&mut e, '−');
        assert!(e.ends_with("+−"), "{e}"); // '−' after '+' is unary
        push_input(&mut e, '÷');
        assert!(e.ends_with("÷"), "{e}"); // drops pending negate, replaces '+'
    }

    #[test]
    fn op_replacement() {
        let mut e = "2+".to_string();
        push_op(&mut e, '×');
        assert_eq!(e, "2×"); // replaces pending operator
        push_op(&mut e, '+');
        assert_eq!(e, "2+"); // replaces again
        push_op(&mut e, '−');
        assert_eq!(e, "2+−"); // '−' after '+' is unary negate
        push_op(&mut e, '÷');
        assert_eq!(e, "2÷"); // '÷' drops pending negate, replaces '+'
    }

    #[test]
    fn dot_rules() {
        let mut e = String::new();
        push_input(&mut e, '.');
        assert_eq!(e, "0.");
        push_input(&mut e, '.');
        assert_eq!(e, "0."); // second dot rejected
        push_input(&mut e, '5');
        assert_eq!(e, "0.5");
        push_input(&mut e, '+');
        push_input(&mut e, '.');
        assert_eq!(e, "0.5+0.");
    }

    #[test]
    fn percent_rules() {
        let mut e = String::new();
        push_input(&mut e, '%');
        assert!(e.is_empty());
        push_input(&mut e, '5');
        push_input(&mut e, '%');
        push_input(&mut e, '%');
        assert_eq!(e, "5%");
    }

    #[test]
    fn sign_toggle() {
        let mut e = "12+3".to_string();
        toggle_sign(&mut e);
        assert_eq!(e, "12+−3");
        toggle_sign(&mut e);
        assert_eq!(e, "12+3");
        let mut e = "−3".to_string();
        toggle_sign(&mut e);
        assert_eq!(e, "3");
        let mut e = "1e-20".to_string();
        toggle_sign(&mut e);
        assert_eq!(e, "−1e-20");
        let mut e = "5×".to_string();
        assert!(!toggle_sign(&mut e));
    }

    #[test]
    fn pending_strip() {
        assert_eq!(strip_pending_ops("2+"), "2");
        assert_eq!(strip_pending_ops("2+−"), "2");
        assert_eq!(strip_pending_ops("2−3"), "2−3");
        assert_eq!(strip_pending_ops("−"), "");
    }

    // -- layout -----------------------------------------------------------------

    #[test]
    fn layout_fits_panel() {
        for (w, h) in [(720, 1600), (720, 1280), (400, 800), (1080, 2400)] {
            let l = keypad_layout(w, h);
            assert_eq!(l.keys.len(), 20);
            let bottom = l.keys.iter().map(|k| k.rect.bottom()).max().unwrap();
            assert!(
                bottom <= h - ui::BOTTOM_SAFE,
                "{w}x{h}: keys overflow bottom"
            );
            for k in &l.keys {
                assert!(k.rect.w >= 40, "key too narrow: {}", k.rect.w);
                assert!(k.rect.h >= 48, "key too short: {} @{}x{}", k.rect.h, w, h);
                assert!(k.rect.x >= 0 && k.rect.right() <= w);
            }
            // No overlaps.
            for (i, a) in l.keys.iter().enumerate() {
                for b in &l.keys[i + 1..] {
                    assert!(a.rect.intersect(b.rect).is_none(), "overlap {a:?} {b:?}");
                }
            }
            // First key row starts at grid_top.
            assert!(l.display.bottom() <= l.keys[0].rect.y);
        }
    }

    #[test]
    fn layout_reference_size() {
        let l = keypad_layout(720, 1600);
        // Digit keys should be well above the 56 px minimum on the A21s panel.
        let seven = l.keys.iter().find(|k| k.label == "7").unwrap();
        assert!(seven.rect.h >= 56, "digit height {}", seven.rect.h);
        assert!(seven.rect.w >= 56);
        // '=' is the accent-filled bottom-right key.
        let eq = l.keys.iter().find(|k| k.kind == KeyKind::Equals).unwrap();
        assert_eq!(eq.rect, l.keys[19].rect);
    }

    #[test]
    fn history_panel_geometry() {
        let closed = history_panel(720, 1600, 0.0);
        let open = history_panel(720, 1600, 1.0);
        assert_eq!(open.y, ui::TOP_SAFE);
        assert!(closed.y < open.y, "closed panel should sit above");
        assert!(
            closed.bottom() <= open.y + 1,
            "closed panel mostly offscreen"
        );
        let (_, _, rows) = history_rows(open, 5);
        assert_eq!(rows.len(), 5);
        for r in &rows {
            assert!(r.x >= 0 && r.right() <= open.w);
            assert!(r.h >= 40);
        }
    }

    // -- app-level behaviour ------------------------------------------------------

    #[test]
    fn app_state_smoke() {
        let mut app = CalcApp::new();
        assert_eq!(app.app_id(), "rabbiz-calculator");
        assert_eq!(app.title(), "Calculator");
        app.input_char('2');
        app.input_char('+');
        app.input_char('3');
        assert_eq!(app.preview, "5");
        app.commit();
        assert_eq!(app.expr, "5");
        assert_eq!(app.history.len(), 1);
        assert_eq!(app.history[0].expr, "2+3");
        assert_eq!(app.history[0].result, "5");
        // Division by zero shows error, keeps expression editable.
        app.input_char('÷');
        app.input_char('0');
        app.input_char('=');
        assert!(app.error.is_some());
        assert_eq!(app.expr, "5÷0");
    }
}
