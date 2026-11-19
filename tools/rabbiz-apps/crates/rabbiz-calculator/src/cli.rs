//! Headless CLI for `rabbiz-calculator --cli <cmd> [args]`.
//!
//! Shares the GUI's pure eval path (`eval_expr` + `format_num`) via
//! [`eval_to_text`]; the MCP `eval` tool uses the same function.
//! History is intentionally absent: the app keeps it in an in-memory
//! `VecDeque` only — nothing is persisted, so there is nothing to print.

use crate::{eval_expr, format_num, strip_pending_ops};

pub const USAGE: &str = "usage: rabbiz-calculator --cli <cmd> [args]\n  eval <expr>   evaluate an expression, e.g. '2+3*4' or '50%+2'\n  help          show this help\n\nOperators may be typed as + - * / x or the keypad glyphs + − × ÷.\nNote: history is session-only in the GUI and is not persisted.";

/// Translate common ASCII spellings into the display charset the expression
/// model uses: '*'/'x'/'X' → '×', '/' → '÷', '-' → '−'. The model's own
/// `normalize` maps '−' back to '-' for the parser, so exponents like
/// "1e-3" keep working.
fn ascii_to_display(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace()) // "2 + 3" from joined argv words
        .map(|c| match c {
            '*' | 'x' | 'X' => '×',
            '/' => '÷',
            '-' => '−',
            _ => c,
        })
        .collect()
}

/// Evaluate `raw` the same way the GUI readout does and return the text a
/// caller should show: the formatted result, or the error message.
/// Mirrors `CalcApp::recalc`: a trailing pending operator previews the
/// stripped expression rather than erroring ("2+" → "2").
pub(crate) fn eval_to_text(raw: &str) -> Result<String, String> {
    let expr = ascii_to_display(raw.trim());
    match eval_expr(&expr) {
        Ok(v) => Ok(format_num(v)),
        Err(e) => {
            let stripped = strip_pending_ops(&expr);
            if stripped.is_empty() {
                Ok("0".into())
            } else if let Ok(v) = eval_expr(stripped) {
                Ok(format_num(v))
            } else {
                Err(e.message().to_string())
            }
        }
    }
}

/// Dispatch one CLI command to its stdout text. `Err` is the message to
/// print on stderr with exit code 1.
fn command(cmd: &str, rest: &[String]) -> Result<String, String> {
    match cmd {
        "eval" => {
            if rest.is_empty() {
                return Err(format!("eval: missing expression\n{USAGE}"));
            }
            eval_to_text(&rest.join(" "))
        }
        "help" | "--help" | "-h" => Ok(USAGE.to_string()),
        _ => Err(format!("unknown command '{cmd}'\n{USAGE}")),
    }
}

/// Run `--cli` mode. `args` are the argv words after `--cli`.
/// Returns the process exit code (0 ok, 1 error).
pub fn run(args: &[String]) -> i32 {
    let Some((cmd, rest)) = args.split_first() else {
        eprintln!("{USAGE}");
        return 1;
    };
    match command(cmd, rest) {
        Ok(s) => {
            println!("{s}");
            0
        }
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EvalErr;

    #[test]
    fn eval_ascii_operators() {
        assert_eq!(eval_to_text("2+3*4").unwrap(), "14");
        assert_eq!(eval_to_text("10/4").unwrap(), "2.5");
        assert_eq!(eval_to_text("2x3").unwrap(), "6");
        assert_eq!(eval_to_text("10-4").unwrap(), "6");
        assert_eq!(eval_to_text("50%+2").unwrap(), "2.5");
        assert_eq!(eval_to_text("1e3+1").unwrap(), "1001");
    }

    #[test]
    fn eval_display_glyphs() {
        assert_eq!(eval_to_text("2+3×4").unwrap(), "14");
        assert_eq!(eval_to_text("2×−3").unwrap(), "-6");
        assert_eq!(eval_to_text("−3+5").unwrap(), "2");
    }

    #[test]
    fn eval_pending_tail_and_errors() {
        // Same preview semantics as the GUI readout.
        assert_eq!(eval_to_text("2+").unwrap(), "2");
        assert!(eval_to_text("5/0").is_err());
        assert!(eval_to_text("5÷0").is_err());
        assert_eq!(eval_to_text("5÷0").unwrap_err(), EvalErr::DivZero.message());
        assert!(eval_to_text("1.2.3").is_err());
    }

    #[test]
    fn dispatch_commands() {
        assert_eq!(command("help", &[]).unwrap(), USAGE);
        assert_eq!(command("--help", &[]).unwrap(), USAGE);
        assert!(command("bogus", &[]).is_err());
        assert!(command("eval", &[]).is_err());
        // Multi-word expressions join with spaces.
        assert_eq!(
            command("eval", &["2".into(), "+".into(), "3".into()]).unwrap(),
            "5"
        );
    }
}
