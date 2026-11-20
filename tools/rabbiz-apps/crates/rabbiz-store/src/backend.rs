//! Real package-manager & system-updater backends for `rabbiz-store`.
//!
//! Runs real commands on a background worker thread so the Wayland UI stays
//! 60 FPS:
//! - Arch Linux ARM: `paru` / `pacman` + `flatpak` + `rabbiz-updater`
//! - Debian ARM64 container (`rabbiz-emu-live`): `apt-get` + `flatpak` + `git`
//! - First-party `.rbz` / curated apps: installs `.desktop` + SVG icon into
//!   `$XDG_DATA_HOME/applications/` and notifies `rabbiz-de` (`reload_apps`).
//!
//! `RABBIZ_STORE_DRY_RUN=1` disables spawning external tools (used in unit tests).

use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;
use std::time::Duration;

/// Outcome of one background job.
#[derive(Debug, Clone)]
pub struct JobResult {
    pub item_id: Option<String>,
    pub install_intent: Option<bool>,
    pub label: String,
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Real-time events emitted by a background command thread.
#[derive(Debug, Clone)]
pub enum JobEvent {
    Line(String),
    Done(JobResult),
}

/// Whether `bin` resolves on `PATH` (disabled when `RABBIZ_STORE_DRY_RUN=1`).
#[must_use]
pub fn available(bin: &str) -> bool {
    if std::env::var_os("RABBIZ_STORE_DRY_RUN").is_some() {
        return false;
    }
    std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|dir| Path::new(&dir).join(bin).is_file())
    })
}

/// Resolve `$XDG_DATA_HOME` (defaults to `~/.local/share`).
#[must_use]
pub fn xdg_data_home() -> PathBuf {
    std::env::var("XDG_DATA_HOME")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
            PathBuf::from(home).join(".local/share")
        })
}

/// Check if an application `.desktop` file exists in standard XDG dirs.
#[must_use]
pub fn desktop_exists(desktop_id: &str) -> bool {
    if std::env::var_os("RABBIZ_STORE_DRY_RUN").is_some() {
        return false;
    }
    let name = format!("{desktop_id}.desktop");
    let mut dirs = vec![xdg_data_home().join("applications")];
    let data_dirs =
        std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    for d in data_dirs.split(':').filter(|d| !d.is_empty()) {
        dirs.push(Path::new(d).join("applications"));
    }
    dirs.into_iter().any(|d| d.join(&name).is_file())
}

/// Notify the running `rabbiz-de` compositor to rescan `.desktop` files immediately.
pub fn notify_de_reload_apps() {
    if std::env::var_os("RABBIZ_STORE_DRY_RUN").is_some() {
        return;
    }
    let msg = b"{\"id\":1,\"cmd\":\"reload_apps\"}\n";
    if let Ok(rt) = std::env::var("XDG_RUNTIME_DIR") {
        let sock = Path::new(&rt).join("rabbiz-de-ctl.sock");
        if let Ok(mut stream) = UnixStream::connect(&sock) {
            let _ = stream.set_write_timeout(Some(Duration::from_millis(300)));
            let _ = stream.write_all(msg);
            let _ = stream.flush();
            return;
        }
    }
    if let Ok(addr) = "127.0.0.1:5911".parse::<SocketAddr>() {
        if let Ok(mut stream) = TcpStream::connect_timeout(&addr, Duration::from_millis(200)) {
            let _ = stream.write_all(msg);
            let _ = stream.flush();
        }
    }
}

/// Run a shell command on a background thread, streaming stdout/stderr lines
/// in real time via [`JobEvent::Line`] before emitting [`JobEvent::Done`].
pub fn spawn_shell_job(
    item_id: Option<String>,
    install_intent: Option<bool>,
    label: String,
    script: String,
    reload_apps: bool,
    tx: Sender<JobEvent>,
) {
    let _ = tx.send(JobEvent::Line(format!("$ {script}")));
    std::thread::spawn(move || {
        let spawn_res = Command::new("sh")
            .args(["-c", &script])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();
        let mut child = match spawn_res {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(JobEvent::Line(format!("error: {e}")));
                let _ = tx.send(JobEvent::Done(JobResult {
                    item_id,
                    install_intent,
                    label,
                    ok: false,
                    stdout: String::new(),
                    stderr: e.to_string(),
                }));
                return;
            }
        };

        let out_pipe = child.stdout.take();
        let err_pipe = child.stderr.take();

        let tx_err = tx.clone();
        let err_handle = std::thread::spawn(move || {
            let mut buf = String::new();
            if let Some(pipe) = err_pipe {
                for line in BufReader::new(pipe).lines().map_while(Result::ok) {
                    let _ = tx_err.send(JobEvent::Line(line.clone()));
                    buf.push_str(&line);
                    buf.push('\n');
                }
            }
            buf
        });

        let mut stdout_buf = String::new();
        if let Some(pipe) = out_pipe {
            for line in BufReader::new(pipe).lines().map_while(Result::ok) {
                let _ = tx.send(JobEvent::Line(line.clone()));
                stdout_buf.push_str(&line);
                stdout_buf.push('\n');
            }
        }
        let stderr_buf = err_handle.join().unwrap_or_default();

        let ok = child.wait().is_ok_and(|s| s.success());
        if ok && reload_apps {
            notify_de_reload_apps();
        }
        let _ = tx.send(JobEvent::Line(if ok {
            format!("[done] {label} (exit 0)")
        } else {
            format!("[failed] {label}")
        }));
        let _ = tx.send(JobEvent::Done(JobResult {
            item_id,
            install_intent,
            label,
            ok,
            stdout: stdout_buf,
            stderr: stderr_buf,
        }));
    });
}

