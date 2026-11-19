//! Headless modes for the same `rabbiz-music` binary:
//!
//! - `rabbiz-music --cli <cmd> …` — one-shot text CLI (list/search/play/
//!   pause/next/prev/seek/status/stop).
//! - `rabbiz-music --mcp` — MCP server over stdio (`rabbiz_app_kit::mcp`).
//!
//! Both reuse `library::scan` and the exact `Player`/mpv-IPC path the GUI
//! uses — there is no second playback implementation. Playback commands
//! attach to the already-running mpv when one exists (GUI or an earlier
//! headless `play`, which leaves mpv running via `Mpv::detach`); they
//! never spawn a player just to pause it, and they report "no session"
//! honestly instead of faking state.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rabbiz_app_kit::mcp::{serve, Tool};
use serde_json::Value;

use crate::library::{self, Scope, Track};
use crate::player;
use crate::Player;

const VERSION: &str = "0.1.0";

pub const HELP: &str = "\
rabbiz-music — Music for RabbizOS

usage:
  rabbiz-music                      open the GUI
  rabbiz-music --cli <cmd> [args]   headless text CLI
  rabbiz-music --mcp                MCP server on stdio

commands:
  list [--scope music|downloads|all]   list library tracks (newest first)
  search <query>                       find tracks by title/path/folder
  play [index|path]                    play a track; bare `play` toggles
                                       play/pause like the GUI transport
  pause                                pause the current track
  next | prev                          next/previous library track
  seek <seconds>                       absolute seek position
  status                               now playing + position
  stop                                 stop playback, forget the track
  help                                 this text

track indices are the numbers shown by `list`/`search` (position in the
full newest-first scan, across all scopes). `play <path>` also accepts any
audio file outside ~/Music and ~/Downloads.
";

// ---------------------------------------------------------------------------
// CLI parsing (pure — unit-tested)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum Cli {
    List {
        scope: Scope,
    },
    Search {
        query: String,
    },
    /// `None` = bare `play` → GUI-style play/pause toggle.
    Play {
        target: Option<String>,
    },
    Pause,
    Next,
    Prev,
    Seek {
        seconds: f64,
    },
    Status,
    Stop,
    Help,
}

/// "music" | "downloads" | "all" (case-insensitive) → [`Scope`].
#[must_use]
pub fn parse_scope(s: &str) -> Option<Scope> {
    match s.to_ascii_lowercase().as_str() {
        "all" => Some(Scope::All),
        "music" => Some(Scope::Music),
        "downloads" => Some(Scope::Downloads),
        _ => None,
    }
}

/// Extract the tokens after `--cli` / `--cli=…` from argv (program name
/// already skipped). `None` when no `--cli` flag is present.
#[must_use]
pub fn cli_tokens(args: &[String]) -> Option<Vec<String>> {
    for (i, a) in args.iter().enumerate() {
        if a == "--cli" {
            return Some(args[i + 1..].to_vec());
        }
        if let Some(rest) = a.strip_prefix("--cli=") {
            let mut v = vec![rest.to_string()];
            v.extend(args[i + 1..].iter().cloned());
            return Some(v);
        }
    }
    None
}

fn no_args(rest: &[String], cli: Cli) -> Result<Cli, String> {
    if rest.is_empty() {
        Ok(cli)
    } else {
        Err(format!("unexpected argument '{}'", rest[0]))
    }
}

