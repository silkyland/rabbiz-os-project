# DE: who may use the privileged Wayland protocols (proposal, needs an owner decision)

Status: **proposal**. Nothing here is enabled. Lane: `de`. Written 2026-11-18. Security rules change only with
an explicit owner decision, so this document ends with the questions the owner has to answer.

## 1. Problem

`rabbiz-de` registers several protocols that let a client see or control other clients. Today most of them
are open to **every** client that can open the Wayland socket:

| Global | Who can bind today | What a client can do with it | Legitimate users |
|---|---|---|---|
| `zwp_input_method_v2` | only the session keyboard (private socket + filter) | become the input method | `rabbiz-osk` |
| `zwp_virtual_keyboard_v1` | **any client** (`\|_\| true` in `de-core/state.rs`) | type into whatever has focus | `rabbiz-osk`, `wayvnc` |
| `zwlr_virtual_pointer_v1` | **any client** (no filter) | move and click | `wayvnc` |
| `zwlr_screencopy_v1` | **any client** (no filter) | read the whole screen, including the lock screen | `grim`, `rabbiz-screenshot`, `wayvnc`, `rabbiz-screenrec` (planned) |
| `zwlr_data_control` / `ext_data_control` | **any client** (`\|_\| true`) | read and set the clipboard at any time | the keyboard's `wl-paste --watch`, `wl-clipboard` |
| `zwlr_layer_shell_v1` | **any client** | draw above everything and, with exclusive keyboard interactivity, take the keyboard | `rabbiz-osk` (and future shell panels) |

The wlroots desktops ship the same openness and trust every program of the logged-in user. A phone differs:
it runs many third-party apps and a browser, the screen shows banking and chat content, and the lock screen
is a security boundary. An app that can screenshot, inject keys or hold an exclusive overlay can read what the
user sees, type for them, or draw a fake password prompt.

## 2. What is technically possible here

- `rabbiz-de` can identify a client at accept time: `Client::get_credentials()` returns pid, uid and gid of the
  peer, and `/proc/<pid>/exe` gives the real executable path (it cannot be faked through `argv[0]`).
  Clients of the public socket are created in `de-core/frame.rs` with `DeClientState::default()`; that is the
  place to attach the identity.
- Smithay's filtered globals (`virtual_keyboard`, `data_control`, `input_method`) take a closure over the
  client. The hand-written globals (`screencopy`, `virtual_pointer`) can add a `can_view` implementation.
- The session keyboard already uses the strongest form available without sandboxing: it connects over a
  private socket pair created by the compositor.

**The hard limit.** Every app runs as the same Unix user (`rabbiz`) as the system helpers. Same-user code can
launch an allowed binary itself (`grim`, `wayvnc`, `wl-paste`) and use it as a proxy, can `ptrace` or preload
into one when the kernel permits it, and can connect to any socket path the user owns. Path allow-lists
therefore raise the bar against careless or naive abuse; they do **not** stop a determined same-user program.
Real isolation needs a different user or a sandbox for third-party apps.

## 3. A bypass that no compositor policy covers

`a21s-wayvnc` listens on `172.16.42.1:5900` and `127.0.0.1:5900` and offers VNC security type **None**
([finding](../devices/samsung/a21s/findings/2026-11-18-de-wayvnc-unauthenticated-localhost.md)). Any local app
can connect to loopback, receive the whole screen and send pointer and keyboard events. This makes every
compositor allow-list moot until the mirror's port is closed to local apps or authenticated.

## 4. Options

| | Option | Gains | Costs and limits |
|---|---|---|---|
| A | Keep it open (today) | nothing breaks | any app can spy and inject; not suitable for a phone with third-party apps |
| B | Default-deny allow-list by executable path, read from `/etc/rabbiz-de/privileged-clients.toml`, with a log line for every refusal | cheap, testable, audit trail, stops accidental or naive abuse | bypassable by same-user proxying (section 2); needs entries for every legitimate tool |
| C | Two sockets: a restricted one for apps the shell launches (no privileged globals) and a privileged one for system services | clear model, works with `WAYLAND_SOCKET` per child like the keyboard | only strong if services and apps run as different users; today they do not |
| D | `wp_security_context_v1` plus sandboxed apps (bubblewrap or Flatpak) | the standard mechanism; per-app restricted socket | needs app sandboxing, a large piece of work; smithay 0.7 has no helper, so it would be hand-written |
| E | User-consent portals for capture and recording (xdg-desktop-portal ScreenCast/Screenshot with a shell-owned dialog) | the user approves each use, like a phone OS; only the portal backend holds screencopy | needs the portal work of M5 and an app ecosystem that uses portals |

## 5. Recommendation (a hypothesis to be challenged)

1. Do **B now** for all privileged globals, plus restrict exclusive-keyboard layer surfaces to the allow-list.
   It is a day of work and gives the log of who asks for what, which we need to decide the rest with data.
2. Close the loopback hole in the mirror: remove the `127.0.0.1` listener unless something needs it, and
   require a VNC password (type 2) with the secret in a root-readable file. Until then B protects less than
   it appears to.
3. Build screen recording (Package 4) so that the **shell** starts it and shows an indicator that apps cannot
   hide; third-party recorders stay refused unless allow-listed. This is the first step toward E.
4. Treat C or D as the real fix for third-party apps and decide it with the M5 sandboxing discussion.

What breaks with B: any tool not on the list is refused (for example a user-installed `wf-recorder` or
`wlrctl`); the user sees nothing happen, and the log says why. The default list would be: `rabbiz-osk`,
`wayvnc`, `grim`, `rabbiz-screenshot`, `rabbiz-screenrec`, `wl-paste`, `wl-copy`.

## 6. Questions for the owner

1. Accept default-deny with an allow-list (option B) as the default? If yes, may unlisted tools be refused
   silently apart from the log?
2. Mirror: drop the loopback listener and add a VNC password, or keep it open for the USB-only convenience?
3. Should app-launched clients be forbidden from exclusive keyboard layer surfaces (this blocks fake lock
   or password overlays), even if it breaks a future third-party on-screen keyboard?
4. Appetite for C/D (separate user or sandbox for third-party apps) as a later milestone?

## 7. Implementation sketch once decided

1. `DeClientState` gets `exe: Option<PathBuf>` and `uid`, filled when the client is accepted.
2. One function `is_allowed(global, &DeClientState, &Policy) -> bool`, pure and unit-tested; each filter and
   `can_view` calls it and logs `denied <global> exe=<path> pid=<pid>` through one helper.
3. The policy file is read at startup; a missing file means the built-in default list above; a malformed file
   is a startup warning and the default list, never "allow all".
4. Tests: allowed exe binds, unknown exe is refused, refusal is logged, malformed policy falls back to the
   default list, the private OSK socket stays allowed.
5. Device check: OSK typing, screenshots, `a21s-wayvnc` and the mirror still work; a copy of an unlisted
   binary is refused. Record it as a finding.
