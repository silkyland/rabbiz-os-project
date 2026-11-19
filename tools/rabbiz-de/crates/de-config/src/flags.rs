//! CLI surface for `rabbiz-de` (M0-T4 + §7.20/7.21 contract).

use std::path::PathBuf;

/// `--renderer` value.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum RendererSelect {
    /// GL if EGL/GBM works and CMA >= 96 MiB, else soft.
    #[default]
    Auto,
    Gl,
    Soft,
}

/// Subcommand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// `run` (default): compositor main loop.
    Run,
    /// `probe`: spike S1 JSON report.
    Probe,
    /// `env --shell`: print the app env contract.
    Env,
    /// `launch <desktop-id>`: launch helper (M1 path, stub in M0).
    Launch(String),
    /// `ctl <subcommand>`: send a runtime control request over the IPC socket.
    Ctl(crate::ipc::IpcCommand),
}

/// Parsed invocation.
#[derive(Debug, Clone)]
pub struct Cli {
    pub command: Command,
    pub flags: Flags,
}

/// Flags accepted by `rabbiz-de` (see plan M0-T4).
#[derive(Debug, Clone)]
pub struct Flags {
    pub renderer: RendererSelect,
    /// Fail when GL cannot initialize.
    pub require_gl: bool,
    /// Print a final JSON summary to stdout.
    pub stats: bool,
    /// Per-second JSONL destination.
    pub stats_file: Option<PathBuf>,
    /// Final summary JSON destination.
    pub stats_summary: Option<PathBuf>,
    /// Scenario label recorded in the summary.
    pub scenario: String,
    /// Stop after N seconds and print the summary.
    pub duration: Option<u64>,
    /// Exit 75 immediately (crash-loop test).
    pub force_fail: bool,
}

impl Default for Flags {
    fn default() -> Self {
        Self {
            renderer: RendererSelect::Auto,
            require_gl: false,
            stats: false,
            stats_file: None,
            stats_summary: None,
            scenario: "default".to_string(),
            duration: None,
            force_fail: false,
        }
    }
}

/// Parse `argv[1..]`.
pub fn parse<I: Iterator<Item = String>>(args: I) -> Result<Cli, String> {
    let mut flags = Flags::default();
    let mut command = Command::Run;
    let mut it = args.peekable();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "run" => command = Command::Run,
            "probe" => command = Command::Probe,
            "env" => command = Command::Env,
            "launch" => {
                let id = it.next().ok_or("launch needs a desktop-id")?;
                command = Command::Launch(id);
            }
            "ctl" => {
                command = Command::Ctl(parse_ctl(&mut it)?);
            }
            // `--shell` is only meaningful with `env`; `--json` matches the probe
            // contract — both accepted silently.
            "--shell" | "--json" => {}
            "--force-fail" => flags.force_fail = true,
            "--require=gl" => flags.require_gl = true,
            "--stats" => flags.stats = true,
            s if s.starts_with("--stats-file") => {
                flags.stats_file = Some(PathBuf::from(value(s, &mut it)?));
            }
            s if s.starts_with("--stats-summary") => {
                flags.stats_summary = Some(PathBuf::from(value(s, &mut it)?));
            }
            s if s.starts_with("--scenario") => flags.scenario = value(s, &mut it)?,
            s if s.starts_with("--duration") => {
                flags.duration = Some(value(s, &mut it)?.parse().map_err(|_| "--duration N")?);
            }
            s if s.starts_with("--renderer") => {
                flags.renderer = match value(s, &mut it)?.as_str() {
                    "auto" => RendererSelect::Auto,
                    "gl" => RendererSelect::Gl,
                    "soft" => RendererSelect::Soft,
                    other => return Err(format!("unknown --renderer {other}")),
                };
            }
            "--help" | "-h" => return Err(help()),
            other => return Err(format!("unknown argument '{other}'\n{}", help())),
        }
    }
    Ok(Cli { command, flags })
}

fn value<I: Iterator<Item = String>>(flag: &str, it: &mut I) -> Result<String, String> {
    if let Some((_, v)) = flag.split_once('=') {
        return Ok(v.to_string());
    }
    it.next().ok_or_else(|| format!("{flag} needs a value"))
}

