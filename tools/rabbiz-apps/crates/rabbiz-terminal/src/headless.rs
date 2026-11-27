//! Headless modes for rabbiz-terminal: `--cli` text CLI and `--mcp` MCP
//! server, plus the command-execution path shared with the GUI.
//!
//! Execution model (same as the GUI): `script -qfec <cmd> /dev/null` gives
//! the command a PTY so line-oriented tools behave; `sh -c` is the fallback
//! when `script` is absent. Output is ANSI-stripped and assembled into
//! lines by [`crate::ansi::OutputParser`] — the same rendering the app
//! shows in its scrollback.
//!
//! Bounds: the GUI runs unbounded (it already blocks the UI for the whole
//! command). Headless callers pass a timeout; on expiry the child's whole
//! process group is SIGKILLed (the child is spawned with `setpgid` so `sh
//! -c` grandchildren die too; `script`'s inner command sits in its own
//! session and is reaped via SIGHUP when the pty master closes). MCP
//! `terminal_run` defaults to 30 s, clamped to [1, 300] s. Arbitrary
//! command execution IS the tool's purpose — it is a terminal — the bound
//! only guarantees one call cannot hang the server forever.
//!
//! History is NOT available headless: `edit::History` is in-memory per
//! GUI session and is never persisted, so there is no `history` command.

use std::io::Read;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rabbiz_app_kit::mcp::{serve, Tool};
use serde_json::Value;

use crate::ansi::OutputParser;

/// MCP `terminal_run` default kill timeout.
pub const MCP_DEFAULT_TIMEOUT_SECS: u64 = 30;
/// Hard clamp for caller-supplied MCP timeouts.
const MAX_TIMEOUT_SECS: u64 = 300;
/// Cap on returned output; the tail is kept (most relevant for logs).
const OUTPUT_CAP: usize = 200_000;
/// How long readers get to drain pipes after the child exits/is killed.
const DRAIN_GRACE: Duration = Duration::from_secs(2);

/// Result of one headless command run.
#[derive(Debug)]
pub struct RunOutcome {
    /// ANSI-stripped, line-assembled output (same as GUI scrollback rows).
    pub lines: Vec<String>,
    /// Exit code; `None` when the child was killed by a signal (mapped to
    /// 128+sig on unix by callers that want a shell-style code).
    pub code: Option<i32>,
    pub timed_out: bool,
}

/// Assemble raw PTY/sh output into display lines (ANSI stripped).
fn parse_output(raw: &str) -> Vec<String> {
    let mut p = OutputParser::new();
    let mut rows = Vec::new();
    p.feed(raw, &mut rows);
    p.finish(&mut rows);
    rows
}

/// Shared command execution: PTY via `script -qfec` (fallback `sh -c`),
/// optional bounded wait. `timeout == None` blocks until exit (GUI path).
pub fn run(cmd: &str, cwd: &Path, timeout: Option<Duration>) -> Result<RunOutcome, String> {
    let mut child = spawn(cmd, cwd, timeout).map_err(|e| e.to_string())?;

    // Drain pipes on threads so a chatty child can't deadlock on a full
    // pipe buffer while we poll try_wait.
    let out_buf = Arc::new(Mutex::new(Vec::new()));
    let err_buf = Arc::new(Mutex::new(Vec::new()));
    let mut readers = Vec::new();
    if let Some(p) = child.stdout.take() {
        readers.push(read_to_buf(p, out_buf.clone()));
    }
    if let Some(p) = child.stderr.take() {
        readers.push(read_to_buf(p, err_buf.clone()));
    }

    let deadline = timeout.map(|d| Instant::now() + d);
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) => {
                if deadline.is_some_and(|d| Instant::now() >= d) {
                    // pgid == child pid (setpgid in pre_exec): kills the
                    // whole group including `sh -c` grandchildren.
                    // SAFETY: `child` has not been waited on yet so its PID is
                    // still live (not recycled) and equals its own process
                    // group ID from `setpgid(0, 0)` in `pre_exec`.
                    #[allow(unsafe_code)]
                    unsafe {
                        libc::killpg(child.id() as libc::pid_t, libc::SIGKILL);
                    }
                    let _ = child.wait();
                    timed_out = true;
                    break None;
                }
                std::thread::sleep(Duration::from_millis(15));
            }
            Err(e) => return Err(e.to_string()),
        }
    };

    // Readers finish at pipe EOF; give stragglers a bounded grace period,
    // then take whatever arrived (a surviving grandchild holding the pipe
    // open must not hang the caller). Handles are dropped, not joined —
    // join() on an unfinished reader would block forever.
    let drain_deadline = Instant::now() + DRAIN_GRACE;
    for h in &readers {
        while !h.is_finished() && Instant::now() < drain_deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    drop(readers);

    let mut text = String::from_utf8_lossy(&out_buf.lock().unwrap()).into_owned();
    // stderr is piped only on the `sh -c` fallback (script merges the
    // child's stderr into the pty already); appending an empty buffer is
    // a no-op on the script path.
    text.push_str(&String::from_utf8_lossy(&err_buf.lock().unwrap()));

    #[cfg(unix)]
    let code = status.and_then(|s| {
        use std::os::unix::process::ExitStatusExt;
        s.code().or_else(|| s.signal().map(|sig| 128 + sig))
    });
    #[cfg(not(unix))]
    let code = status.and_then(|s| s.code());

    Ok(RunOutcome {
        lines: parse_output(&text),
        code,
        timed_out,
    })
}

