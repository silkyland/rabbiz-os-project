//! Default-deny policy for the privileged Wayland globals (`docs/DE-PRIVILEGED-PROTOCOLS.md`, option B).
//!
//! A client is identified once, when its socket is accepted: peer credentials from the kernel and the
//! resolved `/proc/<pid>/exe`. Each filtered global then asks [`allowed`]. A refused client simply does
//! not see the global, and the refusal is logged once per client and protocol. The session keyboard,
//! which the compositor spawns over a private socket, is always trusted.

use std::collections::HashSet;
use std::io;
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};

use de_config::privileged::POLICY_FILE;
use de_config::{Policy, Privileged};
use smithay::reexports::wayland_server::{Client, DisplayHandle};

use crate::state::DeClientState;

/// Who a connected client is, as far as the kernel can tell us.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientIdentity {
    /// Process id of the peer when it connected.
    pub pid: i32,
    /// Resolved executable, `None` if `/proc` could not be read (the process may already be gone).
    pub exe: Option<PathBuf>,
}

static POLICY: OnceLock<Policy> = OnceLock::new();
static LOGGED: OnceLock<Mutex<HashSet<(String, Privileged)>>> = OnceLock::new();

/// Load the policy once at startup. `RABBIZ_DE_PRIVILEGED_POLICY` overrides the file path (tests, bring-up).
pub(crate) fn init_policy() {
    let path = std::env::var_os("RABBIZ_DE_PRIVILEGED_POLICY")
        .map_or_else(|| PathBuf::from(POLICY_FILE), PathBuf::from);
    let (policy, warning) = Policy::load(&path);
    if let Some(w) = warning {
        eprintln!("rabbiz-de: privileged-client policy: {w}; using the built-in list");
    }
    let _ = POLICY.set(policy);
    eprintln!(
        "rabbiz-de: privileged-client policy active (default-deny; set RABBIZ_DE_POLICY_LOG=1 to log which \
         executables are hidden which protocol)"
    );
}

/// Every client that merely lists the registry is checked against every privileged global, so a line per
/// check would say "denied" for programs that never wanted the protocol. Logging is therefore opt-in.
fn log_enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var_os("RABBIZ_DE_POLICY_LOG").is_some_and(|v| v != "0" && !v.is_empty())
    })
}

fn policy() -> &'static Policy {
    POLICY.get_or_init(Policy::builtin)
}

/// Resolve the executable of `pid`, if readable.
fn exe_of(pid: i32) -> Option<PathBuf> {
    std::fs::read_link(Path::new("/proc").join(pid.to_string()).join("exe")).ok()
}

/// Record who `client` is. Call right after the socket is accepted, before any request is dispatched.
pub(crate) fn identify(dh: &DisplayHandle, client: &Client, state: &DeClientState) {
    let identity = match client.get_credentials(dh) {
        Ok(cred) => ClientIdentity {
            pid: cred.pid,
            exe: exe_of(cred.pid),
        },
        Err(_) => ClientIdentity { pid: -1, exe: None },
    };
    let _ = state.identity.set(identity);
}

/// May `client` use the privileged global `g`?
pub(crate) fn allowed(client: &Client, g: Privileged) -> bool {
    let Some(state) = client.get_data::<DeClientState>() else {
        return false;
    };
    allowed_for_state(state, g)
}

fn allowed_for_state(state: &DeClientState, g: Privileged) -> bool {
    if state.session_input_method {
        return matches!(g, Privileged::VirtualKeyboard | Privileged::LayerShell);
    }
    if !state.private_grants.is_empty() {
        return state.private_grants.contains(&g);
    }
    let identity = state.identity.get();
    let ok = policy().allows(g, identity.and_then(|i| i.exe.as_deref()));
    if !ok {
        log_denied(g, identity);
    }
    ok
}

/// Spawn a compositor-trusted helper over a private `WAYLAND_SOCKET` pair,
/// granting it only `grants` (never path-based allow-listing on the public socket).
#[allow(dead_code)] // Used when compositor spawns screenshot/mirror/clipboard helpers
pub(crate) fn spawn_private_helper(
    display: &mut DisplayHandle,
    socket_name: &str,
    program: &str,
    args: &[&str],
    grants: Vec<Privileged>,
) -> io::Result<Child> {
    let (server, client) = UnixStream::pair()?;
    display.insert_client(
        server,
        Arc::new(DeClientState {
            private_grants: grants,
            ..DeClientState::default()
        }),
    )?;
    Command::new(program)
        .args(args)
        .stdin(Stdio::from(OwnedFd::from(client)))
        .env("WAYLAND_SOCKET", "0")
        .env("WAYLAND_DISPLAY", socket_name)
        .spawn()
}

fn log_denied(g: Privileged, identity: Option<&ClientIdentity>) {
    if !log_enabled() {
        return;
    }
    let pid = identity.map_or(-1, |i| i.pid);
    let exe = identity
        .and_then(|i| i.exe.as_deref())
        .map_or_else(|| "<unknown>".to_string(), |p| p.display().to_string());
    let first = LOGGED
        .get_or_init(|| Mutex::new(HashSet::new()))
        .lock()
        .is_ok_and(|mut set| set.insert((exe.clone(), g)));
    if first {
        eprintln!("rabbiz-de: policy hides {g} from {exe} (pid {pid})");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_osk_gets_only_virtual_keyboard_and_layer_shell() {
        let state = DeClientState {
            session_input_method: true,
            ..DeClientState::default()
        };
        assert!(allowed_for_state(&state, Privileged::VirtualKeyboard));
        assert!(allowed_for_state(&state, Privileged::LayerShell));
        assert!(!allowed_for_state(&state, Privileged::Screencopy));
        assert!(!allowed_for_state(&state, Privileged::VirtualPointer));
        assert!(!allowed_for_state(&state, Privileged::DataControl));
        assert!(!allowed_for_state(&state, Privileged::SessionLock));
    }

    #[test]
    fn private_socket_helper_gets_only_its_granted_globals() {
        let state = DeClientState {
            private_grants: vec![Privileged::Screencopy],
            ..DeClientState::default()
        };
        assert!(allowed_for_state(&state, Privileged::Screencopy));
        assert!(!allowed_for_state(&state, Privileged::VirtualKeyboard));
        assert!(!allowed_for_state(&state, Privileged::VirtualPointer));
        assert!(!allowed_for_state(&state, Privileged::DataControl));
        assert!(!allowed_for_state(&state, Privileged::LayerShell));
        assert!(!allowed_for_state(&state, Privileged::SessionLock));
    }
}
