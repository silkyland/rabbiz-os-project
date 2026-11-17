use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use de_apps::{load_desktop_dir, parse_desktop_file, DesktopEntry, LaunchPolicy};
use de_backend::probe::run_probe;
use de_core::env::format_shell_env;
use de_core::{run, RendererChoice, RunConfig};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(subcmd) = args.first().map(String::as_str) else {
        print_usage();
        std::process::exit(2);
    };

    let code = match subcmd {
        "probe" => cmd_probe(&args[1..]),
        "run" => cmd_run(&args[1..]),
        "env" => cmd_env(&args[1..]),
        "launch" => cmd_launch(&args[1..]),
        "-h" | "--help" => {
            print_usage();
            0
        }
        other => {
            eprintln!("unknown subcommand: {other}");
            print_usage();
            2
        }
    };

    std::process::exit(code);
}

fn print_usage() {
    eprintln!(
        "Usage:\n  rabbiz-de probe --json [--require=gl]\n  rabbiz-de run [--renderer=auto|gl|soft] [--require=gl] [--stats] [--stats-file <path>] [--stats-summary <path>] [--scenario <name>] [--duration <sec>] [--spawn <cmd>] [--screenshot <png>] [--screenshot-at <sec>] [--force-fail]\n  rabbiz-de env --shell\n  rabbiz-de launch <desktop-id>"
    );
}

fn cmd_probe(args: &[String]) -> i32 {
    let mut json = false;
    let mut require_gl = false;
    for arg in args {
        match arg.as_str() {
            "--json" => json = true,
            "--require=gl" => require_gl = true,
            other => {
                eprintln!("unknown probe flag: {other}");
                return 2;
            }
        }
    }
    let (report, exit_code) = run_probe(require_gl);
    if json {
        match serde_json::to_string(&report) {
            Ok(s) => println!("{s}"),
            Err(err) => {
                eprintln!("serialize probe report failed: {err}");
                return 1;
            }
        }
    } else {
        match serde_json::to_string_pretty(&report) {
            Ok(s) => println!("{s}"),
            Err(err) => {
                eprintln!("serialize probe report failed: {err}");
                return 1;
            }
        }
    }
    exit_code
}

fn cmd_run(args: &[String]) -> i32 {
    let mut cfg = RunConfig::default();
    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_str();
        if let Some(val) = arg.strip_prefix("--renderer=") {
            cfg.renderer = match parse_renderer(val) {
                Some(r) => r,
                None => {
                    eprintln!("invalid --renderer value: {val}");
                    return 2;
                }
            };
        } else if arg == "--renderer" {
            i += 1;
            let Some(val) = args.get(i) else {
                eprintln!("--renderer requires a value");
                return 2;
            };
            cfg.renderer = match parse_renderer(val) {
                Some(r) => r,
                None => {
                    eprintln!("invalid --renderer value: {val}");
                    return 2;
                }
            };
        } else if arg == "--require=gl" {
            cfg.require_gl = true;
        } else if arg == "--stats" {
            cfg.stats = true;
        } else if arg == "--force-fail" {
            cfg.force_fail = true;
        } else if arg == "--stats-file" {
            i += 1;
            let Some(val) = args.get(i) else {
                eprintln!("--stats-file requires a path");
                return 2;
            };
            cfg.stats_file = Some(PathBuf::from(val));
        } else if let Some(val) = arg.strip_prefix("--stats-file=") {
            cfg.stats_file = Some(PathBuf::from(val));
        } else if arg == "--stats-summary" {
            i += 1;
            let Some(val) = args.get(i) else {
                eprintln!("--stats-summary requires a path");
                return 2;
            };
            cfg.stats_summary = Some(PathBuf::from(val));
        } else if let Some(val) = arg.strip_prefix("--stats-summary=") {
            cfg.stats_summary = Some(PathBuf::from(val));
        } else if arg == "--scenario" {
            i += 1;
            let Some(val) = args.get(i) else {
                eprintln!("--scenario requires a name");
                return 2;
            };
            cfg.scenario = val.clone();
        } else if let Some(val) = arg.strip_prefix("--scenario=") {
            cfg.scenario = val.to_string();
        } else if arg == "--screenshot" {
            i += 1;
            let Some(val) = args.get(i) else {
                eprintln!("--screenshot requires a path");
                return 2;
            };
            cfg.screenshot = Some(PathBuf::from(val));
        } else if arg == "--screenshot-at" {
            i += 1;
            match args.get(i).and_then(|v| v.parse::<f64>().ok()) {
                Some(secs) if secs >= 0.0 => cfg.screenshot_at = secs,
                _ => {
                    eprintln!("--screenshot-at requires seconds");
                    return 2;
                }
            }
        } else if arg == "--duration" {
            i += 1;
            let Some(val) = args.get(i) else {
                eprintln!("--duration requires seconds");
                return 2;
            };
            match val.parse::<f64>() {
                Ok(secs) if secs >= 0.0 => cfg.duration = Some(secs),
                _ => {
                    eprintln!("invalid --duration value: {val}");
                    return 2;
                }
            }
        } else if let Some(val) = arg.strip_prefix("--duration=") {
            match val.parse::<f64>() {
                Ok(secs) if secs >= 0.0 => cfg.duration = Some(secs),
                _ => {
                    eprintln!("invalid --duration value: {val}");
                    return 2;
                }
            }
        } else if arg == "--spawn" {
            i += 1;
            let Some(val) = args.get(i) else {
                eprintln!("--spawn requires a command");
                return 2;
            };
            cfg.spawn = Some(val.clone());
        } else if let Some(val) = arg.strip_prefix("--spawn=") {
            cfg.spawn = Some(val.to_string());
        } else {
            eprintln!("unknown run flag: {arg}");
            return 2;
        }
        i += 1;
    }

    run(cfg)
}

