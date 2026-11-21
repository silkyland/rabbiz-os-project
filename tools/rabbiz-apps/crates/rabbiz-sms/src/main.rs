//! rabbiz-sms — Messages app for RabbizOS.
//!
//! Conversation list → thread view with M3-style bubbles (incoming left on
//! `surface1`, outgoing right on `accent`), run grouping with a single
//! timestamp per run, day separators, and honest delivery states
//! (Queued/Sent/Failed). Messages persist to
//! `~/.local/share/rabbiz-sms/messages.json` (atomic tmp+rename).
//!
//! Telephony reality (device.toml `modem = "planned"`; the removed QML
//! dialer's mm-backend.sh was an unwired reference): sends go through
//! `backend::Backend`, which probes for
//! `mmcli` once. No modem → messages stay `Queued` and are still saved —
//! nothing is faked as delivered.

mod app;
mod backend;
mod contacts;
mod headless;
mod model;
mod store;

/// Value of a GUI hand-off flag: `--flag <value>` or `--flag=<value>`.
fn gui_arg(args: &[String], flag: &str) -> Option<String> {
    if let Some(i) = args.iter().position(|a| a == flag) {
        return args.get(i + 1).cloned();
    }
    let prefix = format!("{flag}=");
    args.iter()
        .find_map(|a| a.strip_prefix(&prefix).map(str::to_string))
        .filter(|v| !v.is_empty())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--mcp") {
        headless::mcp_serve();
    }
    if let Some(pos) = args.iter().position(|a| a == "--cli") {
        std::process::exit(headless::run_cli(&args[pos + 1..]));
    }
    // GUI hand-off: `--to <number>` opens the thread or a prefilled
    // compose (from rabbiz-contacts). Never auto-sends.
    rabbiz_app_kit::run_app(app::SmsApp::new(gui_arg(&args, "--to")));
}

#[cfg(test)]
mod arg_tests {
    use super::*;

    #[test]
    fn gui_arg_parses_space_and_equals_forms() {
        let strs = |xs: &[&str]| xs.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();
        assert_eq!(
            gui_arg(&strs(&["--to", "0812345678"]), "--to"),
            Some("0812345678".to_string())
        );
        assert_eq!(
            gui_arg(&strs(&["--to=+66 81 234 5678"]), "--to"),
            Some("+66 81 234 5678".to_string())
        );
        assert_eq!(gui_arg(&strs(&["--to"]), "--to"), None);
        assert_eq!(gui_arg(&strs(&["--to="]), "--to"), None);
        assert_eq!(gui_arg(&strs(&[]), "--to"), None);
        assert_eq!(gui_arg(&strs(&["--dial", "081"]), "--to"), None);
    }
}
