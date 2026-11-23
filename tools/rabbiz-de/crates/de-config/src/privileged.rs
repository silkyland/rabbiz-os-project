//! Who may use the privileged Wayland protocols (see `docs/DE-PRIVILEGED-PROTOCOLS.md`, option B).
//!
//! Default-deny: a client that is not on the list does not even see the global. The list is matched
//! against the resolved executable path (`/proc/<pid>/exe`), never against `argv[0]`. The built-in list
//! can only be **extended** by `/etc/rabbiz-de/privileged-clients.toml`, so a typo in that file can never
//! lock out the on-screen keyboard or the mirror, and a malformed file falls back to the built-in list.
//!
//! This stops careless or naive programs. It does not stop a determined program of the same Unix user
//! (it can run an allowed binary as a proxy); see the document for the real isolation options.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::Path;

use serde::Deserialize;

/// A protocol that lets a client see or control other clients.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Privileged {
    /// `zwp_virtual_keyboard_v1`: type into the focused client.
    VirtualKeyboard,
    /// `zwlr_virtual_pointer_v1`: move and click.
    VirtualPointer,
    /// `zwlr_screencopy_v1`: read the screen.
    Screencopy,
    /// `zwlr_data_control` / `ext_data_control`: read and set the clipboard at any time.
    DataControl,
    /// `zwlr_layer_shell_v1`: overlays and exclusive keyboard grabs.
    LayerShell,
    /// `ext-session-lock-v1`: external session lock screens.
    SessionLock,
}

impl Privileged {
    /// Every protocol covered by the policy.
    pub const ALL: [Privileged; 6] = [
        Privileged::VirtualKeyboard,
        Privileged::VirtualPointer,
        Privileged::Screencopy,
        Privileged::DataControl,
        Privileged::LayerShell,
        Privileged::SessionLock,
    ];

    /// Key used in the policy file and in log lines.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Privileged::VirtualKeyboard => "virtual_keyboard",
            Privileged::VirtualPointer => "virtual_pointer",
            Privileged::Screencopy => "screencopy",
            Privileged::DataControl => "data_control",
            Privileged::LayerShell => "layer_shell",
            Privileged::SessionLock => "session_lock",
        }
    }
}

impl fmt::Display for Privileged {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.key())
    }
}

/// Path of the optional extension file.
pub const POLICY_FILE: &str = "/etc/rabbiz-de/privileged-clients.toml";

/// Executables allowed per protocol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Policy {
    allow: BTreeMap<Privileged, BTreeSet<String>>,
}

/// Why a policy file was not used.
#[derive(Debug, PartialEq, Eq)]
pub enum PolicyError {
    /// Not valid TOML for the expected shape.
    Parse(String),
    /// A key under `[allow]` is not a protocol name.
    UnknownProtocol(String),
    /// A listed path is not absolute.
    RelativePath(String),
}

impl fmt::Display for PolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PolicyError::Parse(e) => write!(f, "cannot parse policy: {e}"),
            PolicyError::UnknownProtocol(k) => write!(f, "unknown protocol `{k}` in [allow]"),
            PolicyError::RelativePath(p) => write!(f, "`{p}` is not an absolute path"),
        }
    }
}

#[derive(Deserialize)]
struct FileShape {
    #[serde(default)]
    allow: BTreeMap<String, Vec<String>>,
}

impl Policy {
    /// The built-in list: least-privilege defaults for shipped OSKs only.
    ///
    /// General-purpose CLIs (`grim`, `wl-copy`, `wl-paste`, `wayvnc`) are NOT
    /// allow-listed on the public socket; compositor-owned helpers receive a
    /// private `WAYLAND_SOCKET` with only the globals they need.
    #[must_use]
    pub fn builtin() -> Self {
        let osk = [
            "/usr/local/bin/rabbiz-osk",
            "/usr/local/bin/a21s-osk",
            "/usr/bin/squeekboard",
            "/usr/local/bin/squeekboard",
            "/usr/bin/stevia",
            "/usr/local/bin/stevia",
            "/usr/bin/maliit-keyboard",
            "/usr/bin/fcitx5",
        ];
        let mut allow: BTreeMap<Privileged, BTreeSet<String>> = BTreeMap::new();
        let mut put = |g: Privileged, paths: &[&str]| {
            allow
                .entry(g)
                .or_default()
                .extend(paths.iter().map(|p| (*p).to_string()));
        };
        put(Privileged::VirtualKeyboard, &osk);
        put(Privileged::VirtualPointer, &[]);
        put(Privileged::Screencopy, &[]);
        put(Privileged::DataControl, &[]);
        put(Privileged::LayerShell, &osk);
        put(Privileged::SessionLock, &[]);
        Self { allow }
    }