fn parse_renderer(val: &str) -> Option<RendererChoice> {
    match val {
        "auto" => Some(RendererChoice::Auto),
        "gl" => Some(RendererChoice::Gl),
        "soft" => Some(RendererChoice::Soft),
        _ => None,
    }
}

fn cmd_env(args: &[String]) -> i32 {
    if args.iter().any(|a| a == "--shell") {
        print!("{}", format_shell_env());
        0
    } else {
        eprintln!("env requires --shell");
        2
    }
}

fn cmd_launch(args: &[String]) -> i32 {
    let Some(desktop_id) = args.first() else {
        eprintln!("launch requires <desktop-id>");
        return 2;
    };

    let Some(entry) = find_desktop_entry(desktop_id) else {
        eprintln!("desktop entry not found: {desktop_id}");
        return 1;
    };

    let policy = match LaunchPolicy::embedded() {
        Ok(p) => p,
        Err(err) => {
            eprintln!("load embedded launch policy failed: {err}");
            return 1;
        }
    };

    let spec = policy.resolve(&entry);
    if spec.program.is_empty() {
        eprintln!("empty Exec program for {desktop_id}");
        return 1;
    }

    let mut cmd = Command::new(&spec.program);
    cmd.args(&spec.args)
        .envs(&spec.env)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    unsafe {
        cmd.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }

    match cmd.spawn() {
        Ok(_child) => 0,
        Err(err) => {
            eprintln!("spawn {} failed: {err}", spec.program);
            1
        }
    }
}

fn find_desktop_entry(desktop_id: &str) -> Option<DesktopEntry> {
    let trimmed = desktop_id.strip_suffix(".desktop").unwrap_or(desktop_id);
    let as_path = Path::new(desktop_id);
    if as_path.is_file() {
        return parse_desktop_file(as_path).ok().flatten();
    }

    let mut dirs = Vec::new();
    if let Ok(home) = std::env::var("XDG_DATA_HOME") {
        dirs.push(PathBuf::from(home).join("applications"));
    } else if let Ok(home) = std::env::var("HOME") {
        dirs.push(PathBuf::from(home).join(".local/share/applications"));
    }
    dirs.push(PathBuf::from("/usr/local/share/applications"));
    dirs.push(PathBuf::from("/usr/share/applications"));

    for dir in dirs {
        for entry in load_desktop_dir(&dir) {
            if entry.id == trimmed || entry.id == desktop_id {
                return Some(entry);
            }
        }
    }
    None
}
