//! SMS send backend. The A21s modem is Exynos CP via `cpif` — `device.toml`
//! marks `modem = "planned"` and the QML dialer's mm-backend.sh reference
//! was removed with shell/apps (see git history), so ModemManager may not
//! even be installed. This seam
//! therefore probes for `mmcli` once at startup: when present we attempt a
//! real `--messaging-create-sms`; when absent every outgoing message stays
//! honestly `Queued` and is still persisted locally.

use std::process::Command;

/// Result of probing/using the send path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// `mmcli` binary exists — sends are attempted for real.
    Mmcli,
    /// No usable backend; messages queue locally.
    Unavailable,
}

impl Backend {
    /// Probe once. Cheap: single `mmcli --version` spawn.
    #[must_use]
    pub fn detect() -> Self {
        match Command::new("mmcli").arg("--version").output() {
            Ok(out) if out.status.success() => Self::Mmcli,
            _ => Self::Unavailable,
        }
    }

    /// Attempt to send `body` to `peer` (E.164 or national digits).
    /// `Ok(())` = accepted by the modem stack; `Err(reason)` = honest failure
    /// the UI surfaces as `Failed`.
    pub fn send(&self, peer: &str, body: &str) -> Result<(), String> {
        match self {
            Self::Unavailable => Err("modem not available".to_string()),
            Self::Mmcli => mmcli_send(peer, body),
        }
    }
}

/// `mmcli -m 0 --messaging-create-sms 'text='…',number='…''`.
/// mmcli's `key='value'` parser cannot represent a literal `'` inside a
/// value, so bodies containing one are rejected up front rather than
/// silently mangled.
fn mmcli_args(peer: &str, body: &str) -> Result<Vec<String>, String> {
    if body.contains('\'') || peer.contains('\'') {
        return Err("text or number contains a quote the modem CLI can't send".to_string());
    }
    Ok(vec![
        "-m".to_string(),
        "0".to_string(),
        "--messaging-create-sms".to_string(),
        format!("text='{body}',number='{peer}'"),
    ])
}

fn mmcli_send(peer: &str, body: &str) -> Result<(), String> {
    let args = mmcli_args(peer, body)?;
    match Command::new("mmcli").args(&args).output() {
        Ok(out) if out.status.success() => Ok(()),
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let msg = stderr.trim();
            Err(if msg.is_empty() {
                format!("mmcli exited {}", out.status)
            } else {
                msg.chars().take(120).collect()
            })
        }
        Err(e) => Err(format!("mmcli failed: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_shape() {
        let a = mmcli_args("0812345678", "hello").unwrap();
        assert_eq!(a[0], "-m");
        assert_eq!(a[1], "0");
        assert_eq!(a[2], "--messaging-create-sms");
        assert_eq!(a[3], "text='hello',number='0812345678'");
    }

    #[test]
    fn quote_in_body_rejected() {
        assert!(mmcli_args("0812345678", "it's").is_err());
    }

    #[test]
    fn unavailable_always_errors() {
        assert!(Backend::Unavailable.send("123", "hi").is_err());
    }
}