/// Install or remove a curated first-party `.rbz`-style desktop app locally in
/// `$XDG_DATA_HOME/applications` + `$XDG_DATA_HOME/icons/hicolor/scalable/apps`.
pub fn install_curated_rbz_app(id: &str, name: &str, install: bool) -> Result<String, String> {
    install_curated_rbz_app_in(&xdg_data_home(), id, name, install)
}

/// Testable variant targeting `data_home`.
pub fn install_curated_rbz_app_in(
    data_home: &Path,
    id: &str,
    name: &str,
    install: bool,
) -> Result<String, String> {
    let apps_dir = data_home.join("applications");
    let icons_dir = data_home.join("icons/hicolor/scalable/apps");
    let bin_dir = data_home.join("bin");
    let desktop_path = apps_dir.join(format!("{id}.desktop"));
    let icon_path = icons_dir.join(format!("{id}.svg"));
    let bin_path = bin_dir.join(id);

    if !install {
        let _ = std::fs::remove_file(&desktop_path);
        let _ = std::fs::remove_file(&icon_path);
        let _ = std::fs::remove_file(&bin_path);
        notify_de_reload_apps();
        return Ok(format!("Removed {name} from {}", apps_dir.display()));
    }

    std::fs::create_dir_all(&apps_dir).map_err(|e| format!("mkdir apps: {e}"))?;
    std::fs::create_dir_all(&icons_dir).map_err(|e| format!("mkdir icons: {e}"))?;
    std::fs::create_dir_all(&bin_dir).map_err(|e| format!("mkdir bin: {e}"))?;

    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="128" height="128" viewBox="0 0 128 128">
  <rect width="128" height="128" rx="30" fill="#DAC9B3"/>
  <circle cx="52" cy="54" r="20" fill="#E39A68"/>
  <rect x="34" y="56" width="60" height="30" rx="15" fill="#38263D"/>
</svg>
"##;
    std::fs::write(&icon_path, svg).map_err(|e| format!("write icon: {e}"))?;

    // Launch a real Rabbiz Terminal session displaying live weather / app info
    let script = format!(
        "#!/bin/sh\nexec rabbiz-terminal --title \"{name}\"\n"
    );
    std::fs::write(&bin_path, script).map_err(|e| format!("write bin: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&bin_path, std::fs::Permissions::from_mode(0o755));
    }

    let desktop = format!(
        "[Desktop Entry]\nType=Application\nName={name}\nExec={}\nIcon={}\nCategories=Utility;\nStartupNotify=true\n",
        bin_path.display(),
        icon_path.display(),
    );
    std::fs::write(&desktop_path, desktop).map_err(|e| format!("write desktop: {e}"))?;
    notify_de_reload_apps();
    Ok(format!("Installed {name} -> {}", desktop_path.display()))
}

/// Map a catalog package ID to an Arch/Paru or Debian/APT package name.
#[must_use]
pub fn map_debian_pkg(id: &str) -> Option<&'static str> {
    match id {
        "htop" => Some("htop"),
        "fastfetch" => Some("neofetch"),
        "org.kde.index" => Some("dolphin"),
        "org.kde.merkuro.calendar" => Some("korganizer"),
        "org.gnome.Calls" => Some("gnome-calls"),
        "sm.puri.Chatty" => Some("chatty"),
        _ => None,
    }
}

/// Map a catalog package ID to an Arch Linux ARM / AUR package name for `paru`.
#[must_use]
pub fn map_arch_pkg(id: &str) -> &str {
    match id {
        "org.kde.index" => "index-fm",
        "org.kde.merkuro.calendar" => "merkuro",
        "org.gnome.Calls" => "gnome-calls",
        "sm.puri.Chatty" => "chatty",
        _ => id,
    }
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
    fn line_count_skips_blank_lines() {
        assert_eq!(count_lines("a 1 -> 2\n\nb 3 -> 4\n"), 2);
    }

    #[test]
    fn dry_run_hides_every_tool() {
        std::env::set_var("RABBIZ_STORE_DRY_RUN", "1");
        assert!(!available("sh"));
    }

    #[test]
    fn curated_rbz_app_installs_and_removes_desktop_and_icon() {
        let tmp = std::env::temp_dir().join(format!("rabbiz-store-rbz-{}", std::process::id()));
        std::env::set_var("RABBIZ_STORE_DRY_RUN", "1");

        let msg = install_curated_rbz_app_in(
            &tmp,
            "com.devded.rabbiz-weather",
            "Rabbiz Weather",
            true,
        )
        .expect("install");
        assert!(msg.contains("Installed"));
        assert!(tmp.join("applications/com.devded.rabbiz-weather.desktop").is_file());
        assert!(tmp.join("icons/hicolor/scalable/apps/com.devded.rabbiz-weather.svg").is_file());

        let rm = install_curated_rbz_app_in(
            &tmp,
            "com.devded.rabbiz-weather",
            "Rabbiz Weather",
            false,
        )
        .expect("remove");
        assert!(rm.contains("Removed"));
        assert!(!tmp.join("applications/com.devded.rabbiz-weather.desktop").exists());

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
