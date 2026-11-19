//! Pure calculator expression model: evaluation, formatting, and input rules.

pub(crate) const MAX_EXPR_CHARS: usize = 48;
pub(crate) const HISTORY_CAP: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Op {
    Add,
    Sub,
    Mul,
    Div,
}

impl Op {
    pub(crate) fn ch(self) -> char {
        match self {
            Self::Add => '+',
            Self::Sub => '−', // U+2212 minus sign, matches keypad glyph
            Self::Mul => '×',
            Self::Div => '÷',
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KeyKind {
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
pub(crate) enum EvalErr {
    DivZero,
    Invalid,
    Overflow,
}

impl EvalErr {
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::DivZero => "Can't divide by 0",
            Self::Invalid => "Invalid expression",
            Self::Overflow => "Number too large",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct HistEntry {
    pub(crate) expr: String,
    pub(crate) result: String,
}

/// Char classes that can end an operand (i.e. what may precede a binary op).
pub(crate) fn is_operand_end(c: char) -> bool {
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
pub(crate) fn has_pending_tail(expr: &str) -> bool {
    strip_pending_ops(expr).len() < expr.len()
}

/// Strip trailing pending operators so "2+−" and "2−" preview as "2".
/// A trailing '−' is always pending — binary ("2−") or unary ("2+−").
pub(crate) fn strip_pending_ops(expr: &str) -> &str {
    let mut s = expr;
    while let Some(last) = s.chars().last() {
        if !matches!(last, '+' | '−' | '×' | '÷') {
            break;
        }
        s = &s[..s.len() - last.len_utf8()];
    }
    s
}

/// The operator currently pending at the end of the expression, if any.
/// "2×−" highlights '×' (the '−' is a pending unary negate).
pub(crate) fn pending_op(expr: &str) -> Option<KeyKind> {
    if !has_pending_tail(expr) {
        return None;
    }
    let chars: Vec<char> = expr.chars().collect();
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

/// Insert/remove a leading unary minus on the *last number token* of `expr`.
/// Returns true when the expression changed.
pub(crate) fn toggle_sign(expr: &mut String) -> bool {
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
pub(crate) fn push_input(expr: &mut String, c: char) -> bool {
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

pub(crate) fn eval_expr(s: &str) -> Result<f64, EvalErr> {
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
pub(crate) fn format_num(v: f64) -> String {
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
}
