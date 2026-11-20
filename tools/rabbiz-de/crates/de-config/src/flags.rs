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

/// `--backend` value.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum BackendSelect {
    /// Hardware DRM/KMS + libseat + libinput (default).
    #[default]
    Drm,
    /// Headless / emulator virtual output with software renderer.
    Virtual,
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
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub struct Flags {
    pub backend: BackendSelect,
    pub virtual_size: (u32, u32),
    pub virtual_refresh_hz: f64,
    pub ctl_listen: Option<String>,
    pub emu_listen: Option<String>,
    pub headless: bool,
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
            backend: BackendSelect::Drm,
            virtual_size: (720, 1600),
            virtual_refresh_hz: 60.0,
            ctl_listen: None,
            emu_listen: None,
            headless: false,
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
            "--headless" => flags.headless = true,
            "--force-fail" => flags.force_fail = true,
            "--require=gl" => flags.require_gl = true,
            "--stats" => flags.stats = true,
            s if s.starts_with("--backend") => {
                flags.backend = match value(s, &mut it)?.as_str() {
                    "drm" => BackendSelect::Drm,
                    "virtual" => BackendSelect::Virtual,
                    other => return Err(format!("unknown --backend {other}")),
                };
            }
            s if s.starts_with("--size") => {
                flags.virtual_size = parse_virtual_size(&value(s, &mut it)?)?;
            }
            s if s.starts_with("--refresh") => {
                flags.virtual_refresh_hz = parse_virtual_refresh(&value(s, &mut it)?)?;
            }
            s if s.starts_with("--ctl-listen") => {
                flags.ctl_listen = Some(value(s, &mut it)?);
            }
            s if s.starts_with("--emu-listen") => {
                flags.emu_listen = Some(value(s, &mut it)?);
            }
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
    if flags.backend != BackendSelect::Virtual
        && (flags.ctl_listen.is_some() || flags.emu_listen.is_some())
    {
        return Err("--ctl-listen and --emu-listen require --backend=virtual".to_string());
    }
    Ok(Cli { command, flags })
}

fn parse_virtual_size(spec: &str) -> Result<(u32, u32), String> {
    let (w_str, h_str) = spec
        .split_once('x')
        .or_else(|| spec.split_once('X'))
        .ok_or_else(|| format!("--size expects WxH, got '{spec}'"))?;
    let w: u32 = w_str
        .parse()
        .map_err(|_| format!("invalid width in --size '{spec}'"))?;
    let h: u32 = h_str
        .parse()
        .map_err(|_| format!("invalid height in --size '{spec}'"))?;
    if w == 0 || h == 0 || w > 8192 || h > 8192 {
        return Err(format!("--size dimensions must be 1..=8192, got {w}x{h}"));
    }
    Ok((w, h))
}