/// Parse CLI tokens (everything after `--cli`). Empty → [`Cli::Help`].
pub fn parse_cli(tokens: &[String]) -> Result<Cli, String> {
    let Some((cmd, rest)) = tokens.split_first() else {
        return Ok(Cli::Help);
    };
    match cmd.as_str() {
        "help" | "--help" | "-h" => no_args(rest, Cli::Help),
        "list" => {
            let mut scope = None;
            let mut i = 0;
            while i < rest.len() {
                let a = rest[i].as_str();
                if a == "--scope" {
                    i += 1;
                    let v = rest.get(i).ok_or("--scope needs a value")?;
                    scope = Some(parse_scope(v).ok_or_else(|| format!("unknown scope '{v}'"))?);
                } else if let Some(v) = a.strip_prefix("--scope=") {
                    scope = Some(parse_scope(v).ok_or_else(|| format!("unknown scope '{v}'"))?);
                } else {
                    scope = Some(parse_scope(a).ok_or_else(|| format!("unknown scope '{a}'"))?);
                }
                i += 1;
            }
            Ok(Cli::List {
                scope: scope.unwrap_or(Scope::All),
            })
        }
        "search" => {
            if rest.is_empty() {
                Err("search needs a query".into())
            } else {
                Ok(Cli::Search {
                    query: rest.join(" "),
                })
            }
        }
        "play" => Ok(Cli::Play {
            target: if rest.is_empty() {
                None
            } else {
                Some(rest.join(" "))
            },
        }),
        "pause" => no_args(rest, Cli::Pause),
        "next" => no_args(rest, Cli::Next),
        "prev" => no_args(rest, Cli::Prev),
        "stop" => no_args(rest, Cli::Stop),
        "status" => no_args(rest, Cli::Status),
        "seek" => {
            if rest.len() != 1 {
                return Err("seek needs exactly one seconds value".into());
            }
            let secs: f64 = rest[0]
                .parse()
                .map_err(|_| format!("'{}' is not a number of seconds", rest[0]))?;
            if !secs.is_finite() || secs < 0.0 {
                return Err("seek seconds must be a non-negative number".into());
            }
            Ok(Cli::Seek { seconds: secs })
        }
        other => Err(format!("unknown command '{other}'")),
    }
}

// ---------------------------------------------------------------------------
// Library output (shared by CLI and MCP)
// ---------------------------------------------------------------------------

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/home/rabbiz"))
}

/// `index ⇥ title ⇥ caption ⇥ path` — the index is the position in the
/// full newest-first scan so `play <index>` works under any scope.
#[must_use]
pub fn format_track_line(i: usize, t: &Track) -> String {
    format!("{i}\t{}\t{}\t{}", t.title, t.caption(), t.path.display())
}

/// Indices of tracks matching `query` (case-insensitive substring over
/// title, path, subfolder and extension), in library order.
#[must_use]
pub fn search_tracks(tracks: &[Track], query: &str) -> Vec<usize> {
    let q = query.to_lowercase();
    if q.trim().is_empty() {
        return Vec::new();
    }
    tracks
        .iter()
        .enumerate()
        .filter(|(_, t)| {
            t.title.to_lowercase().contains(&q)
                || t.path.to_string_lossy().to_lowercase().contains(&q)
                || t.subfolder.to_lowercase().contains(&q)
                || t.ext.to_lowercase().contains(&q)
        })
        .map(|(i, _)| i)
        .collect()
}

fn list_output(scope: Scope, limit: Option<usize>) -> Result<String, String> {
    let tracks = library::scan(&home()).tracks;
    let vis = library::visible_indices(&tracks, scope);
    let mut out = String::new();
    out.push_str(&format!("{} track(s) in {}\n", vis.len(), scope.label()));
    for i in vis.into_iter().take(limit.unwrap_or(usize::MAX)) {
        out.push_str(&format_track_line(i, &tracks[i]));
        out.push('\n');
    }
    Ok(out.trim_end().to_string())
}

fn search_output(query: &str, limit: Option<usize>) -> Result<String, String> {
    if query.trim().is_empty() {
        return Err("empty search query".into());
    }
    let tracks = library::scan(&home()).tracks;
    let hits = search_tracks(&tracks, query);
    let mut out = String::new();
    out.push_str(&format!("{} match(es) for \"{query}\"\n", hits.len()));
    for i in hits.into_iter().take(limit.unwrap_or(usize::MAX)) {
        out.push_str(&format_track_line(i, &tracks[i]));
        out.push('\n');
    }
    Ok(out.trim_end().to_string())
}

// ---------------------------------------------------------------------------
// Playback session (shared by CLI and MCP)
// ---------------------------------------------------------------------------

/// Pump the player for `ms`, sleeping between iterations so IPC events
/// actually arrive. Returns whether visible state changed.
fn pump_for(p: &mut Player, tracks: &[Track], ms: u64) -> bool {
    let deadline = Instant::now() + Duration::from_millis(ms);
    let mut changed = false;
    while Instant::now() < deadline {
        changed |= p.pump(tracks);
        std::thread::sleep(Duration::from_millis(20));
    }
    changed
}

