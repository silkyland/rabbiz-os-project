# Domain Audit: Framework & App Layer (`APP`)

**Workspace:** `/home/dit/Sites/Labs/android-arm-linux`  
**Audit Mode:** Read-only static source inspection (no builds, no network, no device execution)  
**Working Tree Note:** At audit time, `git status --short` reported uncommitted modifications across `tools/rabbiz-apps/` (including `rabbiz-store`, `rabbiz-settings`, `rabbiz-files`, `rabbiz-terminal`, `rabbiz-dialer`, `rabbiz-sms`, `rabbiz-pdf`, `rabbiz-app-kit`), `tools/rabbiz-de/crates/de-apps/data/launch.toml`, and untracked directories `tools/rabbiz-apps/crates/rabbiz-search/` and `tools/rabbiz-mirror/linux/`. All line numbers below reflect the working tree on disk at audit time.

---

## Scope & Files Reviewed

### Inspected Files
- `tools/rabbiz-apps/crates/rabbiz-store/src/backend.rs` (`direct-read`)
- `tools/rabbiz-apps/crates/rabbiz-store/src/main.rs` (`targeted-read` + `rg-scan`)
- `tools/rabbiz-apps/crates/rabbiz-settings/src/system.rs` (`direct-read`)
- `tools/rabbiz-apps/crates/rabbiz-settings/src/cli.rs` (`targeted-read`)
- `tools/rabbiz-apps/crates/rabbiz-files/src/main.rs` (`targeted-read` + `rg-scan`)
- `tools/rabbiz-apps/crates/rabbiz-files/src/model.rs` (`targeted-read` + `rg-scan`)
- `tools/rabbiz-apps/crates/rabbiz-terminal/src/headless.rs` (`targeted-read`)
- `tools/rabbiz-apps/crates/rabbiz-dialer/src/backend.rs` (`direct-read`)
- `tools/rabbiz-apps/crates/rabbiz-sms/src/backend.rs` (`direct-read`)
- `tools/rabbiz-apps/crates/rabbiz-pdf/Cargo.toml` (`direct-read`)
- `tools/rabbiz-apps/crates/rabbiz-pdf/src/` (`rg-scan`)
- `tools/rabbiz-apps/crates/rabbiz-app-kit/src/mcp.rs` (`direct-read`)
- `tools/rabbiz-apps/crates/rabbiz-app-kit/src/intent.rs` (`targeted-read`)
- `tools/rabbiz-apps/crates/rabbiz-app-kit/src/` (`rg-scan`)
- `tools/rabbiz-apps/crates/rabbiz-search/src/apps.rs` (`targeted-read`, untracked)
- `tools/rabbiz-apps/crates/rabbiz-search/src/web.rs` (`targeted-read`, untracked)
- `tools/rabbiz-apps/crates/rabbiz-music/src/player.rs` (`rg-scan`)
- `tools/rabbiz-sdk/crates/rabbiz-wasm-runner/src/lib.rs` (`targeted-read`)
- `tools/rabbiz-sdk/crates/rabbiz-flutter-runner/src/lib.rs` (`direct-read`)
- `tools/rabbiz-sdk/crates/rabbiz-emu-viewer/src/bin/rabbiz-emu-vnc.rs` (`direct-read`)
- `tools/rabbiz-sdk/crates/rabbiz-cli/src/lib.rs` (`direct-read`)
- `tools/rabbiz-sdk/crates/rabbiz-pack/src/lib.rs` (`direct-read`)
- `tools/rabbiz-mirror/Sources/RabbizMirror/DeviceMirrorService.swift` (`direct-read`)
- `tools/rabbiz-mirror/Sources/RabbizMirror/RFBClient.swift` (`direct-read`)
- `tools/rabbiz-mirror/linux/src/device_service.rs` (`direct-read`, untracked)
- `tools/rabbiz-mirror/linux/src/rfb.rs` (`targeted-read`, untracked)
- `tools/rabbiz-de/crates/de-apps/src/launch.rs` (`direct-read`)
- `shell/Rabbiz/Services/Network.qml` (`targeted-read`)
- `shell/Rabbiz/Services/Bluetooth.qml` (`targeted-read`)
- `shell/Rabbiz/Services/ShellPrefs.qml` (`targeted-read`)
- `shell/Rabbiz/Services/SystemControl.qml` (`targeted-read` + `rg-scan`)
- `shell/Rabbiz/Services/Brightness.qml` (`rg-scan`)
- `shell/Rabbiz/Services/Power.qml` (`rg-scan`)
- `shell/Rabbiz/Services/Media.qml` (`rg-scan`)
- `shell/Rabbiz/Services/Appearance.qml` (`rg-scan`)
- `shell/Rabbiz/Services/Telephony.qml` (`rg-scan`)