fn parse_virtual_refresh(spec: &str) -> Result<f64, String> {
    let hz: f64 = spec
        .parse()
        .map_err(|_| format!("invalid --refresh '{spec}'"))?;
    if !hz.is_finite() || !(1.0..=240.0).contains(&hz) {
        return Err(format!("--refresh must be 1.0..=240.0, got '{spec}'"));
    }
    Ok(hz)
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
    use crate::ipc::{HwButtonKind, IpcCommand, OsdKind, PanelKind, TouchInjectPhase};

    let sub = it.next().ok_or(
        "ctl needs a subcommand: state|lock|launch|screenshot|reload-theme|reload-apps|panel|osd|touch|key|button|mock",
    )?;
    match sub.as_str() {
        "state" | "get_state" => Ok(IpcCommand::GetState),
        "lock" => Ok(IpcCommand::Lock),
        "launch" => {
            let desktop_id = it.next().ok_or("ctl launch needs a desktop-id")?;
            Ok(IpcCommand::Launch { desktop_id })
        }
        "screenshot" => {
            let save = match it.peek().map(String::as_str) {
                Some("--no-save") => {
                    it.next();
                    false
                }
                Some("--save") => {
                    it.next();
                    true
                }
                _ => true,
            };
            Ok(IpcCommand::Screenshot { save })
        }
        "reload-theme" | "reload_theme" => Ok(IpcCommand::ReloadTheme),
        "reload-apps" | "reload_apps" => Ok(IpcCommand::ReloadApps),
        "panel" => {
            let first = it
                .next()
                .ok_or("ctl panel needs notifications|quick and open|close")?;
            let second = it
                .next()
                .ok_or("ctl panel needs notifications|quick and open|close")?;
            let (panel, open) = match (first.as_str(), second.as_str()) {
                ("notifications", "open") | ("open", "notifications") => {
                    (PanelKind::Notifications, true)
                }
                ("notifications", "close") | ("close", "notifications") => {
                    (PanelKind::Notifications, false)
                }
                ("quick", "open") | ("open", "quick") => (PanelKind::Quick, true),
                ("quick", "close") | ("close", "quick") => (PanelKind::Quick, false),
                _ => {
                    return Err(format!(
                        "ctl panel expects notifications|quick and open|close, got '{first} {second}'"
                    ));
                }
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
                .ok()
                .filter(|&p: &u8| p <= 100)
                .ok_or("ctl osd percent must be 0..=100")?;
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
        "touch" => {
            let phase = match it
                .next()
                .ok_or("ctl touch needs down|move|up|cancel")?
                .as_str()
            {
                "down" => TouchInjectPhase::Down,
                "move" | "motion" => TouchInjectPhase::Move,
                "up" => TouchInjectPhase::Up,
                "cancel" => TouchInjectPhase::Cancel,
                other => return Err(format!("unknown touch phase '{other}'")),
            };
            let (x, y) = if matches!(phase, TouchInjectPhase::Up | TouchInjectPhase::Cancel)
                && it.peek().is_none()
            {
                (0, 0)
            } else {
                let x: i32 = it
                    .next()
                    .ok_or("ctl touch needs <x> <y>")?
                    .parse()
                    .map_err(|_| "ctl touch <x> must be an integer")?;
                let y: i32 = it
                    .next()
                    .ok_or("ctl touch needs <x> <y>")?
                    .parse()
                    .map_err(|_| "ctl touch <y> must be an integer")?;
                (x, y)
            };
            let slot: i32 = match it.next() {
                Some(s) => s
                    .parse()
                    .map_err(|_| "ctl touch [slot] must be an integer")?,
                None => 0,
            };
            Ok(IpcCommand::InjectTouch { phase, slot, x, y })
        }
        "key" => {
            let keycode: u32 = it
                .next()
                .ok_or("ctl key needs a keycode")?
                .parse()
                .map_err(|_| "ctl key keycode must be a u32")?;
            let pressed = match it.next().as_deref() {
                None | Some("press" | "down") => true,
                Some("release" | "up") => false,
                Some(other) => return Err(format!("unknown key state '{other}'")),
            };
            Ok(IpcCommand::InjectKey { keycode, pressed })
        }
        "button" => {
            let button = match it
                .next()
                .ok_or("ctl button needs power|vol-up|vol-down")?
                .as_str()
            {
                "power" => HwButtonKind::Power,
                "vol-up" | "vol_up" | "volume-up" | "volume_up" => HwButtonKind::VolumeUp,
                "vol-down" | "vol_down" | "volume-down" | "volume_down" => HwButtonKind::VolumeDown,
                other => return Err(format!("unknown button '{other}'")),
            };
            Ok(IpcCommand::InjectHwButton { button })
        }
        "mock" => {
            let key = it.next().ok_or("ctl mock needs <key> <value>")?;
            let value = match it.next() {
                Some(v) => v,
                None if key == "reset" => String::new(),
                None => return Err("ctl mock needs <key> <value>".to_string()),
            };
            Ok(IpcCommand::SetMock { key, value })
        }
        other => Err(format!("unknown ctl subcommand '{other}'")),
    }
}

fn help() -> String {
    "rabbiz-de [run|probe|env --shell|launch <id>|ctl <subcmd>] \
     [--backend=drm|virtual] [--size=WxH] [--refresh=HZ] [--headless] \
     [--ctl-listen=ADDR:PORT] [--emu-listen=ADDR:PORT] \
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
        assert_eq!(c.flags.backend, BackendSelect::Drm);
        assert_eq!(c.flags.virtual_size, (720, 1600));
        assert!((c.flags.virtual_refresh_hz - 60.0).abs() < f64::EPSILON);
        assert_eq!(c.flags.renderer, RendererSelect::Auto);
    }

    #[test]
    fn parse_virtual_backend_and_listeners() {
        let c = parse(argv(&[
            "--backend=virtual",
            "--size=1600x2560",
            "--refresh=120",
            "--headless",
            "--ctl-listen=127.0.0.1:4721",
            "--emu-listen=127.0.0.1:4722",
        ]))
        .unwrap();
        assert_eq!(c.flags.backend, BackendSelect::Virtual);
        assert_eq!(c.flags.virtual_size, (1600, 2560));
        assert!((c.flags.virtual_refresh_hz - 120.0).abs() < f64::EPSILON);
        assert!(c.flags.headless);
        assert_eq!(c.flags.ctl_listen.as_deref(), Some("127.0.0.1:4721"));
        assert_eq!(c.flags.emu_listen.as_deref(), Some("127.0.0.1:4722"));
    }

    #[test]
    fn rejects_tcp_listeners_without_virtual_backend() {
        let err = parse(argv(&["--ctl-listen=127.0.0.1:4721"])).unwrap_err();
        assert!(err.contains("require --backend=virtual"));

        let err2 = parse(argv(&["--backend=drm", "--emu-listen=127.0.0.1:4722"])).unwrap_err();
        assert!(err2.contains("require --backend=virtual"));

        assert!(parse(argv(&["--size=0x1600"])).is_err());
        assert!(parse(argv(&["--size=9000x1600"])).is_err());
        assert!(parse(argv(&["--refresh=0"])).is_err());
        assert!(parse(argv(&["--refresh=300"])).is_err());
    }

    #[test]
    fn parse_ctl_subcommands() {
        use crate::ipc::{HwButtonKind, IpcCommand, OsdKind, PanelKind, TouchInjectPhase};

        assert_eq!(
            parse(argv(&["ctl", "state"])).unwrap().command,
            Command::Ctl(IpcCommand::GetState)
        );
        assert_eq!(
            parse(argv(&["ctl", "reload-apps"])).unwrap().command,
            Command::Ctl(IpcCommand::ReloadApps)
        );
        assert_eq!(
            parse(argv(&["ctl", "touch", "down", "360", "800"]))
                .unwrap()
                .command,
            Command::Ctl(IpcCommand::InjectTouch {
                phase: TouchInjectPhase::Down,
                slot: 0,
                x: 360,
                y: 800,
            })
        );
        assert_eq!(
            parse(argv(&["ctl", "touch", "move", "360", "400", "2"]))
                .unwrap()
                .command,
            Command::Ctl(IpcCommand::InjectTouch {
                phase: TouchInjectPhase::Move,
                slot: 2,
                x: 360,
                y: 400,
            })
        );
        assert_eq!(
            parse(argv(&["ctl", "touch", "up", "360", "400"]))
                .unwrap()
                .command,
            Command::Ctl(IpcCommand::InjectTouch {
                phase: TouchInjectPhase::Up,
                slot: 0,
                x: 360,
                y: 400,
            })
        );
        assert_eq!(
            parse(argv(&["ctl", "key", "28"])).unwrap().command,
            Command::Ctl(IpcCommand::InjectKey {
                keycode: 28,
                pressed: true,
            })
        );
        assert_eq!(
            parse(argv(&["ctl", "key", "28", "release"]))
                .unwrap()
                .command,
            Command::Ctl(IpcCommand::InjectKey {
                keycode: 28,
                pressed: false,
            })
        );
        assert_eq!(
            parse(argv(&["ctl", "button", "power"])).unwrap().command,
            Command::Ctl(IpcCommand::InjectHwButton {
                button: HwButtonKind::Power,
            })
        );
        assert_eq!(
            parse(argv(&["ctl", "button", "vol-up"])).unwrap().command,
            Command::Ctl(IpcCommand::InjectHwButton {
                button: HwButtonKind::VolumeUp,
            })
        );
        assert_eq!(
            parse(argv(&["ctl", "button", "vol-down"])).unwrap().command,
            Command::Ctl(IpcCommand::InjectHwButton {
                button: HwButtonKind::VolumeDown,
            })
        );
        assert_eq!(
            parse(argv(&["ctl", "mock", "battery", "42"]))
                .unwrap()
                .command,
            Command::Ctl(IpcCommand::SetMock {
                key: "battery".into(),
                value: "42".into(),
            })
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
        assert_eq!(
            parse(argv(&["ctl", "screenshot"])).unwrap().command,
            Command::Ctl(IpcCommand::Screenshot { save: true })
        );
        assert_eq!(
            parse(argv(&["ctl", "screenshot", "--save"]))
                .unwrap()
                .command,
            Command::Ctl(IpcCommand::Screenshot { save: true })
        );
        assert_eq!(
            parse(argv(&["ctl", "panel", "open", "notifications"]))
                .unwrap()
                .command,
            Command::Ctl(IpcCommand::SetPanel {
                panel: PanelKind::Notifications,
                open: true,
            })
        );
        assert_eq!(
            parse(argv(&["ctl", "panel", "close", "quick"]))
                .unwrap()
                .command,
            Command::Ctl(IpcCommand::SetPanel {
                panel: PanelKind::Quick,
                open: false,
            })
        );
        assert_eq!(
            parse(argv(&["ctl", "osd", "brightness", "45"]))
                .unwrap()
                .command,
            Command::Ctl(IpcCommand::ShowOsd {
                kind: OsdKind::Brightness,
                percent: 45,
                muted: false,
            })
        );
    }

    #[test]
    fn parse_ctl_rejects_invalid_args() {
        assert!(parse(argv(&["ctl", "osd", "volume", "101"])).is_err());
        assert!(parse(argv(&["ctl", "osd", "unknown", "50"])).is_err());
        assert!(parse(argv(&["ctl", "panel", "quick"])).is_err());
        assert!(parse(argv(&["ctl", "panel", "drawer", "open"])).is_err());
        assert!(parse(argv(&["ctl", "touch", "hover", "10", "20"])).is_err());
        assert!(parse(argv(&["ctl", "button", "camera"])).is_err());
        assert!(parse(argv(&["ctl", "key", "28", "hold"])).is_err());
    }
}