    /// Built-in list extended by a policy file's contents.
    pub fn extended_with(toml_text: &str) -> Result<Self, PolicyError> {
        let shape: FileShape =
            toml::from_str(toml_text).map_err(|e| PolicyError::Parse(e.to_string()))?;
        let mut policy = Self::builtin();
        for (key, paths) in shape.allow {
            let Some(g) = Privileged::ALL.into_iter().find(|g| g.key() == key) else {
                return Err(PolicyError::UnknownProtocol(key));
            };
            for path in paths {
                if !Path::new(&path).is_absolute() {
                    return Err(PolicyError::RelativePath(path));
                }
                policy.allow.entry(g).or_default().insert(path);
            }
        }
        Ok(policy)
    }

    /// Load [`POLICY_FILE`]-style content from `path`. A missing file is the built-in list; a malformed
    /// file is the built-in list plus the returned message (never "allow all").
    #[must_use]
    pub fn load(path: &Path) -> (Self, Option<String>) {
        match std::fs::read_to_string(path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Self::builtin(), None),
            Err(e) => (
                Self::builtin(),
                Some(format!("cannot read {}: {e}", path.display())),
            ),
            Ok(text) => match Self::extended_with(&text) {
                Ok(p) => (p, None),
                Err(e) => (Self::builtin(), Some(format!("{}: {e}", path.display()))),
            },
        }
    }

    /// Whether the executable may use the protocol. An unknown executable (the process already exited, or
    /// `/proc` is unreadable) is refused: the policy fails closed.
    #[must_use]
    pub fn allows(&self, g: Privileged, exe: Option<&Path>) -> bool {
        let Some(exe) = exe.and_then(Path::to_str) else {
            return false;
        };
        // The kernel appends " (deleted)" when the binary was replaced while running.
        let exe = exe.strip_suffix(" (deleted)").unwrap_or(exe);
        self.allow.get(&g).is_some_and(|set| set.contains(exe))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[allow(clippy::unnecessary_wraps)] // the policy API takes `Option<&Path>`; tests always have one
    fn p(s: &str) -> Option<PathBuf> {
        Some(PathBuf::from(s))
    }

    #[test]
    fn builtin_lets_the_shipped_tools_use_what_they_need() {
        let pol = Policy::builtin();
        for osk in [
            "/usr/local/bin/rabbiz-osk",
            "/usr/local/bin/a21s-osk",
            "/usr/bin/fcitx5",
            "/usr/bin/maliit-keyboard",
        ] {
            assert!(pol.allows(Privileged::VirtualKeyboard, p(osk).as_deref()));
            assert!(pol.allows(Privileged::LayerShell, p(osk).as_deref()));
            assert!(!pol.allows(Privileged::Screencopy, p(osk).as_deref()));
            assert!(!pol.allows(Privileged::DataControl, p(osk).as_deref()));
            assert!(!pol.allows(Privileged::VirtualPointer, p(osk).as_deref()));
            assert!(!pol.allows(Privileged::SessionLock, p(osk).as_deref()));
        }
    }

    #[test]
    fn general_purpose_clis_are_refused_by_builtin_policy() {
        let pol = Policy::builtin();
        for cli in [
            "/usr/bin/grim",
            "/usr/bin/wl-paste",
            "/usr/bin/wl-copy",
            "/usr/bin/wayvnc",
        ] {
            for g in Privileged::ALL {
                assert!(
                    !pol.allows(g, p(cli).as_deref()),
                    "{cli} should not get {g}"
                );
            }
        }
    }

    #[test]
    fn unlisted_programs_are_refused_for_every_protocol() {
        let pol = Policy::builtin();
        for g in Privileged::ALL {
            assert!(
                !pol.allows(g, p("/usr/bin/google-chrome").as_deref()),
                "{g}"
            );
            assert!(!pol.allows(g, p("/home/rabbiz/evil").as_deref()), "{g}");
        }
    }

    #[test]
    fn each_tool_gets_only_its_own_protocols() {
        let pol = Policy::builtin();
        assert!(!pol.allows(Privileged::Screencopy, p("/usr/bin/fcitx5").as_deref()));
        assert!(!pol.allows(Privileged::VirtualPointer, p("/usr/bin/fcitx5").as_deref()));
        assert!(!pol.allows(Privileged::DataControl, p("/usr/bin/fcitx5").as_deref()));
        assert!(!pol.allows(
            Privileged::SessionLock,
            p("/usr/local/bin/rabbiz-osk").as_deref()
        ));
    }

    #[test]
    fn a_renamed_copy_of_an_allowed_tool_is_not_allowed() {
        let pol = Policy::builtin();
        assert!(!pol.allows(Privileged::LayerShell, p("/tmp/rabbiz-osk").as_deref()));
        assert!(!pol.allows(
            Privileged::LayerShell,
            p("/usr/local/bin/rabbiz-osk-copy").as_deref()
        ));
    }

    #[test]
    fn an_unknown_executable_fails_closed() {
        assert!(!Policy::builtin().allows(Privileged::Screencopy, None));
    }

    #[test]
    fn a_replaced_binary_keeps_its_permission() {
        let pol = Policy::builtin();
        assert!(pol.allows(
            Privileged::LayerShell,
            p("/usr/local/bin/rabbiz-osk (deleted)").as_deref()
        ));
    }

    #[test]
    fn the_file_can_only_add_never_remove() {
        let pol = Policy::extended_with(
            "[allow]\nscreencopy = [\"/usr/bin/wf-recorder\"]\nsession_lock = [\"/usr/bin/rabbiz-lock\"]\n",
        )
        .unwrap();
        assert!(pol.allows(Privileged::Screencopy, p("/usr/bin/wf-recorder").as_deref()));
        assert!(pol.allows(
            Privileged::SessionLock,
            p("/usr/bin/rabbiz-lock").as_deref()
        ));
        assert!(
            pol.allows(
                Privileged::LayerShell,
                p("/usr/local/bin/rabbiz-osk").as_deref()
            ),
            "built-ins stay"
        );
        assert!(!pol.allows(
            Privileged::VirtualKeyboard,
            p("/usr/bin/wf-recorder").as_deref()
        ));
    }

    #[test]
    fn bad_files_are_rejected_with_a_reason() {
        assert!(matches!(
            Policy::extended_with("allow = 3"),
            Err(PolicyError::Parse(_))
        ));
        assert_eq!(
            Policy::extended_with("[allow]\nscreenshots = [\"/usr/bin/x\"]"),
            Err(PolicyError::UnknownProtocol("screenshots".into()))
        );
        assert_eq!(
            Policy::extended_with("[allow]\nscreencopy = [\"grim\"]"),
            Err(PolicyError::RelativePath("grim".into()))
        );
    }

    #[test]
    fn loading_falls_back_to_the_builtin_list() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("none.toml");
        let (pol, warn) = Policy::load(&missing);
        assert_eq!(pol, Policy::builtin());
        assert!(warn.is_none(), "a missing file is normal");

        let bad = dir.path().join("bad.toml");
        std::fs::write(&bad, "[allow]\nscreencopy = [\"relative\"]").unwrap();
        let (pol, warn) = Policy::load(&bad);
        assert_eq!(
            pol,
            Policy::builtin(),
            "a malformed file never means allow-all"
        );
        assert!(warn.unwrap().contains("not an absolute path"));

        let good = dir.path().join("good.toml");
        std::fs::write(&good, "[allow]\nscreencopy = [\"/usr/bin/wf-recorder\"]").unwrap();
        let (pol, warn) = Policy::load(&good);
        assert!(warn.is_none());
        assert!(pol.allows(Privileged::Screencopy, p("/usr/bin/wf-recorder").as_deref()));
    }
}
