//! Headless modes for `rabbiz-settings`: `--cli <cmd>` text interface and
//! `--mcp` MCP server over stdio. Both reuse the same `de_config::Prefs`
//! write path (`Prefs::save`, atomic tmp+rename+.bak) and the same
//! `system.rs` hardware readers as the GUI — nothing here writes prefs.json
//! by hand, and every `set` validates the key and value before touching
//! disk.
//!
//! Honesty contract (same as the GUI footer): prefs are loaded by the
//! compositor once at session start, so every prefs-backed change reports
//! "applies on the next desktop session". Brightness/volume/Wi-Fi/BT are
//! live system state, not prefs.

use de_config::keyboards::discover_keyboards;
use de_config::Prefs;
use rabbiz_app_kit::mcp::Tool;

use crate::model::{timeout_label, wallpaper_name};
use crate::system;
use crate::write_theme_file;

/// Prefs keys we expose, in display order. Names are the JSON keys written
/// to `~/.config/rabbiz-de/prefs.json` (camelCase, `de*` prefixed where the
/// field doc says so).
const KEYS: &[(&str, &str)] = &[
    ("deTheme", "light | dark"),
    ("deAccent", "accent id"),
    ("deWallpaperId", "wallpaper id"),
    ("doNotDisturb", "true | false"),
    ("deScreenTimeoutSec", "seconds (0 = never)"),
    ("deKeyboard", "keyboard id"),
    ("deAllowThirdPartyKeyboards", "true | false"),
    ("hiddenApps", "comma-separated desktop-ids"),
    ("dock", "comma-separated desktop-ids (max 6)"),
    ("homeOrder", "comma-separated desktop-ids"),
];

/// Accent ids the DE/shell know about: the brand default plus the legacy
/// shell palette (`ShellPrefs.accents`). `deAccent` is not consumed by
/// de-design yet, so this list is the contract we enforce.
const ACCENT_IDS: &[&str] = &["copper", "mono", "blue", "emerald", "amber", "crimson"];

const NEXT_SESSION: &str = "saved to prefs.json — applies on the next desktop session";

/// Entry point for `--cli <args>`. Returns the process exit code.
pub fn run(args: &[String]) -> i32 {
    let Some((cmd, rest)) = args.split_first() else {
        print!("{}", usage());
        return 0;
    };
    match cmd.as_str() {
        "help" | "--help" | "-h" => {
            print!("{}", usage());
            0
        }
        "get" => cmd_get(rest),
        "set" => cmd_set(rest),
        "system" => {
            print!("{}", system_status());
            0
        }
        other => {
            eprintln!("unknown command: {other}");
            eprint!("{}", usage());
            2
        }
    }
}

fn usage() -> String {
    let mut s = String::from(
        "rabbiz-settings — headless settings\n\
         \n\
         usage:\n\
         \x20 rabbiz-settings                 open the settings GUI\n\
         \x20 rabbiz-settings --cli <cmd>     text CLI\n\
         \x20 rabbiz-settings --mcp           MCP server on stdio\n\
         \n\
         commands:\n\
         \x20 get [key]              print one pref, or all when key is omitted\n\
         \x20 set <key> <value>      write a pref (validated; applies next session)\n\
         \x20 system                 live brightness / volume / Wi-Fi / Bluetooth\n\
         \x20 help                   this text\n\
         \n\
         keys:\n",
    );
    for (k, v) in KEYS {
        s.push_str(&format!("  {k:<32} {v}\n"));
    }
    s
}

fn cmd_get(args: &[String]) -> i32 {
    let prefs = Prefs::load();
    match args {
        [] => {
            for (k, _) in KEYS {
                match pref_value(&prefs, k) {
                    Ok(v) => println!("{k} = {v}"),
                    Err(e) => {
                        eprintln!("{e}");
                        return 1;
                    }
                }
            }
            0
        }
        [key] => match pref_value(&prefs, key) {
            Ok(v) => {
                println!("{v}");
                0
            }
            Err(e) => {
                eprintln!("{e}");
                1
            }
        },
        _ => {
            eprintln!("usage: get [key]");
            2
        }
    }
}

