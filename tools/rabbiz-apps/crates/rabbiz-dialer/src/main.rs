mod app;
mod backend;
mod contacts;
mod headless;
mod model;

/// Value of a GUI hand-off flag: `--flag <value>` or `--flag=<value>`.
/// Returns `None` when the flag is absent or has no value.
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
    // Headless modes take over before any Wayland connection is attempted.
    if args.iter().any(|a| a == "--mcp") {
        headless::mcp_serve();
    }
    if let Some(i) = args.iter().position(|a| a == "--cli") {
        std::process::exit(headless::run_cli(&args[i + 1..]));
    }
    // GUI hand-off: `--dial <number>` prefills the keypad (no auto-call).
    app::run(gui_arg(&args, "--dial"));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strs(xs: &[&str]) -> Vec<String> {
        xs.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn gui_arg_parses_space_and_equals_forms() {
        assert_eq!(
            gui_arg(&strs(&["--dial", "0812345678"]), "--dial"),
            Some("0812345678".to_string())
        );
        assert_eq!(
            gui_arg(&strs(&["--dial=+66 81 234 5678"]), "--dial"),
            Some("+66 81 234 5678".to_string())
        );
        assert_eq!(gui_arg(&strs(&["--dial"]), "--dial"), None);
        assert_eq!(gui_arg(&strs(&["--dial="]), "--dial"), None);
        assert_eq!(gui_arg(&strs(&[]), "--dial"), None);
        // A different flag's value is not picked up.
        assert_eq!(gui_arg(&strs(&["--to", "081"]), "--dial"), None);
    }
}