### Files NOT Inspected (and Reasons)
- `tools/rabbiz-apps/crates/rabbiz-calculator/`, `rabbiz-clock/`, `rabbiz-contacts/`, `rabbiz-notes/`, `rabbiz-photos/` — pure UI/local-model apps outside the primary focus list (only pattern-scanned via `rg` for `Command::new`/`unsafe`).
- `tools/rabbiz-sdk/crates/rabbiz-sdk-c/src/lib.rs` — 31 `unsafe` FFI wrapper blocks flagged by Phase 3 scan; not directly audited for C ABI memory safety in this pass due to time budget (recommend dedicated FFI review).
- `tools/rabbiz-mirror/Sources/RabbizMirror/ZRLEDecoder.swift`, `Framebuffer.swift`, `Inflater.swift`, `MainWindowController.swift`, `AppDelegate.swift`, and `tools/rabbiz-mirror/linux/src/zrle.rs`, `ui.rs` — pixel decoding / UI rendering layers not inspected in depth; buffer-bounds safety of ZRLE decoding in Swift (`UnsafeMutableRawPointer`) remains unverified.
- `shell/Rabbiz/Desktop/`, `shell/Rabbiz/Panels/`, `shell/Rabbiz/Windowing/`, `shell/Rabbiz/Widgets/` — QML UI views outside `Services/`.
- `tools/rabbiz-de/crates/de-apps/src/catalog.rs`, `autostart.rs`, `icons.rs`, `mime.rs`, `prefs.rs` — outside the specific `launch.rs` focus item.

---

## Findings

```
ID: APP-001
Title: Arbitrary Command Execution & Path Traversal via Unvalidated desktop_id in de_apps::launch
Severity: High
Confidence: Confirmed
Location: tools/rabbiz-de/crates/de-apps/src/launch.rs:36-52, 112-120, 151-161
Evidence:
  // lines 40-51
  let (cmdline, extra_env) = match table.overrides.get(desktop_id) {
      Some(o) => (o.exec.clone(), o.env.clone()),
      None => (desktop_exec(desktop_id), BTreeMap::new()),
  };
  let mut argv = split_shellish(&cmdline);
  ...
  spawn(&argv, desktop_id, &extra_env)

  // lines 112-120
  let log = OpenOptions::new()
      .create(true)
      .append(true)
      .open(logdir.join(format!("{log_name}.log")))
      ...
  let mut cmd = Command::new("setsid");
  cmd.arg(&argv[0]).args(&argv[1..]).stdin(Stdio::null());

  // lines 151-161
  fn desktop_exec(desktop_id: &str) -> String {
      for dir in crate::catalog::applications_dirs() {
          let f = dir.join(format!("{desktop_id}.desktop"));
          if let Ok(text) = std::fs::read_to_string(&f) {
              if let Some(exec) = parse_desktop_exec(&text) {
                  return exec;
              }
          }
      }
      desktop_id.to_string()
  }
Impact:
  1) Any caller able to supply a `desktop_id` to `launch()` (including `rabbiz-ctl launch`, the unauthenticated TCP `127.0.0.1:5911` compositor IPC, the `emu_ctl` MCP tool, or a `rabbiz://launch/...` URI intent) can execute arbitrary binaries with arbitrary arguments by passing a command string not matching a `.desktop` file (e.g., `sh -c ...`), because `desktop_exec()` falls back to returning `desktop_id` verbatim and `split_shellish()` splits it into `argv`.
  2) `desktop_id` is joined directly into `dir.join(format!("{desktop_id}.desktop"))` and `logdir.join(format!("{log_name}.log"))` without path-traversal validation, allowing `../` sequences to load arbitrary `.desktop` files outside XDG application directories or create/append arbitrary `*.log` files.
  3) `Command::new("setsid").arg(&argv[0])` omits `--`, so an `argv[0]` starting with `-` is parsed as a `setsid` option.
Exploit/Trigger:
  Invoke `de_apps::launch::launch()` (e.g., via compositor IPC `{"cmd":"launch","desktop_id":"..."}` or `rabbiz://launch/...` URI) with a `desktop_id` containing a shell command + arguments or `../` path segments.
Recommendation:
  1) Validate `desktop_id` strictly against `^[A-Za-z0-9._-]+$` (reject `/`, `\`, `..`, whitespace, and leading `-`).
  2) Remove the fallback that executes unknown `desktop_id` strings as raw commands; return `Err("unknown application")` if no override or `.desktop` entry is found.
  3) Pass `"--"` before `&argv[0]` when invoking `setsid`, or use `pre_exec` with `libc::setsid()` directly.
References: CWE-78, CWE-22, CWE-88
```

```
ID: APP-002
Title: Shell Command Injection via Unvalidated Persisted Catalog (state.json) & Unreviewed AUR Execution in rabbiz-store
Severity: High
Confidence: Confirmed
Location: tools/rabbiz-apps/crates/rabbiz-store/src/main.rs:431-447, 768-855; tools/rabbiz-apps/crates/rabbiz-store/src/backend.rs:118-120, 202-242
Evidence:
  // main.rs:431-447 — loads CatalogItem list directly from disk without validating id/name
  if let Ok(raw) = std::fs::read_to_string(&state_path) {
      if let Ok(saved) = serde_json::from_str::<PersistedStoreState>(&raw) {
          return Self { ... items: saved.items, ... };
      }
  }

  // main.rs:790-796, 837-842 — interpolates item.id directly into sh -c script
  format!("flatpak remote-add --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo && flatpak install -y --noninteractive flathub {}", item.id)
  ...
  let pkg = backend::map_arch_pkg(&item.id); // returns id unchanged for unknown ids
  let script = if want_install {
      format!("paru -S --noconfirm {pkg}")
  } else {
      format!("paru -Rns --noconfirm {pkg}")
  };
  backend::spawn_shell_job(..., script, ...);

  // backend.rs:118-120
  let spawn_res = Command::new("sh")
      .args(["-c", &script])

  // backend.rs:202-204, 227-241
  let desktop_path = apps_dir.join(format!("{id}.desktop"));
  ...
  let script = format!("#!/bin/sh\nexec rabbiz-terminal --title \"{name}\"\n");
  ...
  let desktop = format!("[Desktop Entry]\nType=Application\nName={name}\nExec={}\n...", ...);
Impact:
  1) If `$XDG_DATA_HOME/rabbiz-store/state.json` (which falls back to `/tmp/.local/share/rabbiz-store/state.json` when `HOME` is unset) is modified by another local process, backup restore, or untrusted sync, arbitrary shell metacharacters in `item.id` are executed by `sh -c` when the user or MCP (`store_install`) triggers an install/remove.
  2) In `install_curated_rbz_app_in`, `name` is interpolated unescaped into double quotes inside a generated `#!/bin/sh` wrapper script (shell command injection when launched) and into a `.desktop` file (newline key injection); `id` is joined into paths without traversal checks.
  3) Running `paru -S --noconfirm` / `paru -Syu --noconfirm` (lines 708-710, 839) skips PKGBUILD review for AUR packages; AUR PKGBUILDs are arbitrary user-submitted shell scripts executed during `makepkg`.
