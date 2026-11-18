//! EgisTech ET520 fingerprint reader — the `a21s-et5xx` ABI
//! (`/sys/kernel/a21s_fp/{chipid,status,irq_count}` +
//! `trigger_unlock`, ABI-locked in
//! `devices/samsung/a21s/findings/2026-11-18-soc-drivers-a21s-assumptions.md`).
//!
//! The kernel raises `irq_count` when the sensor sees a finger; while the
//! lock screen or sleep surface is up, an increase means a successful
//! unlock (the sensor is a convenience, not a security boundary — P1
//! decision). `trigger_unlock` is the driver's confirm pulse; the QML shell
//! wrote it on every accepted unlock and `a21s-power-key fp-unlock` still
//! does.

use std::path::Path;

const SYSFS: &str = "/sys/kernel/a21s_fp";

/// One sysfs reading; `None` when the driver is absent.
#[derive(Debug, Clone)]
pub struct Reading {
    /// `et5xx` chip id, or "" when the driver reported `none`.
    pub chipid: String,
    /// Driver status word (`offline`, `idle`, …) as written by the kernel.
    pub status: String,
    /// Monotonic finger-interrupt counter.
    pub irq_count: u64,
}

impl Reading {
    /// A sensor exists (chip id present and not `none`).
    #[must_use]
    pub fn supported(&self) -> bool {
        !self.chipid.is_empty() && self.chipid != "none"
    }
}

/// Read the three ABI files; absent driver ⇒ `None`.
#[must_use]
pub fn read() -> Option<Reading> {
    let dir = Path::new(SYSFS);
    let chipid = std::fs::read_to_string(dir.join("chipid"))
        .ok()
        .map(|s| s.trim().to_string())?;
    let status = std::fs::read_to_string(dir.join("status"))
        .ok()
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let irq_count = std::fs::read_to_string(dir.join("irq_count"))
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);
    Some(Reading { chipid, status, irq_count })
}

/// Pulse the driver's unlock-confirm (same write `a21s-power-key fp-unlock`
/// makes). Best effort — absence is fine.
pub fn arm_trigger() {
    let _ = std::fs::write(Path::new(SYSFS).join("trigger_unlock"), "1\n");
}