fn parse_ctl<I: Iterator<Item = String>>(
    it: &mut std::iter::Peekable<I>,
) -> Result<crate::ipc::IpcCommand, String> {
    use crate::ipc::{IpcCommand, OsdKind, PanelKind};

    let sub = it
        .next()
        .ok_or("ctl needs a subcommand: state|lock|launch|screenshot|reload-theme|panel|osd")?;
    match sub.as_str() {
        "state" | "get_state" => Ok(IpcCommand::GetState),
        "lock" => Ok(IpcCommand::Lock),
        "launch" => {
            let desktop_id = it.next().ok_or("ctl launch needs a desktop-id")?;
            Ok(IpcCommand::Launch { desktop_id })
        }
        "screenshot" => {
            let save = if it.peek().is_some_and(|s| s == "--no-save") {
                it.next();
                false
            } else {
                true
            };
            Ok(IpcCommand::Screenshot { save })
        }
        "reload-theme" | "reload_theme" => Ok(IpcCommand::ReloadTheme),
        "panel" => {
            let panel = match it
                .next()
                .ok_or("ctl panel needs notifications|quick")?
                .as_str()
            {
                "notifications" => PanelKind::Notifications,
                "quick" => PanelKind::Quick,
                other => return Err(format!("unknown panel '{other}'")),
            };
            let open = match it.next().ok_or("ctl panel needs open|close")?.as_str() {
                "open" => true,
                "close" => false,
                other => return Err(format!("expected open|close, got '{other}'")),
            };
            Ok(IpcCommand::SetPanel { panel, open })
        }
        "osd" => {
            let kind = match it.next().ok_or("ctl osd needs volume|brightness")?.as_str() {
                "volume" => OsdKind::Volume,
                "brightness" => OsdKind::Brightness,
                other => return Err(format!("unknown osd kind '{other}'")),
            };
            let percent: u8 = it
                .next()
                .ok_or("ctl osd needs a percent (0..=100)")?
                .parse()
                .map_err(|_| "ctl osd percent must be 0..=255")?;
            let muted = if it.peek().is_some_and(|s| s == "--muted") {
                it.next();
                true
            } else {
                false
            };
            Ok(IpcCommand::ShowOsd {
                kind,
                percent,
                muted,
            })
        }
        other => Err(format!("unknown ctl subcommand '{other}'")),
    }
}

fn help() -> String {
    "rabbiz-de [run|probe|env --shell|launch <id>|ctl <subcmd>] \
     [--renderer=auto|gl|soft] [--require=gl] [--stats] [--stats-file F] \
     [--stats-summary F] [--scenario N] [--duration S] [--force-fail]"
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(v: &[&str]) -> impl Iterator<Item = String> {
        v.iter()
            .map(|s| (*s).to_string())
            .collect::<Vec<_>>()
            .into_iter()
    }

    #[test]
    fn parse_probe_and_flags() {
        let c = parse(argv(&[
            "probe",
            "--renderer=soft",
            "--stats-file",
            "/tmp/x.jsonl",
            "--duration=5",
        ]))
        .unwrap();
        assert_eq!(c.command, Command::Probe);
        assert_eq!(c.flags.renderer, RendererSelect::Soft);
        assert_eq!(c.flags.duration, Some(5));
        assert!(c.flags.stats_file.is_some());
    }

    #[test]
    fn defaults_to_run_auto() {
        let c = parse(argv(&[])).unwrap();
        assert_eq!(c.command, Command::Run);
        assert_eq!(c.flags.renderer, RendererSelect::Auto);
    }

    #[test]
    fn parse_ctl_subcommands() {
        use crate::ipc::{IpcCommand, OsdKind, PanelKind};

        assert_eq!(
            parse(argv(&["ctl", "state"])).unwrap().command,
            Command::Ctl(IpcCommand::GetState)
        );
        assert_eq!(
            parse(argv(&["ctl", "panel", "quick", "open"]))
                .unwrap()
                .command,
            Command::Ctl(IpcCommand::SetPanel {
                panel: PanelKind::Quick,
                open: true,
            })
        );
        assert_eq!(
            parse(argv(&["ctl", "osd", "volume", "70", "--muted"]))
                .unwrap()
                .command,
            Command::Ctl(IpcCommand::ShowOsd {
                kind: OsdKind::Volume,
                percent: 70,
                muted: true,
            })
        );
        assert_eq!(
            parse(argv(&["ctl", "screenshot", "--no-save"]))
                .unwrap()
                .command,
            Command::Ctl(IpcCommand::Screenshot { save: false })
        );
    }
}