Exploit/Trigger:
  Modify `state.json` to contain a crafted `id` or `name` and trigger `store_install` (GUI, `--cli install`, or `--mcp`), or install/upgrade a compromised AUR package via `paru --noconfirm`.
Recommendation:
  1) Never construct `sh -c` strings with interpolated package IDs. Spawn `Command::new("flatpak")`, `Command::new("paru")`, or `Command::new("apt-get")` directly with explicit `args([...])` and `--`.
  2) Do not persist untrusted executable catalog metadata (`id`, `source`) as authoritative in `state.json`; persist only installed/update status keyed by ID and merge against the compiled/signed catalog, and validate package IDs strictly (`^[A-Za-z0-9._+-]+$`).
  3) In `install_curated_rbz_app_in`, reject `/`, `\`, `..`, and control/newline characters in `id` and `name`, and use single-quoted or `exec` argument-safe wrappers (or invoke `rabbiz-terminal` directly in `Exec=`).
  4) Restrict automated `paru` installs to official repositories (`pacman`) or require explicit user PKGBUILD review/allowlisting for AUR packages.
References: CWE-78, CWE-22, CWE-494
```

```
ID: APP-003
Title: SSH Option / Command Injection via Unvalidated host Parameter in RabbizMirror (macOS Swift & Linux Rust)
Severity: High
Confidence: Confirmed
Location: tools/rabbiz-mirror/Sources/RabbizMirror/DeviceMirrorService.swift:12-21; tools/rabbiz-mirror/linux/src/device_service.rs:17-34
Evidence:
  // DeviceMirrorService.swift:12-21
  let target = trimmed.contains("@") ? trimmed : "root@\(trimmed)"
  let process = Process()
  process.executableURL = URL(fileURLWithPath: "/usr/bin/ssh")
  process.arguments = [
      "-o", "BatchMode=yes",
      "-o", "ConnectTimeout=6",
      "-o", "StrictHostKeyChecking=accept-new",
      target,
      "rabbiz-mirror", "start"
  ]

  // tools/rabbiz-mirror/linux/src/device_service.rs:17-34 (untracked)
  let target = if trimmed.contains('@') {
      trimmed.to_string()
  } else {
      format!("root@{trimmed}")
  };
  let output = Command::new("/usr/bin/ssh")
      .args([
          "-o", "BatchMode=yes",
          "-o", "ConnectTimeout=6",
          "-o", "StrictHostKeyChecking=accept-new",
          &target,
          "rabbiz-mirror",
          "start",
      ])
Impact:
  1) Because `--` is omitted before `target`, any host string containing `@` and beginning with `-` (e.g., `-oProxyCommand=...`) is interpreted by `/usr/bin/ssh` as an option rather than a hostname, enabling arbitrary local command execution on the macOS/Linux companion host.
  2) `StrictHostKeyChecking=accept-new` trusts the first host key encountered without verification (TOFU), exposing the initial SSH connection over Wi-Fi to MITM, and defaults to logging in as `root@`.
Exploit/Trigger:
  Supply a host string starting with `-` and containing `@` in the RabbizMirror host input field (or via saved defaults/URL/paste) and click Start.
Recommendation:
  1) Insert `"--"` before `target` in the `ssh` argument vector, AND validate `host` strictly (reject leading `-`, whitespace, and unexpected characters; allow only valid `[user@]hostname-or-ip`).
  2) Prefer connecting as the unprivileged session user (`rabbiz`) rather than `root`, and surface host-key fingerprints for first-time pairing instead of silent `accept-new`.
References: CWE-88, CWE-295
```