fn read_to_buf(
    pipe: impl Read + Send + 'static,
    buf: Arc<Mutex<Vec<u8>>>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut pipe = pipe;
        let mut limited = (&mut pipe).take((OUTPUT_CAP as u64) + 1);
        let mut tmp = [0u8; 8192];
        loop {
            match limited.read(&mut tmp) {
                Ok(0) | Err(_) => break,
                Ok(n) => buf.lock().unwrap().extend_from_slice(&tmp[..n]),
            }
        }
        // Drain any remaining bytes into a sink so a chatty child cannot deadlock
        // on a full pipe buffer while our in-memory buffer stays capped.
        let _ = std::io::copy(&mut pipe, &mut std::io::sink());
    })
}

fn configure(c: &mut Command, cwd: &Path, timeout: Option<Duration>, sh_fallback: bool) {
    c.current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped());
    if timeout.is_some() {
        c.stderr(Stdio::piped());
        // Own process group so the timeout path can killpg the whole tree.
        // Only applied for bounded runs; unbounded runs stay in our group
        // so Ctrl-C in `--cli run` reaches the child too.
        // SAFETY: The closure runs between `fork` and `exec` and only invokes
        // `libc::setpgid(0, 0)`, which is async-signal-safe and performs no
        // allocations or locking.
        #[allow(unsafe_code)]
        unsafe {
            c.pre_exec(|| {
                libc::setpgid(0, 0);
                Ok(())
            });
        }
    } else if sh_fallback {
        // Preserve the GUI's old fallback behaviour: merge stderr in.
        c.stderr(Stdio::piped());
    } else {
        c.stderr(Stdio::null());
    }
}

fn spawn(cmd: &str, cwd: &Path, timeout: Option<Duration>) -> std::io::Result<Child> {
    let mut c = Command::new("script");
    c.args(["-qfec", cmd, "/dev/null"]);
    configure(&mut c, cwd, timeout, false);
    match c.spawn() {
        Ok(child) => Ok(child),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let mut c = Command::new("sh");
            c.arg("-c").arg(cmd);
            configure(&mut c, cwd, timeout, true);
            c.spawn()
        }
        Err(e) => Err(e),
    }
}

// ---------- --cli ----------

fn print_usage() {
    println!(
        "rabbiz-terminal — RabbizOS terminal\n\
         \n\
         usage:\n\
         \x20 rabbiz-terminal                          GUI app\n\
         \x20 rabbiz-terminal --cli run <command...>   run a command, print output,\n\
         \x20                                           exit with its exit code\n\
         \x20 rabbiz-terminal --cli run --timeout <s> <command...>\n\
         \x20                                           kill after <s> seconds\n\
         \x20 rabbiz-terminal --cli help               this text\n\
         \x20 rabbiz-terminal --mcp                    MCP server on stdio\n\
         \n\
         commands run through the same `script -qfec` PTY path as the GUI.\n\
         `history` is not offered: command history is in-memory per GUI\n\
         session and is never persisted.\n\
         --mcp exposes `terminal_run` (default 30 s bound, max 300 s)."
    );
}