/// Index of the track the attached player has loaded: our own `current`
/// first, then a path match against the fresh scan (covers sessions
/// started by the GUI or an earlier CLI call).
fn current_index(p: &Player, tracks: &[Track]) -> Option<usize> {
    if let Some(i) = p.current {
        return Some(i);
    }
    if p.file_path.is_empty() {
        return None;
    }
    tracks
        .iter()
        .position(|t| t.path.to_string_lossy() == p.file_path)
}

fn state_word(p: &Player) -> &'static str {
    if p.is_playing() {
        "playing"
    } else if p.eof {
        "finished"
    } else if p.has_track() {
        "paused"
    } else {
        "idle"
    }
}

/// One-line summary for transport commands: `playing: Title (1:02 / 3:45)`.
fn summary_line(p: &Player, tracks: &[Track]) -> String {
    let title = p.display_title(tracks);
    let title = if title.is_empty() {
        Path::new(&p.file_path)
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "—".into())
    } else {
        title.to_string()
    };
    let dur = if p.duration > 0.0 {
        library::format_time(p.duration)
    } else {
        "--:--".to_string()
    };
    format!(
        "{}: {} ({} / {})",
        state_word(p),
        title,
        library::format_time(p.est_pos()),
        dur
    )
}

/// Multi-line `status` output. Honest about missing mpv / missing session.
fn status_text(p: &Player, tracks: &[Track]) -> String {
    let title = p.display_title(tracks);
    let title = if title.is_empty() {
        Path::new(&p.file_path)
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "—".into())
    } else {
        title.to_string()
    };
    let dur = if p.duration > 0.0 {
        library::format_time(p.duration)
    } else {
        "--:--".to_string()
    };
    let path = if p.file_path.is_empty() {
        "—".to_string()
    } else {
        p.file_path.clone()
    };
    format!(
        "state: {}\ntitle: {}\nposition: {} / {}\npath: {}",
        state_word(p),
        title,
        library::format_time(p.est_pos()),
        dur,
        path
    )
}

/// `status` without requiring a session: reports idle honestly when no
/// mpv is running, and the mpv-missing error when there is no player at all.
fn status_report() -> String {
    if !player::mpv_available() {
        return "state: unknown\nerror: mpv is not installed — nothing can play \
                (install mpv, e.g. pacman -S mpv)"
            .to_string();
    }
    let tracks = library::scan(&home()).tracks;
    let mut p = Player::new(true);
    if !p.attach() {
        return format!(
            "state: idle\nplayer: not running (no mpv session at {})\n\
             start playback with `rabbiz-music --cli play …` or the GUI",
            player::socket_path().display()
        );
    }
    pump_for(&mut p, &tracks, 500);
    status_text(&p, &tracks)
}

enum PlayTarget {
    /// Position in the scanned `tracks` vec.
    Index(usize),
    /// Audio file outside the library roots — appended as a Track.
    File(PathBuf),
}

/// Resolve `play`'s argument: a decimal index into the scan, `~`-aware
/// path, or a path relative to the cwd. Paths must exist and be audio.
fn resolve_target(tracks: &[Track], target: &str, home: &Path) -> Result<PlayTarget, String> {
    let t = target.trim();
    if let Ok(i) = t.parse::<usize>() {
        return tracks
            .get(i)
            .map(|_| PlayTarget::Index(i))
            .ok_or_else(|| format!("no track at index {i} (library has {})", tracks.len()));
    }
    let mut p = PathBuf::from(t);
    if p == Path::new("~") {
        p = home.to_path_buf();
    } else if let Ok(rest) = p.strip_prefix("~/") {
        p = home.join(rest);
    }
    if p.is_relative() {
        if let Ok(cwd) = std::env::current_dir() {
            p = cwd.join(p);
        }
    }
    if let Some(i) = tracks.iter().position(|tr| tr.path == p) {
        return Ok(PlayTarget::Index(i));
    }
    if !p.is_file() {
        return Err(format!("'{}' is not an existing file", t));
    }
    if !library::is_audio(&p) {
        return Err(format!(
            "'{}' is not an audio file ({})",
            t,
            library::ext_of(&p).unwrap_or_else(|| "?".into())
        ));
    }
    Ok(PlayTarget::File(p))
}