```
ID: APP-004
Title: Missing Cryptographic Signature Verification, Path Traversal (app.id / app.exec), and .desktop Injection in .rbz Installer
Severity: High
Confidence: Confirmed
Location: tools/rabbiz-sdk/crates/rabbiz-pack/src/lib.rs:158-208, 358-426, 459-482
Evidence:
  // lib.rs:158-176 — app.id does not reject "..", app.exec does not reject absolute paths or ".."
  pub fn validate(&self) -> Result<(), String> {
      let id = self.app.id.trim();
      ...
      if id.contains('/') || id.contains('\\') || id.chars().any(char::is_whitespace) { ... }
      ...
      if self.app.exec.trim().is_empty() {
          return Err("app.exec must not be empty".to_string());
      }
      Ok(())
  }

  // lib.rs:198-207 — newlines in manifest fields are not stripped before writing .desktop
  format!(
      "[Desktop Entry]\nType=Application\nName={}\nExec={}\nIcon={}\nCategories={}\n...",
      self.app.name, install_bin_path.display(), icon_val, cats, self.app.id, ...
  )

  // lib.rs:465-479
  let install_dir = data_home.join("rabbiz").join("apps").join(app_id);
  let manifest = unpack_rbz(rbz_file, &install_dir)?;
  let bin_path = install_dir.join(&manifest.app.exec);
  if bin_path.exists() {
      set_executable_bit(&bin_path);
  }
  ...
  let desktop_content = manifest.generate_desktop_entry(&bin_path);
Impact:
  1) `verify_rbz()` only verifies SHA-256 hashes stored inside the `.rbz` file itself; there is no cryptographic signature or trusted key verification, so tampered or malicious third-party `.rbz` bundles pass verification.
  2) `app.id = ".."` passes `validate()`, escaping `data_home/rabbiz/apps/`.
  3) `manifest.app.exec` can be an absolute path (e.g., `/bin/sh`) or contain `../` — in Rust, `Path::join("/...")` replaces `install_dir` entirely, causing `set_executable_bit(&bin_path)` to `chmod +x` an arbitrary file and `generate_desktop_entry` to write an arbitrary `Exec=` path.
  4) Newlines in `app.name`, `app.icon`, `app.categories`, or `app.exec` allow injecting arbitrary `.desktop` keys (including overriding `Exec=`).
Exploit/Trigger:
  Install a crafted `.rbz` bundle via `rabbiz install` or the `emu_pack_install` MCP tool.
Recommendation:
  1) Add detached or embedded Ed25519 signature verification against a trusted keyring before unpacking/installing `.rbz` bundles.
  2) Reject `app.id == "."`, `app.id == ".."`, and restrict `app.id` to `^[A-Za-z0-9][A-Za-z0-9._-]*$`.
  3) Require `app.exec` and `app.icon` to be relative paths without `..`, leading `/`, `\`, or control characters, and verify that the resolved `bin_path` stays inside `install_dir`.
  4) Reject `\r` and `\n` in all manifest fields interpolated into `.desktop` files.
References: CWE-347, CWE-22, CWE-93
```

```
ID: APP-005
Title: QEMU Tier 3 Emulator Exposes Unauthenticated Control IPC (5911), Framebuffer (5910), and SSH (2222) on All Network Interfaces (0.0.0.0)
Severity: High
Confidence: Confirmed
Location: tools/rabbiz-sdk/crates/rabbiz-cli/src/lib.rs:583-587
Evidence:
  "-netdev".to_string(),
  format!(
      "user,id=net0,hostfwd=tcp::{}-:5910,hostfwd=tcp::{}-:5911,hostfwd=tcp::{}-:22",
      profile.emu_port, profile.ctl_port, profile.ssh_port
  ),
Impact:
  In QEMU SLIRP `hostfwd=tcp::PORT-:GUEST_PORT`, omitting the host IP binds the forwarded socket to `0.0.0.0` (all interfaces). Any host on the same LAN/Wi-Fi network can view the emulator screen (port 5910), inject arbitrary touch/keyboard events and send `launch` IPC commands (port 5911 — chained with APP-001 for remote command execution inside the VM), and reach SSH (port 2222).
Exploit/Trigger:
  Launch the Tier 3 QEMU VM using the command generated by `build_qemu_vm_command()` on a developer machine connected to a shared network.
Recommendation:
  Explicitly bind all `hostfwd` entries to loopback:
  `hostfwd=tcp:127.0.0.1:{}-:5910,hostfwd=tcp:127.0.0.1:{}-:5911,hostfwd=tcp:127.0.0.1:{}-:22`.
References: CWE-668, CWE-306
```

