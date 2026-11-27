use std::process::Command;
use std::sync::mpsc;
use std::thread;

/// Validate an SSH `[user@]host` target before passing it to `/usr/bin/ssh`.
pub fn is_valid_host(host: &str) -> bool {
    if host.is_empty() || host.len() > 255 || host.starts_with('-') {
        return false;
    }
    if host
        .chars()
        .any(|c| c.is_whitespace() || c.is_control() || !is_allowed_host_char(c))
    {
        return false;
    }
    if let Some((user, hostname)) = host.split_once('@') {
        if user.is_empty()
            || hostname.is_empty()
            || hostname.contains('@')
            || user.starts_with('-')
            || hostname.starts_with('-')
        {
            return false;
        }
    }
    true
}

fn is_allowed_host_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ':' | '@' | '[' | ']')
}

/// Starts the device-side wayvnc unit through SSH, so the app is self-contained:
/// plug in USB, hit Start, hit Connect.
pub fn start_remote_wayvnc(host: String) -> mpsc::Receiver<Result<(), String>> {
    let (tx, rx) = mpsc::channel();
    thread::Builder::new()
        .name("RabbizMirror.SSH".to_string())
        .spawn(move || {
            let trimmed = host.trim();
            if trimmed.is_empty() {
                let _ = tx.send(Err("Enter the device address first.".to_string()));
                return;
            }
            if !is_valid_host(trimmed) {
                let _ = tx.send(Err("Invalid device address.".to_string()));
                return;
            }
            let target = if trimmed.contains('@') {
                trimmed.to_string()
            } else {
                format!("root@{trimmed}")
            };

            let output = Command::new("/usr/bin/ssh")
                .args([
                    "-o",
                    "BatchMode=yes",
                    "-o",
                    "ConnectTimeout=6",
                    "-o",
                    "StrictHostKeyChecking=accept-new",
                    "--",
                    &target,
                    "rabbiz-mirror",
                    "start",
                ])
                .output();

            let result = match output {
                Ok(out) if out.status.success() => Ok(()),
                Ok(out) => {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    let first_two: Vec<&str> = stderr
                        .lines()
                        .map(str::trim)
                        .filter(|l| !l.is_empty())
                        .take(2)
                        .collect();
                    if first_two.is_empty() {
                        Err(format!(
                            "ssh exited with status {}",
                            out.status.code().unwrap_or(-1)
                        ))
                    } else {
                        Err(first_two.join(" "))
                    }
                }
                Err(e) => Err(format!("Could not run ssh: {e}")),
            };
            let _ = tx.send(result);
        })
        .expect("spawn ssh thread");
    rx
}
