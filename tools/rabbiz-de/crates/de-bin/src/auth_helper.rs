//! Root-owned `rabbiz-auth-helper` daemon (`docs/DE-KEYGUARD-AUTH.md` §3).
//!
//! Listens on `/run/rabbiz-auth/helper.sock` (`0660`), stores credentials and
//! lockout state under `/var/lib/rabbiz-auth/<user>/` (`0700` dir, `0600` files),
//! verifies PINs with Argon2id (and Passphrases via `unix_chkpwd` without
//! `pam_rootok`), enforces `SO_PEERCRED` peer UID authorization, and zeroizes
//! all secret buffers immediately after use.

use std::ffi::CString;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use de_system::auth::{
    is_valid_username, load_lockout_policy, root_keyguard_paths, save_lockout_state_atomic,
    save_pin_credential_atomic, AuthOutcome, HelperRequest, HelperResponse, LockMethod,
    PinCredential, DEFAULT_AUTH_HELPER_SOCKET,
};
use zeroize::Zeroize;

/// Maximum serialized request line length (prevents OOM from oversized client writes).
const MAX_REQUEST_BYTES: u64 = 4096;
/// Socket read/write timeout (prevents a stalled client from DoSing Keyguard).
const IO_TIMEOUT: Duration = Duration::from_secs(5);
/// Maximum concurrent client handler threads.
const MAX_CONCURRENT_CLIENTS: usize = 16;

fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Extract the kernel-verified peer UID from an accepted Unix domain socket via `SO_PEERCRED`.
#[allow(unsafe_code)]
fn peer_uid(stream: &UnixStream) -> Option<u32> {
    let mut cred = libc::ucred {
        pid: 0,
        uid: u32::MAX,
        gid: u32::MAX,
    };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: `stream.as_raw_fd()` is a valid open Unix socket FD, and `cred` / `len`
    // point to valid stack storage matching `SOL_SOCKET` / `SO_PEERCRED`.
    let rc = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&raw mut cred).cast::<libc::c_void>(),
            &raw mut len,
        )
    };
    if rc == 0 && len as usize == std::mem::size_of::<libc::ucred>() {
        Some(cred.uid)
    } else {
        None
    }
}

/// Resolve the numeric UID of a validated POSIX username via `getpwnam_r`.
#[allow(unsafe_code)]
fn lookup_user_uid(user: &str) -> Option<u32> {
    if !is_valid_username(user) {
        return None;
    }
    let c_user = CString::new(user).ok()?;
    let mut pwd = std::mem::MaybeUninit::<libc::passwd>::zeroed();
    let mut buf = vec![0_u8; 4096];
    let mut result: *mut libc::passwd = std::ptr::null_mut();
    // SAFETY: `c_user` is NUL-terminated, `pwd` and `buf` are valid writable buffers.
    let rc = unsafe {
        libc::getpwnam_r(
            c_user.as_ptr(),
            pwd.as_mut_ptr(),
            buf.as_mut_ptr().cast::<libc::c_char>(),
            buf.len(),
            &raw mut result,
        )
    };
    if rc == 0 && !result.is_null() {
        // SAFETY: `getpwnam_r` returned 0 with non-null `result` pointing to initialized `pwd`.
        let pw = unsafe { pwd.assume_init() };
        Some(pw.pw_uid)
    } else {
        None
    }
}