```
ID: APP-006
Title: Cleartext Unauthenticated VNC (RFB Security Type None) in RabbizMirror & rabbiz-emu-vnc, plus Unbounded Allocations in Swift RFB Client
Severity: Medium
Confidence: Confirmed
Location: tools/rabbiz-mirror/Sources/RabbizMirror/RFBClient.swift:27, 294-298, 308-311, 406-407, 492-494; tools/rabbiz-mirror/linux/src/rfb.rs:368-375; tools/rabbiz-sdk/crates/rabbiz-emu-viewer/src/bin/rabbiz-emu-vnc.rs:19-34, 50-54, 130-132
Evidence:
  // RFBClient.swift:294-298 (same in linux/src/rfb.rs:368-374)
  guard types.contains(1) else {
      throw RFBError.authentication("device requires authentication (types \(types))")
  }
  try write([1], to: socket)

  // RFBClient.swift:308-311, 406-407, 492-494
  let nameLength = Int(header[20]) << 24 | ...
  if nameLength > 0 { _ = try readBytes(socket, nameLength) }
  ...
  case 3:
      _ = try readBytes(socket, 3)
      let length = try readUInt32(socket)
      if length > 0 { _ = try readBytes(socket, length) }
  ...
  case -223: // DesktopSize
      framebuffer.resize(width: width, height: height)

  // rabbiz-emu-vnc.rs:50-54, 130-132
  vnc.write_all(&[1, 1]).map_err(|e| e.to_string())?; // Security: None
  ...
  let len = u32::from_be_bytes(len_b) as usize;
  let mut txt = vec![0u8; len.min(65536)];
  vnc.read_exact(&mut txt).map_err(|e| e.to_string())?;
Impact:
  1) Both macOS and Linux `RabbizMirror` clients require RFB security type `1` (None — no authentication, no encryption). When mirroring over Wi-Fi, screen contents and all injected keystrokes (including lockscreen PINs/passwords) are transmitted in cleartext, and the device's `wayvnc` service must run unauthenticated.
  2) `rabbiz-emu-vnc` serves unauthenticated RFB type `1`, accepts any bind address via CLI arg 3, spawns an unbounded number of OS threads (`listener.incoming()`), and desynchronizes RFB framing if `ClientCutText` exceeds 65,536 bytes (only reading `len.min(65536)`).
  3) `RFBClient.swift` allocates unbounded `nameLength`, `ServerCutText` length (up to 4 GiB), and unchecked `DesktopSize` resize (up to 65535×65535 = 16 GiB) from untrusted network packets, allowing a spoofed/MITM server to crash the companion app via OOM.
Exploit/Trigger:
  Mirror over Wi-Fi on a shared network (passive sniffing or MITM/rogue VNC server), or connect to `rabbiz-emu-vnc` with multiple clients or oversized `ClientCutText`.
Recommendation:
  1) Tunnel VNC over the existing SSH connection (`ssh -L 127.0.0.1:<local>:127.0.0.1:5900`) and bind `wayvnc` strictly to `127.0.0.1`, or implement VeNCrypt/TLS + authentication in `RFBClient` and `wayvnc`.
  2) Enforce upper bounds in `RFBClient.swift` for `nameLength` (e.g., `<= 65536`), `ServerCutText` (`<= 65536`), and `DesktopSize` (`1..=16384`), matching the handshake check.
  3) In `rabbiz-emu-vnc`, enforce loopback-only binding by default, cap concurrent clients, and drain/reject `ClientCutText` lengths > 65536 cleanly.
References: CWE-319, CWE-306, CWE-770
```

```
ID: APP-007
Title: WASM Sandbox Resource Exhaustion & Crash Vectors (Unbounded Recursion, Unbounded Locals Allocation, i32 Sign-Extension Overflow)
Severity: Medium
Confidence: Confirmed
Location: tools/rabbiz-sdk/crates/rabbiz-wasm-runner/src/lib.rs:308-314, 391-424, 505-514, 559-585
Evidence:
  // lib.rs:308-314, 423 — unbounded locals count
  for _ in 0..local_decl_count {
      let n = read_leb_u32(bytes, &mut bpos)? as usize;
      ...
      total_locals = total_locals.saturating_add(n);
  }
  ...
  let mut locals = vec![0i32; ftype.params + func.num_locals];

  // lib.rs:505-514 — recursive native call with no call-depth limit
  Instr::Call(callee_idx) => {
      ...
      if let Some(ret) = self.invoke_func(callee_idx, &call_args, host, fuel)? {

  // lib.rs:559-566 (same in I32Store lines 575-584)
  Instr::I32Load { offset } => {
      let base = pop_i32(&mut stack)? as usize;
      let addr = base.checked_add(offset as usize)
          .ok_or_else(|| "memory address overflow".to_string())?;
      if addr + 4 > self.memory.len() { ... }
      let val = i32::from_le_bytes([self.memory[addr], ...]);
Impact:
  Although `#![forbid(unsafe_code)]` prevents memory corruption, a malicious `.wasm` app can abort/crash the host runner process (cannot be caught as `Err`):
  1) Infinite or deep recursion (`call 0`) exhausts the native Rust call stack long before `DEFAULT_MAX_STEPS = 250_000` fuel runs out, triggering a fatal stack-overflow abort.
  2) Declaring a huge `local_decl_count` / `n` causes `vec![0i32; ...]` to attempt multi-gigabyte allocation and abort with OOM.
  3) Casting a negative `i32` (`-1i32 as usize == usize::MAX`) in `I32Load`/`I32Store` with `offset == 0` causes `addr + 4` to wrap around to `3` in `--release` builds, bypassing the `addr + 4 > self.memory.len()` check and panicking on `self.memory[addr]`.
Exploit/Trigger:
  Load and execute a crafted `.wasm` module in `rabbiz-wasm-runner`.
Recommendation:
  1) Pass a `depth: usize` parameter to `invoke_func` and trap when `depth > MAX_CALL_DEPTH` (e.g., 128).
  2) Cap `total_locals` per function (e.g., `<= 4096`) and `stack.len()` (e.g., `<= 4096`) during parsing/execution.
  3) Convert WASM linear memory base address as `pop_i32(&mut stack)? as u32 as usize` (zero-extended, per WebAssembly spec) and use `addr.checked_add(4)` for the bounds check.
References: CWE-674, CWE-770, CWE-190
```

```
ID: APP-008
Title: Wi-Fi Password Exposed in Process Command Line (/proc/<pid>/cmdline) and Missing Option Separator in rabbiz-settings
Severity: Medium
Confidence: Confirmed
Location: tools/rabbiz-apps/crates/rabbiz-settings/src/system.rs:373-374, 383-393, 415
Evidence:
  // system.rs:373-374
  Command::new("nmcli")
      .args(["dev", "wifi", "connect", &ssid, "password", &pw])

  // system.rs:383-384, 415
  Command::new("nmcli").args(["connection", "up", &ssid])
  ...
  let ok = run(&["nmcli", "connection", "delete", &ssid]).is_some();