/// `play [target]`: attach to a live player or spawn+detach one, load the
/// track, pump until mpv reports state, return a one-line summary. The
/// mpv process is deliberately left running (`Mpv::detach`) so playback
/// continues after this one-shot process exits.
fn do_play(target: Option<&str>) -> Result<String, String> {
    if !player::mpv_available() {
        return Err(
            "mpv is not installed — playback unavailable (install mpv, e.g. pacman -S mpv)".into(),
        );
    }
    let home = home();
    let mut tracks = library::scan(&home).tracks;
    let mut p = Player::new(true);

    match target {
        Some(t) => {
            let tgt = resolve_target(&tracks, t, &home)?;
            let idx = match tgt {
                PlayTarget::Index(i) => i,
                PlayTarget::File(f) => {
                    tracks.push(library::track_for_path(&f, &home));
                    tracks.len() - 1
                }
            };
            p.play_track(idx, &tracks); // ensure_started: attach-or-spawn
        }
        None => {
            // Bare `play`: GUI transport semantics — toggle when something
            // is loaded in the live session, otherwise start track #0.
            p.attach();
            if p.mpv.is_some() {
                pump_for(&mut p, &tracks, 400);
            }
            if p.has_track() {
                p.toggle(&tracks);
            } else {
                if tracks.is_empty() {
                    return Err("library is empty — no audio in ~/Music or ~/Downloads".into());
                }
                p.play_track(0, &tracks);
            }
        }
    }

    pump_for(&mut p, &tracks, 700);
    if p.mpv.is_none() {
        return Err(p
            .error
            .clone()
            .unwrap_or_else(|| "could not start playback".into()));
    }
    let msg = summary_line(&p, &tracks);
    // Leave mpv running: the CLI is one-shot, playback must outlive it.
    if let Some(m) = p.mpv.take() {
        m.detach();
    }
    Ok(msg)
}

/// Transport commands that only make sense against a live session — they
/// attach, never spawn. `Status` is handled separately by [`status_report`].
fn do_control(cli: &Cli) -> Result<String, String> {
    if !player::mpv_available() {
        return Err("mpv is not installed — playback unavailable".into());
    }
    let tracks = library::scan(&home()).tracks;
    let mut p = Player::new(true);
    if !p.attach() {
        return Err(format!(
            "no playback session running (socket {}) — start playback with \
             `rabbiz-music --cli play …` or the GUI",
            player::socket_path().display()
        ));
    }
    pump_for(&mut p, &tracks, 500);

    match cli {
        Cli::Pause => {
            if !p.has_track() {
                return Err("nothing is loaded — play a track first".into());
            }
            p.set_paused(true);
        }
        Cli::Next | Cli::Prev => {
            if tracks.is_empty() {
                return Err("library is empty".into());
            }
            let cur = current_index(&p, &tracks);
            let next = if matches!(cli, Cli::Next) {
                library::next_index(cur, tracks.len())
            } else {
                library::prev_index(cur, tracks.len())
            };
            match next {
                Some(i) => p.play_track(i, &tracks),
                None => return Err("library is empty".into()),
            }
        }
        Cli::Seek { seconds } => {
            if !p.has_track() {
                return Err("nothing is loaded — play a track first".into());
            }
            p.seek_absolute(*seconds);
        }
        Cli::Stop => p.stop(),
        _ => return Err("internal error: not a control command".into()),
    }

    pump_for(&mut p, &tracks, 250);
    let msg = if matches!(cli, Cli::Stop) {
        "stopped".to_string()
    } else {
        summary_line(&p, &tracks)
    };
    // Client-only handle: dropping it never kills the shared mpv.
    Ok(msg)
}

// ---------------------------------------------------------------------------
// Entry points
// ---------------------------------------------------------------------------

/// Run a parsed CLI command; prints to stdout, returns the exit code.
pub fn run_cli(cli: &Cli) -> i32 {
    let result = match cli {
        Cli::Help => {
            println!("{HELP}");
            return 0;
        }
        Cli::List { scope } => list_output(*scope, None),
        Cli::Search { query } => search_output(query, None),
        Cli::Play { target } => do_play(target.as_deref()),
        Cli::Status => {
            println!("{}", status_report());
            return 0;
        }
        other => do_control(other),
    };
    match result {
        Ok(msg) => {
            println!("{msg}");
            0
        }
        Err(e) => {
            eprintln!("rabbiz-music: {e}");
            1
        }
    }
}