/// Verify a user's system passphrase without `pam_rootok` bypass (`SEC-003`).
///
/// Uses `unix_chkpwd <user> nonull` (which verifies against `/etc/shadow` via `crypt`
/// even when invoked as UID 0, never `/bin/su`). Fails closed if `unix_chkpwd` is absent.
fn verify_pam_passphrase(user: &str, passphrase: &str) -> bool {
    if !is_valid_username(user) || passphrase.is_empty() || passphrase.contains('\0') {
        return false;
    }
    let chkpwd = ["/usr/bin/unix_chkpwd", "/sbin/unix_chkpwd"]
        .into_iter()
        .find(|p| Path::new(p).is_file());
    let Some(chkpwd_path) = chkpwd else {
        eprintln!(
            "rabbiz-auth-helper: unix_chkpwd not found; failing closed for passphrase auth"
        );
        return false;
    };
    let Ok(mut child) = Command::new(chkpwd_path)
        .args([user, "nonull"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    if let Some(mut stdin) = child.stdin.take() {
        let mut buf = Vec::with_capacity(passphrase.len() + 1);
        buf.extend_from_slice(passphrase.as_bytes());
        buf.push(0);
        let _ = stdin.write_all(&buf);
        buf.zeroize();
    }
    child.wait().is_ok_and(|s| s.success())
}

fn handle_request(req: &HelperRequest, caller_uid: u32) -> HelperResponse {
    if !is_valid_username(&req.user) {
        return HelperResponse::Unavailable;
    }
    // Enforce SO_PEERCRED: only root (UID 0) or the target user themselves may verify.
    if caller_uid != 0 {
        let Some(target_uid) = lookup_user_uid(&req.user) else {
            return HelperResponse::Unavailable;
        };
        if caller_uid != target_uid {
            eprintln!(
                "rabbiz-auth-helper: rejecting peer uid {caller_uid} for target user '{}'",
                req.user
            );
            return HelperResponse::Unavailable;
        }
    }

    // Disallow client-supplied Swipe method from ever succeeding or resetting lockout/BFU state.
    if req.method == LockMethod::Swipe {
        return HelperResponse::Unavailable;
    }

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

    // Enforce the root-stored credential in `/var/lib/rabbiz-auth/<user>/keyguard.json`:
    // if `keyguard.json` exists, always verify against it (client cannot downgrade or
    // switch methods to bypass a configured PIN credential).
    let ok = if cred_path.exists() {
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
    } else {
        match req.method {
            LockMethod::Passphrase => verify_pam_passphrase(&req.user, &req.secret),
            LockMethod::Swipe | LockMethod::Pin | LockMethod::BiometricWithFallback => {
                return HelperResponse::Unavailable;
            }
        }
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
    let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
    let _ = stream.set_write_timeout(Some(IO_TIMEOUT));

    let Some(caller_uid) = peer_uid(&stream) else {
        return;
    };

    let mut reader = BufReader::new((&stream).take(MAX_REQUEST_BYTES));
    let mut line = String::new();
    let read_res = reader.read_line(&mut line);
    if read_res.is_err() || !line.ends_with('\n') {
        line.zeroize();
        return;
    }
    let resp = match serde_json::from_str::<HelperRequest>(line.trim()) {
        Ok(req) => handle_request(&req, caller_uid),
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

fn bind_socket_atomic(path: &Path) -> std::io::Result<UnixListener> {
    let _ = std::fs::remove_file(path);
    // Set umask 0o007 around bind() so the socket is created 0660 atomically with
    // no window where other users could connect before chmod (`SVC-005`).
    #[allow(unsafe_code)]
    let old_umask = unsafe { libc::umask(0o007) };
    let res = UnixListener::bind(path);
    #[allow(unsafe_code)]
    unsafe {
        libc::umask(old_umask);
    }
    let listener = res?;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o660));
    Ok(listener)
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
    let listener = match bind_socket_atomic(path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("rabbiz-auth-helper: bind {}: {e}", path.display());
            std::process::exit(1);
        }
    };
    let active = Arc::new(AtomicUsize::new(0));
    for conn in listener.incoming().flatten() {
        let prev = active.fetch_add(1, Ordering::AcqRel);
        if prev >= MAX_CONCURRENT_CLIENTS {
            active.fetch_sub(1, Ordering::AcqRel);
            drop(conn);
            continue;
        }
        let active_cl = Arc::clone(&active);
        if std::thread::Builder::new()
            .name("rabbiz-auth-client".into())
            .spawn(move || {
                handle_client(conn);
                active_cl.fetch_sub(1, Ordering::AcqRel);
            })
            .is_err()
        {
            active.fetch_sub(1, Ordering::AcqRel);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swipe_method_is_always_rejected() {
        let req = HelperRequest {
            user: "rabbiz".to_string(),
            method: LockMethod::Swipe,
            secret: String::new(),
        };
        assert_eq!(handle_request(&req, 0), HelperResponse::Unavailable);
    }

    #[test]
    fn invalid_username_is_rejected() {
        for bad in ["", "-root", "--help", "../etc", "a/b", "root\0x"] {
            let req = HelperRequest {
                user: bad.to_string(),
                method: LockMethod::Passphrase,
                secret: "pw".to_string(),
            };
            assert_eq!(handle_request(&req, 0), HelperResponse::Unavailable);
        }
    }
}