Impact:
  1) Passing the Wi-Fi PSK as a command-line argument to `nmcli` exposes it in `/proc/<pid>/cmdline` (and `ps aux`) for up to 15 seconds while connecting. Notably, this is a regression of a previously fixed review finding documented in `shell/Rabbiz/Services/Network.qml:67-68`, which used `rabbiz-wifi-connect` over stdin specifically to avoid argv PSK exposure.
  2) Omitting `--` (or `id`) before `&ssid` allows a nearby Wi-Fi AP broadcasting an SSID beginning with `-` to inject options into `nmcli`.
Exploit/Trigger:
  Connect to a WPA/WPA2/WPA3 network in `rabbiz-settings` while any local unprivileged process monitors `/proc/*/cmdline`, or tap/forget a nearby AP whose SSID starts with `-`.
Recommendation:
  1) Pipe the PSK over stdin to `rabbiz-wifi-connect` (matching `Network.qml`), or pass `nmcli --ask` / use NetworkManager D-Bus API directly.
  2) Use `["connection", "up", "id", "--", &ssid]` and `["connection", "delete", "id", "--", &ssid]` (or reject SSIDs starting with `-` where `nmcli` does not support `--`).
References: CWE-214, CWE-88
```

```
ID: APP-009
Title: Unquoted Shell Interpolation in Still-Shipped Legacy QML Shell Services
Severity: Medium
Confidence: Confirmed
Location: shell/Rabbiz/Services/SystemControl.qml:114-115, 180-187, 193-211; shell/Rabbiz/Services/Brightness.qml:74-81
Evidence:
  // SystemControl.qml:180-187
  function setGovernor(name) {
      if (!name || name.length === 0) return
      root.cpuGovernor = name
      _runCmd(["sh", "-c",
          "for g in /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor; do "
        + "echo " + name + " > \"$g\" 2>/dev/null || true; done"])
  }

  // SystemControl.qml:114-115
  _runCmd(["sh", "-c",
      "echo " + val + " > " + root._torchPath + "/brightness 2>/dev/null || true"])

  // Brightness.qml:74-81
  setProc.command = ["sh", "-c",
      "... backlight " + root._dev + " " + raw + " ... "
    + "|| brightnessctl -d " + root._dev + " set " + v + "% ... "
    + "|| echo " + raw + " > /sys/class/backlight/" + root._dev + "/brightness ..."]
Impact:
  `shell/Rabbiz/` is still deployed to `/usr/share/rabbiz-shell/` by `scripts/deploy_shell.sh:122-125` and remains reachable in `labwc` session mode. Unquoted interpolation of `name`, `root._torchPath`, `gov`, and `root._dev` into `sh -c` strings creates command injection surfaces if any caller, IPC handler, or sysfs device name contains shell metacharacters.
Exploit/Trigger:
  Invoke `SystemControl.setGovernor(...)` with a non-allowlisted string, or encounter a sysfs backlight/LED node name containing spaces or shell metacharacters.
Recommendation:
  Pass all dynamic values as positional arguments (`sh -c '... "$1" ...' _ <arg>`) as already done in `ShellPrefs.qml:194-197`, and allowlist `name`/`gov` to `["schedutil", "performance", "powersave"]`. If `shell/Rabbiz/` is deprecated in favor of `rabbiz-de`, remove it from deployment once `labwc` fallback is retired.
References: CWE-78
```

```
ID: APP-010
Title: Unvalidated External Search Result URLs & Plaintext Chrome Password Store in Untracked rabbiz-search Crate
Severity: Medium
Confidence: Confirmed
Location: tools/rabbiz-apps/crates/rabbiz-search/src/apps.rs:133-156
Evidence:
  pub fn open_url_in_browser(url: &str) {
      let chrome = Path::new("/opt/google/chrome/google-chrome");
      let mut cmd = if chrome.exists() {
          let mut c = Command::new("setsid");
          c.arg(chrome)
              .args([
                  ...
                  "--password-store=basic",
                  "--enable-wayland-ime",
              ])
              .arg(url);
          c
      } else {
          let mut c = Command::new("setsid");
          c.arg("xdg-open").arg(url);
          c
      };
Impact:
  1) `url` comes from scraped external web search results (`web.rs`) and is passed to `google-chrome` or `xdg-open` without verifying that it begins with `http://` or `https://` and without a `--` separator. A crafted search result URL starting with `-` can inject Chrome CLI flags, and non-HTTP schemes (`file://`, `rabbiz://`, etc.) can trigger local file/intent handlers.
  2) `--password-store=basic` forces Chromium/Chrome to store saved passwords unencrypted in plaintext on disk rather than using `gnome-keyring` / Secret Service.
Exploit/Trigger:
  Tap a search result in `rabbiz-search` whose parsed `href` uses a non-HTTP scheme or starts with `-`, or save website credentials in Chrome launched via `rabbiz-search`.
Recommendation:
  Validate that `url` starts with `https://` or `http://` (and contains no control characters) before calling `open_url_in_browser`, pass `"--"` before `url`, and remove `--password-store=basic` once the Secret Service keyring is active in the session.
