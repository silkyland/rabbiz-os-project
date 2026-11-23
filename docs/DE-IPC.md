# RFC: `rabbiz-de` Runtime Shell Control IPC (Tier 1.6)

- **Lane**: `de`
- **Status**: Implemented (v1 — all 7 commands wired in `de-config`/`de-core`/`de-bin`; step 4 script migration pending)
- **Parent handoff**: [HANDOFF-de-audit-2026-11-19.md](HANDOFF-de-audit-2026-11-19.md) §3 Pillar 6 & §4 Tier 1.6

---

## 1. Problem

Today `rabbiz-de` has no runtime control bus. External components (hardware key daemons, `rabbiz-mirror`, Settings, test harnesses, and power scripts) interact with the shell via ad-hoc files (`a21s-power.cmd`, ready files, frame-dump triggers) or by spawning new CLI processes.

This creates race conditions, makes state inspection hard in automated tests, and blocks clean integration with Settings and companion tools.

---

## 2. Design Principles

1. **No async runtime in `de-core`**: Matches [DE-SEAMS.md](DE-SEAMS.md) — `de-core` runs a synchronous calloop/poll event loop.
2. **Typed JSON-lines over a user-scoped Unix domain socket** (`$XDG_RUNTIME_DIR/rabbiz-de-ctl.sock`, mode `0600` created atomically with a `0177` umask; IPC is disabled if `XDG_RUNTIME_DIR` is unset — never falls back to `/tmp`), with optional D-Bus bridge later in `de-system`.
   - Zero extra crate dependencies (`std::os::unix::net::UnixListener` + `serde_json`, both already in the workspace).
   - Trivial to test from `de-smoke`, Python, shell scripts (`socat`), or `rabbiz-mirror`.
3. **Fail-closed lock & backend guards**:
   - While Keyguard or `ext-session-lock-v1` is active, only `get_state`, `lock`, `reload_theme`, `reload_apps`, `show_osd`, and `set_panel` (`open: false`) are permitted.
   - `set_mock` and `inject_*` (`inject_touch`, `inject_key`, `inject_hw_button`) are compiled behind the `emu` feature, restricted to `--backend=virtual`, and refused while locked; no IPC command can unlock the session.
   - Optional TCP listeners (`--ctl-listen`, `--emu-listen`) require `--backend=virtual` and reject non-loopback addresses.

---

## 3. Protocol Sketch (v1)

Line-delimited JSON request/response (`\n`-terminated, max 16 KiB per message):

### Commands (`Client -> Compositor`)
```json
{"id": 1, "cmd": "get_state"}
{"id": 2, "cmd": "lock"}
{"id": 3, "cmd": "launch", "desktop_id": "org.kde.kalk"}
{"id": 4, "cmd": "screenshot", "save": true}
{"id": 5, "cmd": "show_osd", "kind": "volume", "percent": 65, "muted": false}
{"id": 6, "cmd": "reload_theme"}
{"id": 7, "cmd": "set_panel", "panel": "quick", "open": true}
```

### Responses (`Compositor -> Client`)
```json
{"id": 1, "ok": true, "data": {
  "foreground": "shell",
  "screen": "home",
  "locked": false,
  "output": {"w": 720, "h": 1600},
  "insets": {"ime_bottom": 0},
  "windows": [{"app_id": "org.kde.kalk", "focused": false}]
}}
```

Error shape:
```json
{"id": 2, "ok": false, "error": "permission_denied"}
```

---

## 4. Crate Placement (Respecting `check_seams.py`)

- **Wire types (`IpcRequest`, `IpcResponse`, `ShellStateSnapshot`)**: `de-config::ipc` (keeps frozen `de-api` untouched in v1).
- **Socket listener & dispatch**: `de-core/src/runtime/ctl_ipc.rs`, polled non-blocking inside the main event loop (`0600` user-scoped socket; optional `SO_PEERCRED` check via `de-core/src/protocols/privileged.rs` planned).
- **CLI client subcommand**: `rabbiz-de ctl <command>` in `de-bin/src/main.rs` (replaces ad-hoc trigger files while keeping legacy `a21s-power.cmd` as a compatibility shim until hardware key daemons migrate).

---

## 5. Rollout Order

1. [x] Add `de-config::ipc` wire structs + round-trip unit tests (host-tested on macOS/Linux).
2. [x] Add non-blocking socket listener in `de-core/src/runtime/ctl_ipc.rs` (`get_state`, `lock`, `launch`, `screenshot`, `show_osd`, `reload_theme`, `set_panel`).
3. [x] Add `rabbiz-de ctl` subcommand in `de-bin`.
4. [ ] Migrate `de-smoke` and hardware key / mirror scripts to `rabbiz-de ctl`, keeping `a21s-power.cmd` fallback for one release cycle.
