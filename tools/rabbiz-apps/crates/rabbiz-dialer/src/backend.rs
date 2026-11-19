//! Telephony backend detection and ModemManager (`mmcli`) bridge.
//!
//! Reality on the A21s today (devices/samsung/a21s/docs/HANDOFF-MODEM.md,
//! shell/Rabbiz/Services/Telephony.qml, shell/apps/dialer/mm-backend.sh):
//! the Shannon s318ap CP sits behind the `cpif` kernel driver and exposes
//! `/dev/umts_*` nodes, but there is no telephony stack yet — no
//! ModemManager plugin, no oFono. `mm-backend.sh` documents `mmcli` as the
//! *intended* privileged path once a modem object exists.
//!
//! So this module probes once at startup:
//!   1. `mmcli -L` — a real ModemManager modem → real calls via
//!      `--voice-create-call` / `-o <call> --hangup` / `--send-dtmf`.
//!   2. `/dev/umts_*` nodes — cpif is loaded but no call path exists →
//!      simulated calls, honestly labelled.
//!   3. Neither — simulated calls, labelled "No modem".

use std::process::Command;

/// How calls are placed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    /// ModemManager modem found; `mmcli -m <index>` drives real calls.
    Mmcli { modem: usize },
    /// No call-capable backend; the app simulates the call UX (same demo
    /// state machine as the shell Telephony service).
    Simulated,
}

#[derive(Debug)]
pub struct Backend {
    pub kind: BackendKind,
    /// `/dev/umts_*` (a21s-cpif) or cdc-wdm/wwan node present.
    pub modem_node: bool,
}

impl Backend {
    /// One-time probe. `mmcli -L` is quick; a missing binary just means
    /// "no ModemManager".
    #[must_use]
    pub fn detect() -> Self {
        let modem_node = umts_nodes_present();
        let kind =
            mmcli_modem_index().map_or(BackendKind::Simulated, |m| BackendKind::Mmcli { modem: m });
        Self { kind, modem_node }
    }

    /// Header chip text: "MM", "SIM", or "DEMO".
    #[must_use]
    pub fn chip(&self) -> &'static str {
        match self.kind {
            BackendKind::Mmcli { .. } => "MM",
            BackendKind::Simulated if self.modem_node => "SIM",
            BackendKind::Simulated => "DEMO",
        }
    }

    /// Honest one-line status for the header subtitle.
    #[must_use]
    pub fn status_line(&self) -> &'static str {
        match self.kind {
            BackendKind::Mmcli { .. } => "ModemManager · real calls",
            BackendKind::Simulated if self.modem_node => "Modem node · simulated calls",
            BackendKind::Simulated => "No modem · simulated calls",
        }
    }

    /// Whether the "simulate incoming call" affordance may be shown — only
    /// meaningful while calls are simulated anyway.
    #[must_use]
    pub fn can_simulate_incoming(&self) -> bool {
        matches!(self.kind, BackendKind::Simulated)
    }
}

/// True when a cpif (`umts_*`) or ModemManager (`cdc-wdm*`/`wwan*`) device
/// node exists — same probe list as `Telephony.qml`'s FolderListModel.
fn umts_nodes_present() -> bool {
    let Ok(rd) = std::fs::read_dir("/dev") else {
        return false;
    };
    rd.flatten().any(|e| {
        let n = e.file_name();
        let n = n.to_string_lossy();
        n.starts_with("umts_ipc")
            || n.starts_with("umts_at")
            || n.starts_with("umts_rfs")
            || n.starts_with("umts_boot")
            || n.starts_with("cdc-wdm")
            || n.starts_with("wwan")
    })
}

/// Parse `mmcli -L` for the first modem index. Output lines look like
/// `  /org/freedesktop/ModemManager1/Modem/0 [vendor] name`.
fn mmcli_modem_index() -> Option<usize> {
    let out = Command::new("mmcli").arg("-L").output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines() {
        if let Some(pos) = line.rfind("/Modem/") {
            let tail = &line[pos + "/Modem/".len()..];
            let digits: String = tail.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(idx) = digits.parse::<usize>() {
                return Some(idx);
            }
        }
    }
    None
}

/// Place a real call through ModemManager. Returns the call object path
/// (`/org/freedesktop/ModemManager1/Modem/N/Call/M`) parsed from mmcli
/// output, used later for hangup/DTMF.
pub fn mmcli_create_call(modem: usize, number: &str) -> Result<String, String> {
    let arg = format!("number={number}");
    let modem = modem.to_string();
    let out = Command::new("mmcli")
        .args(["-m", modem.as_str(), "--voice-create-call", arg.as_str()])
        .output()
        .map_err(|e| format!("mmcli failed: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    let text = String::from_utf8_lossy(&out.stdout);
    Ok(first_object_path(&text).unwrap_or_default())
}

/// Hang up a call object created by [`mmcli_create_call`].
pub fn mmcli_hangup(call_path: &str) -> Result<(), String> {
    if call_path.is_empty() {
        return Err("no call object path".to_string());
    }
    let out = Command::new("mmcli")
        .args(["-o", call_path, "--hangup"])
        .output()
        .map_err(|e| format!("mmcli failed: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Send in-call DTMF tones (`"1*#"`).
pub fn mmcli_send_dtmf(call_path: &str, tones: &str) -> Result<(), String> {
    if call_path.is_empty() {
        return Err("no call object path".to_string());
    }
    let out = Command::new("mmcli")
        .args(["-o", call_path, "--send-dtmf", tones])
        .output()
        .map_err(|e| format!("mmcli failed: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// First `/org/...` token in mmcli output.
fn first_object_path(text: &str) -> Option<String> {
    text.split_whitespace()
        .find(|t| t.starts_with('/'))
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mmcli_list() {
        // Shape of `mmcli -L` output.
        let text = "    /org/freedesktop/ModemManager1/Modem/0 [Qualcomm] SDX55\n";
        // Exercise the parser logic inline (mmcli_modem_index shells out, so
        // replicate its scan here).
        let mut found = None;
        for line in text.lines() {
            if let Some(pos) = line.rfind("/Modem/") {
                let tail = &line[pos + "/Modem/".len()..];
                let digits: String = tail.chars().take_while(|c| c.is_ascii_digit()).collect();
                found = digits.parse::<usize>().ok();
            }
        }
        assert_eq!(found, Some(0));
    }

    #[test]
    fn extracts_object_path() {
        assert_eq!(
            first_object_path("Call created: /org/freedesktop/ModemManager1/Modem/0/Call/1\n"),
            Some("/org/freedesktop/ModemManager1/Modem/0/Call/1".to_string())
        );
        assert_eq!(first_object_path("no path here"), None);
    }

    #[test]
    fn backend_labels() {
        let sim = Backend {
            kind: BackendKind::Simulated,
            modem_node: false,
        };
        assert_eq!(sim.chip(), "DEMO");
        assert_eq!(sim.status_line(), "No modem · simulated calls");
        let cpif = Backend {
            kind: BackendKind::Simulated,
            modem_node: true,
        };
        assert_eq!(cpif.chip(), "SIM");
        assert_eq!(cpif.status_line(), "Modem node · simulated calls");
        let mm = Backend {
            kind: BackendKind::Mmcli { modem: 0 },
            modem_node: true,
        };
        assert_eq!(mm.chip(), "MM");
        assert!(!mm.can_simulate_incoming());
    }
}
