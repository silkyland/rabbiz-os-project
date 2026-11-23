//! Root-owned `rabbiz-auth-helper` daemon (`docs/DE-KEYGUARD-AUTH.md` §3).
//!
//! Listens on `/run/rabbiz-auth/helper.sock` (`0660`), stores credentials and
//! lockout state under `/var/lib/rabbiz-auth/<user>/` (`0700` dir, `0600` files),
//! verifies PINs with Argon2id (and Passphrases via `su`/`unix_chkpwd` PAM),
//! and zeroizes all secret buffers immediately after use.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use de_system::auth::{
    load_lockout_policy, root_keyguard_paths, save_lockout_state_atomic,
    save_pin_credential_atomic, AuthOutcome, HelperRequest, HelperResponse, LockMethod,
    PinCredential, DEFAULT_AUTH_HELPER_SOCKET,
};
use zeroize::Zeroize;

fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn verify_pam_passphrase(user: &str, passphrase: &str) -> bool {
    let Ok(mut child) = Command::new("/bin/su")
        .args(["-c", "true", user])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    if let Some(mut stdin) = child.stdin.take() {
        let mut buf = format!("{passphrase}\n").into_bytes();
        let _ = stdin.write_all(&buf);
        buf.zeroize();
    }
    child.wait().is_ok_and(|s| s.success())
}

fn handle_request(req: &HelperRequest) -> HelperResponse {
    let Some((cred_path, state_path)) = root_keyguard_paths(&req.user) else {
        return HelperResponse::Unavailable;
    };
    let now_s = now_epoch_secs();
    let mut policy = load_lockout_policy(&state_path, now_s);
    if let Some(secs) = policy.cooldown_remaining(now_s) {
        return HelperResponse::Cooldown {
            retry_after_secs: secs,
        };
    }

    let ok = match req.method {
        LockMethod::Swipe => true,
        LockMethod::Pin | LockMethod::BiometricWithFallback => {
            let Ok(text) = std::fs::read_to_string(&cred_path) else {
                return HelperResponse::Unavailable;
            };
            let Ok(cred) = serde_json::from_str::<PinCredential>(&text) else {
                return HelperResponse::Unavailable;
            };
            let (valid, upgraded) = cred.verify_and_upgrade(&req.secret);
            if let Some(new_cred) = upgraded {
                let _ = save_pin_credential_atomic(&cred_path, &new_cred);
            }
            valid
        }
        LockMethod::Passphrase => verify_pam_passphrase(&req.user, &req.secret),
    };

    let outcome = policy.record_attempt(ok, now_s);
    if let Some(parent) = state_path.parent() {
        let _ = std::fs::create_dir_all(parent);
        let _ = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700));
    }
    let _ = save_lockout_state_atomic(&state_path, policy.state());

    match outcome {
        AuthOutcome::Accepted => HelperResponse::Accepted,
        AuthOutcome::Rejected { attempts_remaining } => {
            HelperResponse::Rejected { attempts_remaining }
        }
        AuthOutcome::Cooldown { retry_after_secs } => {
            HelperResponse::Cooldown { retry_after_secs }
        }
    }
}

fn handle_client(stream: UnixStream) {
    let mut reader = BufReader::new(&stream);
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        line.zeroize();
        return;
    }
    let resp = match serde_json::from_str::<HelperRequest>(line.trim()) {
        Ok(req) => handle_request(&req),
        Err(_) => HelperResponse::Unavailable,
    };
    line.zeroize();
    if let Ok(json) = serde_json::to_string(&resp) {
        let mut out = stream;
        let _ = out.write_all(json.as_bytes());
        let _ = out.write_all(b"\n");
        let _ = out.flush();
    }
}

fn main() {
    let sock_path = std::env::var("RABBIZ_AUTH_SOCKET")
        .unwrap_or_else(|_| DEFAULT_AUTH_HELPER_SOCKET.to_string());
    let path = Path::new(&sock_path);
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            eprintln!("rabbiz-auth-helper: mkdir {}: {e}", parent.display());
            std::process::exit(1);
        }
    }
    let _ = std::fs::remove_file(path);
    let listener = match UnixListener::bind(path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("rabbiz-auth-helper: bind {}: {e}", path.display());
            std::process::exit(1);
        }
    };
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o660));
    for conn in listener.incoming().flatten() {
        handle_client(conn);
    }
}