fn cmd_set(args: &[String]) -> i32 {
    let [key, value, ..] = args else {
        eprintln!("usage: set <key> <value>");
        return 2;
    };
    // `set` takes exactly one value token; lists are comma-separated.
    if args.len() > 2 {
        eprintln!("usage: set <key> <value>  (lists are comma-separated, no spaces)");
        return 2;
    }
    match set_pref(key, value) {
        Ok(msg) => {
            println!("{msg}");
            0
        }
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

// ------------------------------------------------------------------ get/set

/// Read one prefs key as a display string. Unknown key → error listing keys.
pub fn pref_value(prefs: &Prefs, key: &str) -> Result<String, String> {
    let known = KEYS.iter().any(|(k, _)| *k == key);
    if !known {
        return Err(format!(
            "unknown key '{key}' — expected one of: {}",
            KEYS.iter().map(|(k, _)| *k).collect::<Vec<_>>().join(", ")
        ));
    }
    Ok(match key {
        "deTheme" => prefs.theme_mode.clone(),
        "deAccent" => prefs.accent.clone(),
        "deWallpaperId" => format!(
            "{} ({})",
            prefs.wallpaper_id,
            wallpaper_name(&prefs.wallpaper_id)
        ),
        "doNotDisturb" => prefs.do_not_disturb.to_string(),
        "deScreenTimeoutSec" => format!(
            "{} ({})",
            prefs.screen_timeout_sec,
            timeout_label(prefs.screen_timeout_sec)
        ),
        "deKeyboard" => prefs.keyboard_id.clone(),
        "deAllowThirdPartyKeyboards" => prefs.allow_third_party_keyboards.to_string(),
        "hiddenApps" => join_ids(&prefs.hidden_apps.iter().cloned().collect::<Vec<_>>()),
        "dock" => join_ids(&prefs.dock),
        "homeOrder" => join_ids(&prefs.home_order),
        _ => unreachable!("key validated above"),
    })
}

fn join_ids(ids: &[String]) -> String {
    if ids.is_empty() {
        "(none)".into()
    } else {
        ids.join(",")
    }
}

/// Validate `value` for `key`, mutate `prefs`, save atomically. Mirrors the
/// app's write paths exactly (theme also writes the mode file, disabling
/// 3rd-party keyboards forces `rabbiz-osk`, uninstalled keyboards refused).
pub fn set_pref(key: &str, value: &str) -> Result<String, String> {
    let known = KEYS.iter().any(|(k, _)| *k == key);
    if !known {
        return Err(format!(
            "unknown key '{key}' — expected one of: {}",
            KEYS.iter().map(|(k, _)| *k).collect::<Vec<_>>().join(", ")
        ));
    }
    let mut prefs = Prefs::load();
    // Every arm either assigns `note` or diverges (`return`/`unreachable!`).
    let note: String;
    match key {
        "deTheme" => {
            let mode = value.trim();
            if mode != "light" && mode != "dark" {
                return Err("deTheme must be 'light' or 'dark'".into());
            }
            prefs.theme_mode = mode.into();
            // Same as the GUI: write the mode file the session reads.
            write_theme_file(mode).map_err(|e| format!("couldn't write theme file: {e}"))?;
            note = format!("theme: {mode}");
        }
        "deAccent" => {
            let id = value.trim();
            if !ACCENT_IDS.contains(&id) {
                return Err(format!(
                    "unknown accent '{id}' — expected one of: {}",
                    ACCENT_IDS.join(", ")
                ));
            }
            prefs.accent = id.into();
            note = format!("accent: {id}");
        }
        "deWallpaperId" => {
            let id = value.trim();
            if !de_paint::wallpaper::IDS.contains(&id) {
                return Err(format!(
                    "unknown wallpaper '{id}' — expected one of: {}",
                    de_paint::wallpaper::IDS.join(", ")
                ));
            }
            prefs.wallpaper_id = id.into();
            note = format!("wallpaper: {}", wallpaper_name(id));
        }
        "doNotDisturb" => {
            let b = parse_bool(value)?;
            prefs.do_not_disturb = b;
            note = format!("do not disturb: {}", if b { "on" } else { "off" });
        }
        "deScreenTimeoutSec" => {
            let secs: u32 = value.trim().parse().map_err(|_| {
                format!("deScreenTimeoutSec must be a number of seconds, got '{value}'")
            })?;
            prefs.screen_timeout_sec = secs;
            note = format!("screen timeout: {}", timeout_label(secs));
        }
        "deKeyboard" => {
            let id = value.trim();
            let kbs = discover_keyboards(prefs.allow_third_party_keyboards);
            match kbs.iter().find(|k| k.id == id) {
                Some(kb) if !kb.installed => {
                    return Err(format!("{} is not installed", kb.name));
                }
                Some(kb) => {
                    prefs.keyboard_id = kb.id.clone();
                    note = format!("keyboard: {}", kb.name);
                }
                None => {
                    return Err(format!(
                        "unknown keyboard '{id}' — known ids: {}",
                        kbs.iter()
                            .map(|k| k.id.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
            }
        }
        "deAllowThirdPartyKeyboards" => {
            let b = parse_bool(value)?;
            prefs.allow_third_party_keyboards = b;
            if !b {
                // Same as the GUI: blocking 3rd-party forces the built-in OSK.
                prefs.keyboard_id = "rabbiz-osk".into();
            }
            note = format!(
                "3rd-party keyboards: {}",
                if b { "allowed" } else { "blocked" }
            );
        }
        "hiddenApps" => {
            prefs.hidden_apps = parse_id_list(value)?.into_iter().collect();
            note = format!(
                "hidden apps: {}",
                join_ids(&prefs.hidden_apps.iter().cloned().collect::<Vec<_>>())
            );
        }
        "dock" => {
            let ids = parse_id_list(value)?;
            if ids.len() > 6 {
                return Err("dock holds at most 6 entries".into());
            }
            prefs.dock = ids.clone();
            note = format!("dock: {}", join_ids(&ids));
        }
        "homeOrder" => {
            prefs.home_order = parse_id_list(value)?;
            note = format!("home order: {}", join_ids(&prefs.home_order));
        }
        _ => unreachable!("key validated above"),
    }
    prefs
        .save()
        .map_err(|e| format!("couldn't save prefs: {e}"))?;
    Ok(format!("{note} — {NEXT_SESSION}"))
}

/// `true|false|on|off|1|0|yes|no` (case-insensitive).
fn parse_bool(value: &str) -> Result<bool, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "on" | "1" | "yes" => Ok(true),
        "false" | "off" | "0" | "no" => Ok(false),
        other => Err(format!("expected true/false, got '{other}'")),
    }
}

/// Comma-separated desktop-ids; empty string clears the list.
fn parse_id_list(value: &str) -> Result<Vec<String>, String> {
    let v = value.trim();
    if v.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for id in v.split(',') {
        let id = id.trim();
        if id.is_empty() {
            return Err("empty entry in id list".into());
        }
        if !out.iter().any(|o| o == id) {
            out.push(id.to_string());
        }
    }
    Ok(out)
}

// ------------------------------------------------------------- system status

/// Live hardware state as text lines — the same reads the GUI makes
/// (sysfs backlight, `wpctl`, `nmcli`, `bluetoothctl`).
pub fn system_status() -> String {
    let mut s = String::new();
    match system::backlight_level() {
        Some((raw, max)) if max > 0 => {
            let pct = (raw as f32 / max as f32 * 100.0).round() as i32;
            s.push_str(&format!("brightness: {pct}% ({raw}/{max})\n"));
        }
        _ => s.push_str("brightness: unavailable\n"),
    }
    match system::volume() {
        Some((v, muted)) => {
            let pct = (v.clamp(0.0, 1.0) * 100.0).round() as i32;
            s.push_str(&format!(
                "volume: {pct}%{}\n",
                if muted { " (muted)" } else { "" }
            ));
        }
        None => s.push_str("volume: unavailable\n"),
    }
    s.push_str(&format!(
        "wifi: {}\n",
        tri(system::wifi_state(), "on", "off")
    ));
    s.push_str(&format!(
        "bluetooth: {}\n",
        tri(system::bt_state(), "on", "off")
    ));
    s
}

fn tri(state: Option<bool>, on: &'static str, off: &'static str) -> &'static str {
    match state {
        Some(true) => on,
        Some(false) => off,
        None => "unknown",
    }
}

// -------------------------------------------------------------------- MCP

fn arg_str<'a>(args: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(|v| v.as_str())
}

fn mcp_get(args: &serde_json::Value) -> Result<String, String> {
    let prefs = Prefs::load();
    match arg_str(args, "key") {
        Some(key) => pref_value(&prefs, key),
        None => {
            let mut s = String::new();
            for (k, _) in KEYS {
                {
                    let v = pref_value(&prefs, k)?;
                    s.push_str(&format!("{k} = {v}\n"))
                }
            }
            Ok(s.trim_end().to_string())
        }
    }
}

fn mcp_set(args: &serde_json::Value) -> Result<String, String> {
    let key = arg_str(args, "key").ok_or("missing 'key'")?;
    let value = arg_str(args, "value").ok_or("missing 'value'")?;
    set_pref(key, value)
}

fn mcp_system(_: &serde_json::Value) -> Result<String, String> {
    Ok(system_status().trim_end().to_string())
}

/// MCP tool table served by `--mcp`.
pub static TOOLS: &[Tool] = &[
    Tool {
        name: "settings_get",
        description: "Read a RabbizOS desktop preference (deTheme, deAccent, deWallpaperId, doNotDisturb, deScreenTimeoutSec, deKeyboard, deAllowThirdPartyKeyboards, hiddenApps, dock, homeOrder). Omit 'key' to list all.",
        input_schema: r#"{"type":"object","properties":{"key":{"type":"string","description":"pref key; omit for all"}},"additionalProperties":false}"#,
        handler: mcp_get,
    },
    Tool {
        name: "settings_set",
        description: "Write a validated RabbizOS desktop preference; saved to prefs.json and applies on the next desktop session.",
        input_schema: r#"{"type":"object","properties":{"key":{"type":"string"},"value":{"type":"string","description":"e.g. 'dark', 'ember', 'true', '600', comma-separated ids"}},"required":["key","value"],"additionalProperties":false}"#,
        handler: mcp_set,
    },
    Tool {
        name: "system_status",
        description: "Live brightness, volume/mute, Wi-Fi and Bluetooth state read back from the OS.",
        input_schema: r#"{"type":"object","properties":{},"additionalProperties":false}"#,
        handler: mcp_system,
    },
];

// -------------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bool_parse() {
        assert!(parse_bool("true").unwrap());
        assert!(!parse_bool("OFF").unwrap());
        assert!(parse_bool("1").unwrap());
        assert!(!parse_bool("no").unwrap());
        assert!(parse_bool("maybe").is_err());
    }

    #[test]
    fn id_list_parse() {
        assert_eq!(
            parse_id_list("a,b, a ,b").unwrap(),
            vec!["a".to_string(), "b".to_string()]
        );
        assert!(parse_id_list("a,,b").is_err());
        assert!(parse_id_list("").unwrap().is_empty());
    }

    #[test]
    fn unknown_keys_rejected() {
        let prefs = Prefs::default();
        assert!(pref_value(&prefs, "nope").is_err());
        assert!(set_pref("nope", "x").is_err());
        assert!(set_pref("PATH", "/etc").is_err());
        assert!(set_pref("hiddenApps'; rm -rf /", "x").is_err());
    }

    #[test]
    fn invalid_values_rejected_without_writing() {
        // These must fail validation before Prefs::load/save is reached in
        // a way that could mutate anything.
        assert!(set_pref("deTheme", "blue").is_err());
        assert!(set_pref("deTheme", "dark; rm").is_err());
        assert!(set_pref("deWallpaperId", "not-a-wallpaper").is_err());
        assert!(set_pref("deAccent", "neon-pink").is_err());
        assert!(set_pref("deScreenTimeoutSec", "soon").is_err());
        assert!(set_pref("deScreenTimeoutSec", "-5").is_err());
        assert!(set_pref("doNotDisturb", "perhaps").is_err());
        assert!(set_pref("deKeyboard", "definitely-not-a-kb").is_err());
        assert!(set_pref("dock", "a,b,c,d,e,f,g").is_err());
    }

    #[test]
    fn valid_values_pass_validation() {
        // deTheme writes the mode file under XDG_CONFIG_HOME — point HOME at
        // a temp dir so the test never touches the real config.
        let tmp = std::env::temp_dir().join(format!("rabbiz-settings-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::env::set_var("XDG_CONFIG_HOME", &tmp);
        let r = set_pref("deTheme", "dark");
        std::env::remove_var("XDG_CONFIG_HOME");
        let _ = std::fs::remove_dir_all(&tmp);
        assert!(r.is_ok(), "{r:?}");
        assert!(r.unwrap().contains("applies on the next desktop session"));
    }

    #[test]
    fn key_list_matches_prefs_fields() {
        // Every Prefs field must be reachable through exactly one key.
        assert_eq!(KEYS.len(), 10);
        let names: Vec<&str> = KEYS.iter().map(|(k, _)| *k).collect();
        for k in [
            "deTheme",
            "deAccent",
            "deWallpaperId",
            "doNotDisturb",
            "deScreenTimeoutSec",
            "deKeyboard",
            "deAllowThirdPartyKeyboards",
            "hiddenApps",
            "dock",
            "homeOrder",
        ] {
            assert!(names.contains(&k), "missing key {k}");
        }
        // And the JSON serde names really are these (guard against drift).
        let json = Prefs::default().to_json();
        for k in names {
            assert!(json.contains(&format!("\"{k}\"")), "prefs.json lacks {k}");
        }
    }

    #[test]
    fn tools_have_valid_schemas() {
        for t in TOOLS {
            assert!(serde_json::from_str::<serde_json::Value>(t.input_schema).is_ok());
        }
        assert_eq!(TOOLS.len(), 3);
    }
}