References: CWE-88, CWE-20, CWE-312
```

```
ID: APP-011
Title: Predictable /tmp Paths in rabbiz-cli MCP Tools & Unvalidated AVD Name Path Traversal
Severity: Low
Confidence: Confirmed
Location: tools/rabbiz-sdk/crates/rabbiz-cli/src/lib.rs:522-525, 742, 764, 770
Evidence:
  // lib.rs:522-525
  pub fn create_avd_in(avds_root: &Path, name: &str, device: &str) -> Result<AvdProfile, String> {
      let profile = AvdProfile::for_device(name, device);
      let dir = avds_root.join(name);
      std::fs::create_dir_all(&dir)...

  // lib.rs:742, 764, 770
  std::env::temp_dir().join("rabbiz-emu-mcp-shot.png")
  ...
  let rbz_out = std::env::temp_dir().join("rabbiz-mcp-build.rbz");
  ...
  std::env::temp_dir().join("rabbiz-mcp-xdg")
Impact:
  1) `create_avd_in` allows `name` containing `../` or leading `/` to write `avd.toml` outside `avds_root`.
  2) Fixed filenames in shared `/tmp` are susceptible to symlink/clobber races on multi-user systems and collisions between concurrent MCP invocations.
Exploit/Trigger:
  Pass a traversal `name` to `rabbiz avd create`, or run `emu_pack_install` / `emu_screenshot` on a host where another user pre-creates symlinks in `/tmp`.
Recommendation:
  Validate AVD `name` against `^[A-Za-z0-9._-]+$` (reject `.` / `..`), and use unique private temp directories (e.g., `$XDG_RUNTIME_DIR` or `0700` per-PID/random temp dir).
References: CWE-22, CWE-377
```

```
ID: APP-012
Title: Unbounded Pipe Buffering Before Output Truncation in rabbiz-terminal Headless Runner
Severity: Low
Confidence: Confirmed
Location: tools/rabbiz-apps/crates/rabbiz-terminal/src/headless.rs:39, 70-78, 139-153
Evidence:
  const OUTPUT_CAP: usize = 200_000;
  ...
  fn read_to_buf(pipe: impl Read + Send + 'static, buf: Arc<Mutex<Vec<u8>>>) -> std::thread::JoinHandle<()> {
      std::thread::spawn(move || {
          let mut pipe = pipe;
          let mut tmp = [0u8; 8192];
          loop {
              match pipe.read(&mut tmp) {
                  Ok(0) | Err(_) => break,
                  Ok(n) => buf.lock().unwrap().extend_from_slice(&tmp[..n]),
              }
          }
      })
  }
Impact:
  Although `OUTPUT_CAP` (200 KB) is defined to bound returned output, `read_to_buf` appends all child stdout/stderr into an unbounded `Vec<u8>` in memory while the command runs. A chatty command (e.g., `yes` or dumping a large binary) can exhaust the 4 GB RAM on the Galaxy A21s before the 30 s timeout fires.
Exploit/Trigger:
  Run a high-output command via `rabbiz-terminal --cli` or `--mcp` (`terminal_run`).
Recommendation:
  Enforce a ring-buffer or hard byte cap (e.g., `2 * OUTPUT_CAP`) inside `read_to_buf`, discarding older bytes as new data arrives while continuing to drain the pipe so the child does not block.