/// Parse `run [--timeout <s>] <command...>`; returns (timeout, command).
fn parse_run_args(args: &[String]) -> Result<(Option<Duration>, String), String> {
    let (timeout, rest) = match args.split_first() {
        Some((flag, rest)) if flag == "--timeout" => {
            let secs = rest
                .first()
                .ok_or("--timeout needs <seconds>")?
                .parse::<u64>()
                .map_err(|_| "--timeout needs <seconds>".to_string())?;
            (Some(Duration::from_secs(secs.max(1))), &rest[1..])
        }
        _ => (None, args),
    };
    let cmd = rest.join(" ");
    if cmd.trim().is_empty() {
        return Err("run needs a command".into());
    }
    Ok((timeout, cmd))
}

fn cli_main(args: &[String]) -> i32 {
    if args.is_empty() || matches!(args[0].as_str(), "help" | "--help" | "-h") {
        print_usage();
        return 0;
    }
    if args[0] != "run" {
        eprintln!("rabbiz-terminal: unknown cli command '{}'", args[0]);
        return 2;
    }
    let (timeout, cmd) = match parse_run_args(&args[1..]) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("rabbiz-terminal: {e}");
            return 2;
        }
    };
    let cwd = std::env::current_dir().unwrap_or_else(|_| crate::home_dir());
    match run(&cmd, &cwd, timeout) {
        Ok(o) => {
            for l in &o.lines {
                println!("{l}");
            }
            if o.timed_out {
                eprintln!("[timed out]");
                return 124; // coreutil `timeout` convention
            }
            // Shell-style: signal death reports 128+sig; no status -> 1.
            o.code.unwrap_or(1)
        }
        Err(e) => {
            eprintln!("rabbiz-terminal: {e}");
            1
        }
    }
}

// ---------- --mcp ----------

fn terminal_run(args: &Value) -> Result<String, String> {
    let cmd = args
        .get("command")
        .and_then(Value::as_str)
        .ok_or("missing 'command'")?;
    if cmd.trim().is_empty() {
        return Err("empty command".into());
    }
    let timeout = args
        .get("timeout_secs")
        .and_then(Value::as_u64)
        .unwrap_or(MCP_DEFAULT_TIMEOUT_SECS)
        .clamp(1, MAX_TIMEOUT_SECS);
    let cwd = args
        .get("cwd")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| crate::home_dir()));
    if !cwd.is_dir() {
        return Err(format!("cwd is not a directory: {}", cwd.display()));
    }
    match run(cmd, &cwd, Some(Duration::from_secs(timeout))) {
        Ok(o) => {
            let mut text = o.lines.join("\n");
            if o.timed_out {
                text.push_str(&format!("\n[timed out after {timeout}s]"));
            }
            text.push_str(&format!(
                "\n[exit {}]",
                o.code.map_or_else(|| "?".to_string(), |c| c.to_string())
            ));
            if text.len() > OUTPUT_CAP {
                let mut cut = text.len() - OUTPUT_CAP;
                while !text.is_char_boundary(cut) {
                    cut += 1;
                }
                text = format!("[output truncated, tail kept]\n{}", &text[cut..]);
            }
            Ok(text)
        }
        Err(e) => Err(e),
    }
}

static TOOLS: &[Tool] = &[Tool {
    name: "terminal_run",
    description: "Run a shell command on the device through the same `script -qfec` \
                  PTY path the Terminal app uses, and return ANSI-stripped output \
                  plus the exit code. Arbitrary commands are the point of a \
                  terminal tool; the call is bounded — default 30 s, set \
                  timeout_secs (1-300) to change. cwd defaults to the server's \
                  working directory.",
    input_schema: r#"{"type":"object","properties":{"command":{"type":"string","description":"shell command line"},"cwd":{"type":"string","description":"working directory"},"timeout_secs":{"type":"integer","description":"kill after N seconds (default 30, max 300)"}},"required":["command"]}"#,
    handler: terminal_run,
}];