// ---------------------------------------------------------------------------
// MCP tools
// ---------------------------------------------------------------------------

fn mcp_library_list(args: &Value) -> Result<String, String> {
    let scope = parse_scope(args.get("scope").and_then(Value::as_str).unwrap_or("all"))
        .ok_or("scope must be all|music|downloads")?;
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .map(|n| n as usize);
    list_output(scope, limit)
}

fn mcp_library_search(args: &Value) -> Result<String, String> {
    let query = args
        .get("query")
        .and_then(Value::as_str)
        .ok_or("missing 'query'")?;
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .map(|n| n as usize);
    search_output(query, limit)
}

fn mcp_playback_status(_args: &Value) -> Result<String, String> {
    Ok(status_report())
}

fn mcp_playback_control(args: &Value) -> Result<String, String> {
    let action = args
        .get("action")
        .and_then(Value::as_str)
        .ok_or("missing 'action'")?;
    match action {
        "play" => do_play(args.get("track").and_then(Value::as_str)),
        "pause" => do_control(&Cli::Pause),
        "next" => do_control(&Cli::Next),
        "prev" => do_control(&Cli::Prev),
        "stop" => do_control(&Cli::Stop),
        "seek" => {
            let secs = args
                .get("seconds")
                .and_then(Value::as_f64)
                .ok_or("seek needs a numeric 'seconds'")?;
            if !secs.is_finite() || secs < 0.0 {
                return Err("seconds must be a non-negative number".into());
            }
            do_control(&Cli::Seek { seconds: secs })
        }
        other => Err(format!("unknown action '{other}'")),
    }
}

static TOOLS: &[Tool] = &[
    Tool {
        name: "library_list",
        description: "List music-library tracks (newest first). Optional scope: all|music|downloads; optional limit.",
        input_schema: r#"{"type":"object","properties":{"scope":{"type":"string","enum":["all","music","downloads"]},"limit":{"type":"integer"}}}"#,
        handler: mcp_library_list,
    },
    Tool {
        name: "library_search",
        description: "Search the music library by title, path or folder. Returns index<TAB>title<TAB>caption<TAB>path lines.",
        input_schema: r#"{"type":"object","properties":{"query":{"type":"string"},"limit":{"type":"integer"}},"required":["query"]}"#,
        handler: mcp_library_search,
    },
    Tool {
        name: "playback_status",
        description: "Report what is playing: state (playing/paused/finished/idle), title, position/duration and file path. Honest when mpv or a session is missing.",
        input_schema: r#"{"type":"object"}"#,
        handler: mcp_playback_status,
    },
    Tool {
        name: "playback_control",
        description: "Control playback: action play|pause|next|prev|seek|stop. 'play' accepts an optional 'track' (library index or audio file path); 'seek' needs 'seconds'. Playback persists in mpv after the call returns.",
        input_schema: r#"{"type":"object","properties":{"action":{"type":"string","enum":["play","pause","next","prev","seek","stop"]},"track":{"type":"string"},"seconds":{"type":"number"}},"required":["action"]}"#,
        handler: mcp_playback_control,
    },
];

