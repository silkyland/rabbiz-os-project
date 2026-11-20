//! Real package-manager backends for `rabbiz-store`.
//!
//! Commands only run when the binary is on `PATH` (`paru`, `pacman`,
//! `flatpak`, `rabbiz`). Jobs run on a worker thread and report back over a
//! channel so the UI thread never blocks. When a tool is missing — e.g. in
//! the emulator container — the store stays in demo mode and says so.
//! `RABBIZ_STORE_DRY_RUN=1` forces demo mode (tests, screenshots).

use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;

/// Outcome of one background job.
#[derive(Debug, Clone)]
pub struct JobResult {
    pub label: String,
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Whether `bin` resolves on `PATH`.
#[must_use]
pub fn available(bin: &str) -> bool {
    if std::env::var_os("RABBIZ_STORE_DRY_RUN").is_some() {
        return false;
    }
    std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|dir| Path::new(&dir).join(bin).is_file())
    })
}

/// Run `argv` on a worker thread; the result is sent to `tx`.
pub fn spawn_job(label: String, argv: Vec<String>, tx: Sender<JobResult>) {
    std::thread::spawn(move || {
        let Some((bin, args)) = argv.split_first() else {
            return;
        };
        let res = Command::new(bin)
            .args(args)
            .stdin(Stdio::null())
            .output();
        let msg = match res {
            Ok(out) => JobResult {
                label,
                ok: out.status.success(),
                stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            },
            Err(e) => JobResult {
                label,
                ok: false,
                stdout: String::new(),
                stderr: e.to_string(),
            },
        };
        let _ = tx.send(msg);
    });
}

/// Split a shell-style preview (`"paru -S --noconfirm htop"`) into argv.
/// Previews never contain quotes, so whitespace splitting is exact.
#[must_use]
pub fn argv_of(preview: &str) -> Vec<String> {
    preview.split_whitespace().map(str::to_string).collect()
}

/// Count non-empty lines (update listings print one package per line).
#[must_use]
pub fn count_lines(s: &str) -> usize {
    s.lines().filter(|l| !l.trim().is_empty()).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argv_and_line_count() {
        assert_eq!(
            argv_of("flatpak install -y flathub org.videolan.VLC"),
            vec!["flatpak", "install", "-y", "flathub", "org.videolan.VLC"]
        );
        assert_eq!(count_lines("a 1 -> 2\n\nb 3 -> 4\n"), 2);
    }

    #[test]
    fn dry_run_hides_every_tool() {
        std::env::set_var("RABBIZ_STORE_DRY_RUN", "1");
        assert!(!available("sh"));
    }
}