/// Handle headless args before `run_app`. Returns `Some(exit_code)` when
/// the process should exit, `None` to continue to the GUI.
pub fn dispatch(args: &[String]) -> Option<i32> {
    match args.first().map(String::as_str)? {
        "--mcp" => {
            serve(TOOLS, "rabbiz-terminal", "0.1.0");
        }
        "--cli" => Some(cli_main(&args[1..])),
        "--help" | "-h" => {
            print_usage();
            Some(0)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &str) -> String {
        v.to_string()
    }

    #[test]
    fn parse_run_plain_command() {
        let (t, cmd) = parse_run_args(&[s("echo"), s("hi"), s("there")]).unwrap();
        assert_eq!(t, None);
        assert_eq!(cmd, "echo hi there");
    }

    #[test]
    fn parse_run_timeout() {
        let (t, cmd) = parse_run_args(&[s("--timeout"), s("5"), s("sleep"), s("1")]).unwrap();
        assert_eq!(t, Some(Duration::from_secs(5)));
        assert_eq!(cmd, "sleep 1");
    }

    #[test]
    fn parse_run_rejects_bad_input() {
        assert!(parse_run_args(&[]).is_err());
        assert!(parse_run_args(&[s("--timeout")]).is_err());
        assert!(parse_run_args(&[s("--timeout"), s("abc"), s("ls")]).is_err());
        assert!(parse_run_args(&[s("--timeout"), s("5")]).is_err());
        // A command that merely starts with dashes is still a command.
        assert!(parse_run_args(&[s("--foo")]).is_ok());
    }

    #[test]
    fn parse_output_strips_ansi_fixture() {
        // Same fixture style as ansi.rs tests: colours, OSC title, CR
        // progress rewrite — headless output must look like scrollback.
        let raw = "\x1b]0;title\x07\x1b[31mred\x1b[0m \x1b[32mok\x1b[0m\r\n10%\r100%\nplain\n";
        let lines = parse_output(raw);
        assert_eq!(lines, vec!["red ok", "100%", "plain"]);
        assert!(!lines.iter().any(|l| l.contains('\u{1b}')));
    }

    /// True when the `script -qfec` PTY path actually works here. BSD
    /// `script` (macOS) rejects the flags and yields empty output, so
    /// exec-dependent tests bail out instead of failing off-device.
    fn script_works() -> bool {
        run("echo __probe__", Path::new("/tmp"), None)
            .map(|o| o.lines.iter().any(|l| l.contains("__probe__")))
            .unwrap_or(false)
    }

    #[test]
    fn run_captures_exit_code() {
        if !script_works() {
            return;
        }
        let out = run("echo hi-$((20+22)); exit 3", Path::new("/tmp"), None).expect("run");
        assert!(out.lines.iter().any(|l| l.contains("hi-42")), "{out:?}");
        assert_eq!(out.code, Some(3));
    }

    #[test]
    fn run_times_out() {
        if !script_works() {
            return;
        }
        let out = run(
            "sleep 30",
            Path::new("/tmp"),
            Some(Duration::from_millis(300)),
        )
        .expect("run");
        assert!(out.timed_out);
    }

    #[test]
    fn mcp_tool_validates_args() {
        assert!(terminal_run(&serde_json::json!({})).is_err());
        assert!(terminal_run(&serde_json::json!({"command": "  "})).is_err());
        assert!(
            terminal_run(&serde_json::json!({"command": "pwd", "cwd": "/nonexistent-xyz"}))
                .is_err()
        );
    }

    #[test]
    fn mcp_tool_runs_and_reports() {
        if !script_works() {
            return;
        }
        let v = terminal_run(&serde_json::json!({"command": "echo mcp-ok", "timeout_secs": 10}))
            .expect("terminal_run");
        assert!(v.contains("mcp-ok"), "{v}");
        assert!(v.contains("[exit 0]"), "{v}");
    }

    #[test]
    fn tool_schema_parses() {
        for t in TOOLS {
            assert!(serde_json::from_str::<serde_json::Value>(t.input_schema).is_ok());
        }
    }
}