/// `--mcp`: serve MCP on stdin/stdout until EOF. Never returns.
pub fn serve_mcp() -> ! {
    serve(TOOLS, "rabbiz-music", VERSION);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn track(path: &str, title: &str, folder: library::Folder) -> Track {
        Track {
            path: PathBuf::from(path),
            title: title.into(),
            ext: "mp3".into(),
            folder,
            subfolder: String::new(),
            mtime: 0,
        }
    }

    #[test]
    fn parse_scope_names() {
        assert_eq!(parse_scope("all"), Some(Scope::All));
        assert_eq!(parse_scope("Music"), Some(Scope::Music));
        assert_eq!(parse_scope("DOWNLOADS"), Some(Scope::Downloads));
        assert_eq!(parse_scope("videos"), None);
    }

    #[test]
    fn cli_tokens_extracts_after_marker() {
        assert_eq!(cli_tokens(&args(&["list"])), None);
        assert_eq!(
            cli_tokens(&args(&["--cli", "list", "--scope", "music"])),
            Some(args(&["list", "--scope", "music"]))
        );
        assert_eq!(
            cli_tokens(&args(&["--cli=search", "dark", "matter"])),
            Some(args(&["search", "dark", "matter"]))
        );
    }

    #[test]
    fn parse_cli_commands() {
        assert_eq!(parse_cli(&[]), Ok(Cli::Help));
        assert_eq!(parse_cli(&args(&["help"])), Ok(Cli::Help));
        assert_eq!(
            parse_cli(&args(&["list"])),
            Ok(Cli::List { scope: Scope::All })
        );
        assert_eq!(
            parse_cli(&args(&["list", "--scope", "downloads"])),
            Ok(Cli::List {
                scope: Scope::Downloads
            })
        );
        assert_eq!(
            parse_cli(&args(&["list", "music"])),
            Ok(Cli::List {
                scope: Scope::Music
            })
        );
        assert_eq!(
            parse_cli(&args(&["search", "dark", "matter"])),
            Ok(Cli::Search {
                query: "dark matter".into()
            })
        );
        assert_eq!(parse_cli(&args(&["play"])), Ok(Cli::Play { target: None }));
        assert_eq!(
            parse_cli(&args(&["play", "3"])),
            Ok(Cli::Play {
                target: Some("3".into())
            })
        );
        assert_eq!(parse_cli(&args(&["pause"])), Ok(Cli::Pause));
        assert_eq!(
            parse_cli(&args(&["seek", "30"])),
            Ok(Cli::Seek { seconds: 30.0 })
        );
        assert_eq!(parse_cli(&args(&["status"])), Ok(Cli::Status));
        assert!(parse_cli(&args(&["bogus"])).is_err());
        assert!(parse_cli(&args(&["seek"])).is_err());
        assert!(parse_cli(&args(&["seek", "abc"])).is_err());
        assert!(parse_cli(&args(&["seek", "-5"])).is_err());
        assert!(parse_cli(&args(&["search"])).is_err());
        assert!(parse_cli(&args(&["list", "--scope", "bogus"])).is_err());
        assert!(parse_cli(&args(&["pause", "extra"])).is_err());
    }

    #[test]
    fn search_tracks_matches_title_path_and_scope_fields() {
        let tracks = vec![
            track(
                "/h/Music/a/01_Dark_Matter.flac",
                "01 Dark Matter",
                library::Folder::Music,
            ),
            track(
                "/h/Downloads/podcast_one.opus",
                "Podcast One",
                library::Folder::Downloads,
            ),
            track(
                "/h/Music/b/bollywood.mp3",
                "Bollywood Mix",
                library::Folder::Music,
            ),
        ];
        assert_eq!(search_tracks(&tracks, "dark"), vec![0]);
        assert_eq!(search_tracks(&tracks, "PODCAST"), vec![1]);
        assert_eq!(search_tracks(&tracks, "downloads"), vec![1]);
        assert_eq!(search_tracks(&tracks, "flac"), vec![0]);
        assert_eq!(search_tracks(&tracks, "  "), Vec::<usize>::new());
        assert_eq!(search_tracks(&tracks, "nothing-here"), Vec::<usize>::new());
    }

    #[test]
    fn format_track_line_is_tab_separated_with_global_index() {
        let t = track("/h/Music/x.mp3", "X", library::Folder::Music);
        assert_eq!(format_track_line(7, &t), "7\tX\tMusic\t/h/Music/x.mp3");
    }

    #[test]
    fn mcp_schemas_are_valid_json() {
        for t in TOOLS {
            assert!(
                serde_json::from_str::<Value>(t.input_schema).is_ok(),
                "{}",
                t.name
            );
        }
    }

    #[test]
    fn mcp_control_validates_args() {
        assert!(mcp_playback_control(&serde_json::json!({})).is_err());
        assert!(mcp_playback_control(&serde_json::json!({"action":"bogus"})).is_err());
        assert!(mcp_playback_control(&serde_json::json!({"action":"seek"})).is_err());
        assert!(
            mcp_playback_control(&serde_json::json!({"action":"seek","seconds":-1.0})).is_err()
        );
    }
}