References: CWE-400, CWE-770
```

---

## Positive Observations

- **`#![forbid(unsafe_code)]` widely adopted**: `rabbiz-store/src/main.rs:6`, `rabbiz-wasm-runner/src/lib.rs:5`, `rabbiz-emu-vnc.rs:4`, `rabbiz-cli/src/lib.rs:6`, and `rabbiz-pack/src/lib.rs:5` all forbid `unsafe`, eliminating memory-corruption classes in parsers and protocol handlers.
- **Pure-Rust PDF rendering (`rabbiz-pdf`)**: Uses pure-Rust `hayro = "=0.9.0"` (`tools/rabbiz-apps/crates/rabbiz-pdf/Cargo.toml:16`) with zero `unsafe` and zero external subprocesses, avoiding classic C PDF parser (Poppler/MuPDF) memory-corruption risks.
- **Symlink-aware file operations & filename sanitization (`rabbiz-files`)**: `tools/rabbiz-apps/crates/rabbiz-files/src/model.rs:360-400, 424-436` uses `fs::symlink_metadata` (never following symlinks on delete/copy) and `clean_name()` rejects `.`, `..`, `/`, `\`, and `\0`.
- **Well-documented `unsafe` and process-group timeout cleanup (`rabbiz-terminal`)**: `tools/rabbiz-apps/crates/rabbiz-terminal/src/headless.rs:87-96, 164-173` puts bounded children in their own process group via async-signal-safe `setpgid(0, 0)` and terminates the whole group with `killpg` on timeout, with explicit `SAFETY` comments.
- **Modem CLI quote rejection (`rabbiz-sms`)**: `tools/rabbiz-apps/crates/rabbiz-sms/src/backend.rs:45-55` explicitly rejects single quotes in `peer`/`body` before formatting `mmcli` key-value arguments and invokes `Command::new("mmcli")` without a shell.
- **Positional-argument shell quoting in `ShellPrefs.qml`**: `shell/Rabbiz/Services/ShellPrefs.qml:193-197` passes file path and JSON payload as `$1`/`$2` arguments to `sh -c` rather than interpolating them into the script string.

---

## Cross-Domain Leads

- **DE / IPC (`DE`)**: `rabbiz-store/src/backend.rs:98-103`, `rabbiz-cli/src/lib.rs:594-612`, and `rabbiz-emu-vnc.rs:176-183` all connect to unauthenticated TCP `127.0.0.1:5911` (`rabbiz-de` control IPC). Combined with **APP-001** (`de_apps::launch` arbitrary command execution) and **APP-005** (QEMU `0.0.0.0` forwarding), any unauthenticated client reaching port 5911 gains arbitrary command execution.
- **System / Network (`NET` / `SYS`)**: Device-side `wayvnc` / `rabbiz-mirror start` is invoked over SSH as `root@` and serves unauthenticated RFB type 1 (None) (**APP-003**, **APP-006**). Verify `wayvnc` bind address and systemd unit privileges in the OS/services domain.

---

## Reviewed Files Machine List

```files
tools/rabbiz-apps/crates/rabbiz-store/src/backend.rs|reviewed|direct-read|Domain 7 focus area
tools/rabbiz-apps/crates/rabbiz-store/src/main.rs|reviewed|targeted-read|Domain 7 focus area
tools/rabbiz-apps/crates/rabbiz-settings/src/system.rs|reviewed|direct-read|Domain 7 focus area
tools/rabbiz-apps/crates/rabbiz-settings/src/cli.rs|reviewed|targeted-read|Domain 7 focus area
tools/rabbiz-apps/crates/rabbiz-files/src/main.rs|reviewed|targeted-read|Domain 7 focus area
tools/rabbiz-apps/crates/rabbiz-files/src/model.rs|reviewed|targeted-read|Domain 7 focus area
tools/rabbiz-apps/crates/rabbiz-terminal/src/headless.rs|reviewed|targeted-read|Domain 7 focus area
tools/rabbiz-apps/crates/rabbiz-dialer/src/backend.rs|reviewed|direct-read|Domain 7 focus area
tools/rabbiz-apps/crates/rabbiz-sms/src/backend.rs|reviewed|direct-read|Domain 7 focus area
tools/rabbiz-apps/crates/rabbiz-pdf/Cargo.toml|reviewed|direct-read|Domain 7 focus area
tools/rabbiz-apps/crates/rabbiz-app-kit/src/mcp.rs|reviewed|direct-read|Domain 7 focus area
tools/rabbiz-apps/crates/rabbiz-app-kit/src/intent.rs|reviewed|targeted-read|Domain 7 focus area
tools/rabbiz-apps/crates/rabbiz-search/src/apps.rs|reviewed|targeted-read|Untracked new app crate
tools/rabbiz-apps/crates/rabbiz-search/src/web.rs|reviewed|targeted-read|Untracked new app crate
tools/rabbiz-apps/crates/rabbiz-music/src/player.rs|reviewed|rg-scan|Subprocess check
tools/rabbiz-sdk/crates/rabbiz-wasm-runner/src/lib.rs|reviewed|targeted-read|Domain 7 focus area
tools/rabbiz-sdk/crates/rabbiz-flutter-runner/src/lib.rs|reviewed|direct-read|Domain 7 focus area
tools/rabbiz-sdk/crates/rabbiz-emu-viewer/src/bin/rabbiz-emu-vnc.rs|reviewed|direct-read|Domain 7 focus area
tools/rabbiz-sdk/crates/rabbiz-cli/src/lib.rs|reviewed|direct-read|Domain 7 focus area
tools/rabbiz-sdk/crates/rabbiz-pack/src/lib.rs|reviewed|direct-read|Domain 7 focus area (.rbz installer)
tools/rabbiz-mirror/Sources/RabbizMirror/DeviceMirrorService.swift|reviewed|direct-read|Domain 7 focus area
tools/rabbiz-mirror/Sources/RabbizMirror/RFBClient.swift|reviewed|direct-read|Domain 7 focus area
tools/rabbiz-mirror/linux/src/device_service.rs|reviewed|direct-read|Untracked Linux mirror port
tools/rabbiz-mirror/linux/src/rfb.rs|reviewed|targeted-read|Untracked Linux mirror port
tools/rabbiz-de/crates/de-apps/src/launch.rs|reviewed|direct-read|Domain 7 focus area
shell/Rabbiz/Services/Network.qml|reviewed|targeted-read|Legacy QML shell services
shell/Rabbiz/Services/Bluetooth.qml|reviewed|targeted-read|Legacy QML shell services
shell/Rabbiz/Services/ShellPrefs.qml|reviewed|targeted-read|Legacy QML shell services
shell/Rabbiz/Services/SystemControl.qml|reviewed|targeted-read|Legacy QML shell services
shell/Rabbiz/Services/Brightness.qml|reviewed|rg-scan|Legacy QML shell services
shell/Rabbiz/Services/Power.qml|reviewed|rg-scan|Legacy QML shell services
shell/Rabbiz/Services/Media.qml|reviewed|rg-scan|Legacy QML shell services
shell/Rabbiz/Services/Appearance.qml|reviewed|rg-scan|Legacy QML shell services
shell/Rabbiz/Services/Telephony.qml|reviewed|rg-scan|Legacy QML shell services
tools/rabbiz-sdk/crates/rabbiz-sdk-c/src/lib.rs|skipped|none|31 unsafe FFI blocks need dedicated C ABI audit
tools/rabbiz-mirror/Sources/RabbizMirror/ZRLEDecoder.swift|skipped|none|Pixel decoder outside primary focus time budget
```
