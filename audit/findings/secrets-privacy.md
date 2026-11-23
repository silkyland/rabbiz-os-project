# Domain Audit: Secrets & Privacy (`PRIV`)

## Scope & Files Reviewed

### Files Inspected

| File | Method | Notes |
|---|---|---|
| `audit/tools/secrets-scan.txt` | `direct-read` | All 31 pattern hits triaged directly against source files |
| `rootfs-overlay/usr/local/bin/rabbiz-wifi-connect` | `direct-read` | Full file (51 lines): PSK stdin/pty handling, `script -qec` invocation |
| `rootfs-overlay/usr/local/bin/a21s-telemetry` | `direct-read` | Full file (54 lines): local procfs/sysfs hardware metrics |
| `scripts/release/public_history.sh` | `direct-read` | Full file (149 lines): history rewrite, redaction rules, verification |
| `scripts/mirror_setup.sh` | `direct-read` | Full file (163 lines): `wayvnc` systemd unit, `--lan`, `curl -skL` |
| `scripts/probe_device.sh` | `direct-read` | Full file (62 lines): ADB device profile capture |
| `scripts/build_emu_image.sh` | `targeted-read` | Lines 170–230: `RABBIZ_EMU_LISTEN` / `RABBIZ_CTL_LISTEN` drop-in |
| `tools/rabbiz-apps/crates/rabbiz-settings/src/system.rs` | `targeted-read` | Lines 330–430: `connect_wifi_async` `nmcli` argv PSK leak |
| `shell/Rabbiz/Services/Network.qml` | `targeted-read` | Lines 50–105, 240–295: `wifiConnect`, `_connPsk` lifecycle |
| `shell/Rabbiz/Services/SystemControl.qml` | `targeted-read` | Lines 1–90, 405–465: `a21s-telemetry` consumer |
| `rootfs-overlay/usr/local/bin/rabbiz-spotify-ctl` | `targeted-read` | Lines 485–690: `credentials.json` existence checks |
| `devices/samsung/a21s/docs/a21s-access-decision.md` | `direct-read` | Full file (7 lines): real device serial verified |
| `devices/samsung/a21s/docs/a21s-wifi-bluetooth.md` | `targeted-read` | Lines 120–170: home SSID and BT adapter address verified |
| `devices/samsung/a21s/docs/device-a21s-profile.txt` | `direct-read` | Full file (161 lines): checked for serial/IMEI/MAC PII |
| `devices/samsung/a21s/findings/2026-11-18-de-wayvnc-unauthenticated-localhost.md` | `direct-read` | Full file (35 lines): verified unauthenticated VNC exposure |
| `tools/rabbiz-sdk/Dockerfile.emu` | `direct-read` | Full file (24 lines): `0.0.0.0:5910` / `0.0.0.0:5911` entrypoint |
| `tools/rabbiz-sdk/crates/rabbiz-emu-viewer/src/main.rs` | `targeted-read` | Lines 140–190: Docker `-p 5910:5910 -p 5911:5911` host bind |
| `tools/rabbiz-sdk/crates/rabbiz-emu-viewer/src/bin/rabbiz-emu-vnc.rs` | `targeted-read` | Lines 1–150: unauthenticated RFB 3.8 bridge & touch forwarding |
| `tools/rabbiz-mirror/Sources/RabbizMirror/RFBClient.swift` | `targeted-read` | Lines 25–105: RFB 3.8 client (no authentication) |
| `tools/rabbiz-de/crates/de-config/src/flags.rs` | `targeted-read` | Lines 170–250, 510–570: loopback-only TCP listener validation |
| `tools/rabbiz-de/crates/de-core/src/runtime/ctl_ipc.rs` | `targeted-read` | Lines 1–170, 284–470, 690–745: Unix `0600` bind, loopback enforcement, lock checks |
| `tools/rabbiz-de/crates/de-core/src/runtime/composite.rs` | `targeted-read` | Lines 350–410: `capture_output` lock-screen blanking |
| `tools/rabbiz-de/crates/de-core/src/protocols/screencopy.rs` | `targeted-read` | Lines 1–200: `zwlr_screencopy_manager_v1` privileged check & capture path |
| `tools/rabbiz-de/crates/de-core/src/protocols/privileged.rs` | `targeted-read` | Lines 1–100: `SO_PEERCRED` (`get_credentials`) client identity |
| `tools/rabbiz-de/crates/de-core/src/runtime/state.rs` | `targeted-read` | Lines 825–875: `ctl_ipc` & `emu_server` initialization |
| `tools/rabbiz-de/crates/de-bin/src/auth_helper.rs` | `targeted-read` | Lines 1–90: credential storage, PAM verify, zeroize |
| `tools/rabbiz-de/crates/de-system/src/auth.rs` | `targeted-read` | Lines 500–540: `HelperRequest` Drop zeroize, `HelperResponse` |
| Repository-wide pattern scans (`scripts/`, `docs/`, `findings/`, `devices/`, `rootfs-overlay/`, `tools/`, `shell/`) | `rg-scan` | Scanned for credentials, keys, tokens, MACs, Samsung serials, IMEIs, LAN IPs, host paths, emails, VNC/IPC listeners |

### Not Inspected (Out of Scope / Excluded)

- `src/` and `build/`: Gitignored third-party kernel/bootloader trees; not part of tracked RabbizOS source.
- `.git/` commit history objects: Read-only audit focused on the working tree; full historical commit scan (e.g., `gitleaks detect`) was not executed.
- Binary files, images (`*.png`, `*.jpg`, `*.img`), and compiled `target/` directories: Not scanned for embedded EXIF/metadata or compiled strings.
- `socs/exynos850/drivers/` beyond `secrets-scan.txt` MAC hits: Upstream/vendor kernel driver code; audited by Kernel domain.
- Live device filesystem (`/root/.ssh/authorized_keys`, `/etc/NetworkManager/system-connections/`, `/home/rabbiz/.cache/spotify-player/credentials.json`): No live device access per read-only audit rules.

---

## Triage of `audit/tools/secrets-scan.txt` (All 31 Hits)

All 31 hits in `audit/tools/secrets-scan.txt` were verified directly in source:

- **True Positives / Actionable (6 hits):**
  - `devices/samsung/a21s/docs/a21s-wifi-bluetooth.md:149` (`MAC_ADDR`): Bluetooth adapter MAC (`XX:XX:XX:XX:XX:XX`, suffix matches `A2:17`) committed in doc -> see **PRIV-005**.
  - `scripts/build_emu_image.sh:193-194` (`BIND_ALL_INTERFACES`, 2 hits): `RABBIZ_EMU_LISTEN=0.0.0.0:5910` and `RABBIZ_CTL_LISTEN=0.0.0.0:5911` -> see **PRIV-004**.
  - `tools/rabbiz-sdk/Dockerfile.emu:22` (`BIND_ALL_INTERFACES`): `--emu-listen=0.0.0.0:5910 --ctl-listen=0.0.0.0:5911` -> see **PRIV-004**.
  - `tools/rabbiz-sdk/crates/rabbiz-emu-viewer/src/main.rs:166` (`BIND_ALL_INTERFACES`): `--emu-listen=0.0.0.0:5910 --ctl-listen=0.0.0.0:5911` (and `-p 5910:5910 -p 5911:5911` at lines 179–182) -> see **PRIV-004**.
- **False Positives / Benign Policy, API, or Sentinel References (25 hits):**
  - `AGENTS.md:24`, `docs/HANDOFF-2026-11-16-device-ui.md:98`, `docs/PLAN-keyboard.md:225`, `docs/PROMPT-DESKTOP-SHELL.md:20`: Ground-rule reminders never to print `~/.credentail` (confirmed no tracked script reads or prints `~/.credentail` or `WIFI_PASSWORD`).
  - `docs/DE-APP-LIFECYCLE.md:71`, `docs/DE-KEYGUARD-AUTH.md:24`, `docs/DE-PLAN.md:12,84,405`, `docs/DE-PRIVILEGED-PROTOCOLS.md:27`: Design/planning prose and Wayland `Client::get_credentials()` API docs.
  - `rootfs-overlay/usr/local/bin/rabbiz-spotify-ctl:496,531,565,667`: Existence checks (`[ -f ... ]`) for user's local `spotify-player/credentials.json`; contents are never read, logged, or transmitted.
  - `shell/Rabbiz/Services/Bluetooth.qml:279`: Placeholder comment `AA:BB:CC:DD:EE:FF`.
  - `socs/exynos850/drivers/scsc/` (`scsc_bt_module.c:101`, `cfg80211_ops.c:3508`, `ioctl.c:1963`, `mgt.c:97,139`, `mib.h:1585`): Zero (`00:00:00:00:00:00`), broadcast (`ff:ff:ff:ff:ff:ff`), and comment example MAC sentinels.
  - `tools/rabbiz-de/crates/de-bin/src/auth_helper.rs:3`, `de-config/src/flags.rs:542`, `de-core/src/protocols/privileged.rs:3,69`, `de-system/src/auth.rs:511`: Positive security implementations and unit tests.

---

## Findings

### PRIV-001
```
ID: PRIV-001
Title: Unauthenticated wayvnc screen-capture and remote-input server bound to loopback (and optional LAN) bypasses compositor privilege policy
Severity: High
Confidence: Confirmed
Location: scripts/mirror_setup.sh:14,21,75,155-163
Evidence:
  scripts/mirror_setup.sh:14:  LISTEN="172.16.42.1 127.0.0.1"
  scripts/mirror_setup.sh:21:      --lan) LISTEN="0.0.0.0" ;;
  scripts/mirror_setup.sh:75:  ExecStart=/usr/bin/wayvnc -o DSI-1 -f 30 $LISTEN
  scripts/mirror_setup.sh:76:  Restart=always
  scripts/mirror_setup.sh:80:  WantedBy=default.target
  devices/samsung/a21s/findings/2026-11-18-de-wayvnc-unauthenticated-localhost.md:18-19:
    `a21s-wayvnc` listens on `172.16.42.1:5900` (usb0) and `127.0.0.1:5900`, and its RFB handshake offers
    only security type 1, **None**. There is no wayvnc config file and the device has no firewall rules.
  tools/rabbiz-de/crates/de-config/src/privileged.rs:219:
    "/usr/bin/wayvnc",
Impact:
  1. Local privilege/policy bypass & screen eavesdropping: Because `/usr/bin/wayvnc` is on `rabbiz-de`'s
     built-in privileged allowlist for screencopy and virtual input, binding `127.0.0.1:5900` with RFB
     security type `None` allows ANY local process or untrusted third-party app running on the phone to
     silently capture the entire display (every app's contents, messages, passwords on screen) and inject
     arbitrary pointer/keyboard events, completely defeating `rabbiz-de`'s default-deny Wayland privilege policy.
  2. Physical USB / LAN eavesdropping & takeover: Any host connected via USB (`usb0` `172.16.42.1`) — or
     any host on the Wi-Fi LAN if `--lan` is used — can silently watch the screen and control the device
     without authentication or any on-screen indicator. (Note: `rabbiz-de` mitigates lock-screen pixel
     disclosure via `capture_output` blanking in `composite.rs:354-360`, but unlocked sessions remain
     fully exposed.)
Exploit/Trigger:
  Run `scripts/mirror_setup.sh` (which enables `a21s-wayvnc.service` persistently under `default.target`).
  Any local app connects to `127.0.0.1:5900` (or USB peer connects to `172.16.42.1:5900`) using RFB 3.8
  security type 1 (`None`) and sends `FramebufferUpdateRequest` / `PointerEvent` / `KeyEvent`.
Recommendation:
  1. Remove `127.0.0.1` from the default `LISTEN` list (or replace direct TCP with SSH port-forwarding
     over an authenticated SSH session, or Unix socket with `0600` permissions).
  2. Do not enable `a21s-wayvnc.service` persistently on boot (`default.target`); make mirroring on-demand
     with explicit user confirmation and a persistent status-bar recording/mirroring indicator.
  3. Enable wayvnc authentication (TLS + username/password or VeNCrypt) with credentials stored in a
     `0600` file, and remove the unauthenticated `--lan` (`0.0.0.0`) option.
References: CWE-306 (Missing Authentication for Critical Function), CWE-200 (Exposure of Sensitive Information)
```

### PRIV-002
```
ID: PRIV-002
Title: Plaintext Wi-Fi passphrase exposed in process command line (/proc/<pid>/cmdline) by rabbiz-settings
Severity: Medium
Confidence: Confirmed
Location: tools/rabbiz-apps/crates/rabbiz-settings/src/system.rs:370-381
Evidence:
  370: pub fn connect_wifi_async(ssid: String, password: Option<String>, tx: Sender<NetEvent>) {
  371:     std::thread::spawn(move || {
  372:         let ok = if let Some(pw) = password {
  373:             Command::new("nmcli")
  374:                 .args(["dev", "wifi", "connect", &ssid, "password", &pw])
  375:                 .env("LC_ALL", "C")
  376:                 .stdin(Stdio::null())
  377:                 .stdout(Stdio::null())
  378:                 .stderr(Stdio::null())
  379:                 .spawn()
  380:                 .ok()
  381:                 .is_some_and(|mut c| status_bounded(&mut c, 15))
Impact:
  Any local process or unprivileged user on the device can read `/proc/<pid>/cmdline` (world-readable on
  standard Linux mounts without `hidepid=2`) during the up to 15-second connection window and harvest the
  plaintext Wi-Fi PSK. This directly regresses the mitigation already built for the QML shell in
  `rootfs-overlay/usr/local/bin/rabbiz-wifi-connect`.
Exploit/Trigger:
  User connects to a WPA/WPA2/WPA3 Wi-Fi network from the native `rabbiz-settings` app while another local
  process polls `/proc/*/cmdline`.
Recommendation:
  Replace the direct `nmcli ... password <pw>` call in `rabbiz-settings` with either:
  1. NetworkManager's D-Bus API (`org.freedesktop.NetworkManager.AddAndActivateConnection`), passing the
     PSK over the D-Bus socket instead of argv, or
  2. Spawning `rabbiz-wifi-connect <ssid>` and writing the PSK to stdin (zeroizing the `pw` buffer
     immediately after writing).
References: CWE-214 (Invocation of Process Using Visible Sensitive Information)
```

### PRIV-003
```
ID: PRIV-003
Title: History-sanitization script (public_history.sh) re-fetches unredacted objects, omits Linux home paths and commit messages, and passes without gitleaks
Severity: Medium
Confidence: Confirmed
Location: scripts/release/public_history.sh:67-82,96,120,129-134
Evidence:
  67: cat > "$WORK/replace.txt" <<'EOF'
  68: regex:192\.168\.\d{1,3}\.\d{1,3}==><lan-ip>
  69: regex:/Users/[A-Za-z0-9._-]+/==>/Users/<user>/
  70: EOF
  ...
  79: git -C "$OUT" filter-repo --quiet --force \
  80:   --mailmap "$WORK/mailmap" \
  81:   --replace-text "$WORK/replace.txt"
  ...
  96: git -C "$OUT" fetch --quiet "$SRC" "refs/heads/$BRANCH" 2>/dev/null || true
  97: git -C "$OUT" diff --stat FETCH_HEAD HEAD 2>/dev/null | tail -n 1 | sed 's/^/    /' || true
  ...
  120:     [[ -z "$lit" || "$lit" == regex:* || "$lit" == \#* ]] && continue
  ...
  129: if command -v gitleaks >/dev/null 2>&1; then
  130:   if gitleaks detect --source "$OUT" --redact --no-banner --exit-code 1; then ...
  131: else
  132:   echo "  WARNING: gitleaks not installed — NO secret scan was done..."
  134: fi
Impact:
  1. Line 96 (`git fetch "$SRC"`) copies unredacted commits and blobs from `$SRC` back into `$OUT/.git/objects`
     (referenced by `FETCH_HEAD`) AFTER `git-filter-repo` completes. If `$OUT` is zipped, mirrored, or pushed
     with `--all`/`--mirror`, all redacted secrets/PII are still present in the object store.
  2. Lines 68–69 only redact `192.168.x.x` and macOS `/Users/<user>/`, missing Linux home paths
     (`/home/<user>/`, already present in `docs/PLAN.md:8`), Samsung serials (`R58...`), and real SSIDs/MACs.
  3. Line 81 uses `--replace-text` (blob contents only) without `--replace-message`, leaving private IPs,
     paths, or strings from `REDACT_FILE` unredacted inside commit messages.
  4. Line 120 skips verification of `regex:` rules in `REDACT_FILE`.
  5. Lines 129–134 treat missing `gitleaks` as a soft warning and still exit `0` (`RESULT: OK`).
Exploit/Trigger:
  Maintainer runs `scripts/release/public_history.sh` on a host without `gitleaks` (or with sensitive strings
  in commit messages / `/home/<user>/` paths / `regex:` rules) and publishes the resulting repository.
Recommendation:
  1. Remove `git -C "$OUT" fetch "$SRC"` on line 96 (compare against `$SRC` using `git diff --no-index` or a
     separate temporary clone so unredacted objects never enter `$OUT/.git/objects`).
  2. Add `--replace-message "$WORK/replace.txt"` to `git filter-repo`.
  3. Add `/home/[A-Za-z0-9._-]+/` (excluding `/home/rabbiz/` and `/home/gamer/`), Samsung serial patterns,
     and `10.x`/`172.16-31.x` (excluding `172.16.42.x` USB gadget) to the default redaction rules.
  4. Fail closed (`fail=1`) when `gitleaks` is not installed unless an explicit `--allow-no-gitleaks` flag
     is passed, and verify `regex:` rules from `REDACT_FILE` using `git log --all -p -G`.
References: CWE-200 (Exposure of Sensitive Information), CWE-538 (Insertion of Sensitive Information into Externally-Accessible File or Directory)
```

### PRIV-004
```
ID: PRIV-004
Title: Emulator Docker launcher publishes unauthenticated display/control ports on all host interfaces (0.0.0.0) and passes 0.0.0.0 listen flags; rabbiz-emu-vnc lacks authentication and loopback enforcement
Severity: Medium
Confidence: Confirmed
Location: tools/rabbiz-sdk/crates/rabbiz-emu-viewer/src/main.rs:166,179-182
Evidence:
  tools/rabbiz-sdk/crates/rabbiz-emu-viewer/src/main.rs:166:
    "exec {} run --backend=virtual --size=720x1600 --emu-listen=0.0.0.0:5910 --ctl-listen=0.0.0.0:5911"
  tools/rabbiz-sdk/crates/rabbiz-emu-viewer/src/main.rs:179-182:
    "-p", "5910:5910",
    "-p", "5911:5911",
  tools/rabbiz-sdk/Dockerfile.emu:22:
    exec rabbiz-de run --backend=virtual --size=${RABBIZ_EMU_SIZE:-720x1600} --emu-listen=0.0.0.0:5910 --ctl-listen=0.0.0.0:5911 "$@"
  scripts/build_emu_image.sh:193-194:
    Environment=RABBIZ_EMU_LISTEN=0.0.0.0:5910
    Environment=RABBIZ_CTL_LISTEN=0.0.0.0:5911
  tools/rabbiz-sdk/crates/rabbiz-emu-viewer/src/bin/rabbiz-emu-vnc.rs:19-23,50-55:
    let vnc_addr = std::env::args().nth(3).unwrap_or_else(|| "127.0.0.1:5900".to_string());
    let listener = TcpListener::bind(&vnc_addr).expect("bind VNC 5900");
    ...
    // 2. Security: 1 type = None (1)
    vnc.write_all(&[1, 1]).map_err(|e| e.to_string())?;
Impact:
  1. `docker run -p 5910:5910 -p 5911:5911` binds ports 5910 (unauthenticated frame stream) and 5911
     (unauthenticated control IPC: `Launch`, `InjectTouch`, `InjectKey`, `Screenshot`, `GetState`) on
     `0.0.0.0` of the developer's host machine, exposing them to the entire LAN (bypassing host firewalls
     via Docker's iptables NAT rules on Linux).
  2. Note: Current `rabbiz-de` rejects non-loopback `--emu-listen`/`--ctl-listen` (`de-config/src/flags.rs:171-181`,
     `ctl_ipc.rs:134-137,298-301`), which currently prevents `rabbiz-de` from binding `0.0.0.0` inside the
     container/VM (also breaking container/VM startup if run with the hardened binary), but if an older
     binary or relaxed flag is used, LAN peers gain full unauthenticated screen view and control.
  3. `rabbiz-emu-vnc` offers unauthenticated RFB security type `None` (1), forwards pointer events to
     `ctl_addr` touch injection, and does not enforce loopback on `argv[3]`.
Exploit/Trigger:
  Developer runs `rabbiz-emu-viewer` (auto-launching Docker) on a shared/untrusted network, or runs
  `rabbiz-emu-vnc` with a non-loopback bind address.
Recommendation:
  1. Change Docker port publishing in `rabbiz-emu-viewer/src/main.rs:179-182` to loopback-only:
     `-p 127.0.0.1:5910:5910 -p 127.0.0.1:5911:5911`.
  2. Enforce loopback-only bind validation in `rabbiz-emu-vnc.rs` (matching `de-config/src/flags.rs:171-181`).
  3. Reconcile container/VM networking (`Dockerfile.emu`, `build_emu_image.sh`) using a loopback-only
     proxy or container-scoped Unix socket mounted into the host instead of `0.0.0.0`.
References: CWE-668 (Exposure of Resource to Wrong Sphere), CWE-306 (Missing Authentication for Critical Function)
```

### PRIV-005
```
ID: PRIV-005
Title: Device serial number, home Wi-Fi SSID, Bluetooth MAC, private LAN IP, and developer host path committed in tracked docs/logs
Severity: Medium
Confidence: Confirmed
Location: devices/samsung/a21s/docs/a21s-access-decision.md:4-5
Evidence:
  (All real identifiers redacted below per audit policy)
  - Real Samsung SM-A217F device serial (`R58XXXXXXXX`):
      devices/samsung/a21s/docs/a21s-access-decision.md:4-5
      docs/PLAN.md:27
  - Real home Wi-Fi SSID (`<REDACTED_SSID>`):
      devices/samsung/a21s/docs/a21s-wifi-bluetooth.md:139
  - Bluetooth adapter address (`XX:XX:XX:XX:XX:XX`; note last two octets mirror model `A2:17`):
      devices/samsung/a21s/docs/a21s-wifi-bluetooth.md:149
  - Private LAN IP (`192.168.x.x`):
      devices/samsung/a21s/docs/a21s-flash-boot-checklist.md:24
      devices/samsung/a21s/docs/HANDOFF-kernel-lane.md:9
      devices/samsung/a21s/findings/logs/2026-11-18-s2mu106-charger-online-props.txt:9
  - Developer Linux host path (`/home/<user>/Sites/Labs/android-arm-linux`):
      docs/PLAN.md:8
Impact:
  Violates repository ground rules (`AGENTS.md:24`: "Never leak secrets... Same for serials, IMEIs, real
  MAC addresses and tokens"). If the repository is published publicly without adding all of these strings
  to `scripts/release/redact.local.txt`, the owner's device serial, home SSID, LAN addressing, and host
  username are permanently exposed.
Exploit/Trigger:
  Public release or sharing of the repository.
Recommendation:
  1. Replace the device serial with `<device-serial>`, the SSID with `<wifi-ssid>`, the BT MAC with
     `<bt-mac>` (unless confirmed synthetic), the LAN IP with `<device-wifi-ip>`, and the host path with
     `<repo-root>` in the tracked docs/logs.
  2. Add these literal values to `scripts/release/redact.local.txt` before running `public_history.sh`.
References: CWE-200 (Exposure of Sensitive Information)
```

### PRIV-006
```
ID: PRIV-006
Title: rabbiz-wifi-connect PSK echo race under load, shell interpolation in PROFILE, and uncleaned _connPsk in Network.qml
Severity: Low
Confidence: Confirmed
Location: rootfs-overlay/usr/local/bin/rabbiz-wifi-connect:22-26,41
Evidence:
  rootfs-overlay/usr/local/bin/rabbiz-wifi-connect:22-26:
    ask() {
        local what="$1"; shift
        ( sleep 2; printf '%s\n' "$PSK" ) \
            | timeout 45 script -qec "stty -echo; nmcli --ask $what" /dev/null
    }
  rootfs-overlay/usr/local/bin/rabbiz-wifi-connect:41:
    ask "connection modify \"$PROFILE\" wifi-sec.psk" || true
  shell/Rabbiz/Services/Network.qml:70,254:
    70:  root._connPsk = password || ""
    254: onStarted: connProc.write((root._connPsk.length > 0 ? root._connPsk : "") + "\n")
Impact:
  1. Timing/echo race: `ask()` sleeps a fixed 2 seconds before writing `$PSK` into the pty. Under heavy
     CPU/I/O load, if `script` has not yet run `stty -echo` before `printf` writes `$PSK`, the plaintext
     PSK is echoed to stdout and captured by `Network.qml`'s `StdioCollector` (`root._connOut`).
  2. Shell injection in `PROFILE`: Line 41 interpolates `"$PROFILE"` inside double quotes in
     `script -qec "..."`. A NetworkManager connection name containing `$(...)`, `` `...` ``, or `"`
     (e.g., from a malicious SSID previously saved as a connection name) executes arbitrary shell commands
     as the session user.
  3. Residual secret in memory: `Network.qml` never clears `root._connPsk` after writing it to `connProc`
     stdin, leaving the plaintext Wi-Fi passphrase in QML singleton memory for the rest of the session.
Exploit/Trigger:
  1. Connecting to Wi-Fi under high system load (echo race).
  2. Reconnecting to an existing saved Wi-Fi profile whose name contains shell metacharacters.
Recommendation:
  1. Replace the `script -qec` + `sleep 2` pty hack with a small Python/Rust helper calling NetworkManager
     over D-Bus (or `nmcli --ask` via a proper pty wait-for-prompt loop with `ECHO` disabled via `termios`
     before `exec`, and zero shell interpolation).
  2. Clear `root._connPsk = ""` immediately inside `connProc.onStarted` right after `connProc.write(...)`.
References: CWE-78 (OS Command Injection), CWE-362 (Race Condition), CWE-316 (Cleartext Storage of Sensitive Information in Memory)
```

### PRIV-007
```
ID: PRIV-007
Title: a21s-telemetry collects local hardware metrics only (no PII, no network egress, no disk persistence)
Severity: Info
Confidence: Confirmed
Location: rootfs-overlay/usr/local/bin/a21s-telemetry:1-54
Evidence:
  rootfs-overlay/usr/local/bin/a21s-telemetry:16-54:
    Reads only /proc/stat, /proc/meminfo, /sys/devices/system/cpu/cpu0/cpufreq/{scaling_cur_freq,scaling_governor},
    `df -h /`, /sys/class/leds/*{torch,flash}*, /sys/kernel/a21s_fp/{chipid,status,irq_count},
    /dev/umts_* names, and /sys/devices/platform/soc@0/14020000.cpif/* status files; prints to stdout.
  shell/Rabbiz/Services/SystemControl.qml:407-411:
    property var telemetryProc: Process { command: ["a21s-telemetry"] ... }
Impact:
  No privacy risk. Despite the name "telemetry", the script performs strictly local hardware polling for
  the on-device UI (CPU/RAM/disk/torch/fingerprint/modem status). It does not read IMEI, IMSI, MACs,
  serial numbers, or user data, does not write files, and performs no network activity.
Exploit/Trigger: N/A (informational verification).
Recommendation:
  No security change required. Optionally document clearly in `a21s-telemetry` header that it is strictly
  local hardware polling with no external transmission.
References: N/A
```

---

## Positive Observations

1. **Loopback-only & virtual-backend-only TCP enforcement in `rabbiz-de`** (`tools/rabbiz-de/crates/de-config/src/flags.rs:171-181,541-563`; `tools/rabbiz-de/crates/de-core/src/runtime/ctl_ipc.rs:134-137,298-301`): Both CLI parsing and socket binding strictly reject non-loopback addresses (`0.0.0.0`, `[::]`, LAN IPs) for `--ctl-listen` and `--emu-listen`, and refuse TCP listeners entirely unless `--backend=virtual` is active.
2. **Atomic `0600` control socket with no `/tmp` fallback** (`tools/rabbiz-de/crates/de-core/src/runtime/ctl_ipc.rs:90-123`): `rabbiz-de-ctl.sock` is bound under `umask(0o177)` (no `bind`→`chmod` race) and disabled if `XDG_RUNTIME_DIR` is unset.
3. **Lock-screen capture blanking & IPC lockdown** (`tools/rabbiz-de/crates/de-core/src/runtime/composite.rs:354-360`; `tools/rabbiz-de/crates/de-core/src/runtime/ctl_ipc.rs:1002-1020,1107-1160`): `capture_output()` returns a blank frame while locked, and `command_allowed_while_locked` blocks `InjectTouch`, `InjectKey`, `Launch`, `Screenshot`, and `SetMock` during lock.
4. **Memory zeroization & privilege separation in `rabbiz-auth-helper`** (`tools/rabbiz-de/crates/de-bin/src/auth_helper.rs:1-6,40-43`; `tools/rabbiz-de/crates/de-system/src/auth.rs:505-519`): Credentials live under root-owned `0700`/`0600` storage, PINs use Argon2id, responses never echo secrets, and secret buffers implement `zeroize::Zeroize` on drop and immediately after PAM stdin write.
5. **No hardcoded passwords, API keys, private keys, or IMEIs found** across tracked `scripts/`, `docs/`, `findings/`, `devices/`, `rootfs-overlay/`, `tools/`, and `shell/`, and `~/.credentail` / `WIFI_PASSWORD` is never read or printed by any tracked script.

---

## Reviewed Files Machine List

```files
audit/tools/secrets-scan.txt|reviewed|direct-read|Triaged all 31 scanner hits against source
rootfs-overlay/usr/local/bin/rabbiz-wifi-connect|reviewed|direct-read|Audited PSK stdin/pty handling and shell interpolation
rootfs-overlay/usr/local/bin/a21s-telemetry|reviewed|direct-read|Verified local-only hardware polling with no PII or network egress
scripts/release/public_history.sh|reviewed|direct-read|Audited redaction rules, git filter-repo invocation, and verification gaps
scripts/mirror_setup.sh|reviewed|direct-read|Audited unauthenticated wayvnc listener and --lan exposure
scripts/probe_device.sh|reviewed|direct-read|Verified ADB profile collection fields
scripts/build_emu_image.sh|reviewed|targeted-read|Checked RABBIZ_EMU_LISTEN and RABBIZ_CTL_LISTEN drop-in
tools/rabbiz-apps/crates/rabbiz-settings/src/system.rs|reviewed|targeted-read|Found plaintext Wi-Fi PSK passed in nmcli argv
shell/Rabbiz/Services/Network.qml|reviewed|targeted-read|Audited rabbiz-wifi-connect caller and _connPsk lifecycle
shell/Rabbiz/Services/SystemControl.qml|reviewed|targeted-read|Verified a21s-telemetry consumer
rootfs-overlay/usr/local/bin/rabbiz-spotify-ctl|reviewed|targeted-read|Verified credentials.json existence checks only
devices/samsung/a21s/docs/a21s-access-decision.md|reviewed|direct-read|Found committed real device serial number
devices/samsung/a21s/docs/a21s-wifi-bluetooth.md|reviewed|targeted-read|Found committed home Wi-Fi SSID and BT adapter address
devices/samsung/a21s/docs/device-a21s-profile.txt|reviewed|direct-read|Verified no serial/IMEI/MAC PII in device profile dump
devices/samsung/a21s/findings/2026-11-18-de-wayvnc-unauthenticated-localhost.md|reviewed|direct-read|Verified unauthenticated wayvnc loopback/usb0 finding
tools/rabbiz-sdk/Dockerfile.emu|reviewed|direct-read|Checked 0.0.0.0:5910 and 0.0.0.0:5911 entrypoint
tools/rabbiz-sdk/crates/rabbiz-emu-viewer/src/main.rs|reviewed|targeted-read|Found Docker -p 5910:5910 -p 5911:5911 all-interface host publish
tools/rabbiz-sdk/crates/rabbiz-emu-viewer/src/bin/rabbiz-emu-vnc.rs|reviewed|targeted-read|Verified unauthenticated RFB None bridge and missing loopback check
tools/rabbiz-mirror/Sources/RabbizMirror/RFBClient.swift|reviewed|targeted-read|Verified unauthenticated RFB 3.8 client
tools/rabbiz-de/crates/de-config/src/flags.rs|reviewed|targeted-read|Verified loopback-only validation for ctl-listen and emu-listen
tools/rabbiz-de/crates/de-core/src/runtime/ctl_ipc.rs|reviewed|targeted-read|Verified 0600 Unix socket, loopback TCP check, and lock-screen IPC restrictions
tools/rabbiz-de/crates/de-core/src/runtime/composite.rs|reviewed|targeted-read|Verified capture_output lock-screen blanking
tools/rabbiz-de/crates/de-core/src/protocols/screencopy.rs|reviewed|targeted-read|Verified screencopy privileged gate and capture_output path
tools/rabbiz-de/crates/de-core/src/protocols/privileged.rs|reviewed|targeted-read|Verified SO_PEERCRED client identification
tools/rabbiz-de/crates/de-core/src/runtime/state.rs|reviewed|targeted-read|Verified ctl_ipc and emu_server virtual-backend gating
tools/rabbiz-de/crates/de-bin/src/auth_helper.rs|reviewed|targeted-read|Verified auth helper credential storage and secret zeroization
tools/rabbiz-de/crates/de-system/src/auth.rs|reviewed|targeted-read|Verified HelperRequest Drop zeroize and HelperResponse no-echo
docs/PLAN.md|reviewed|rg-scan|Found committed device serial (line 27) and developer host path (line 8)
devices/samsung/a21s/docs/a21s-flash-boot-checklist.md|reviewed|rg-scan|Found committed private LAN IP (line 24)
devices/samsung/a21s/docs/HANDOFF-kernel-lane.md|reviewed|rg-scan|Found committed private LAN IP (line 9)
devices/samsung/a21s/findings/logs/2026-11-18-s2mu106-charger-online-props.txt|reviewed|rg-scan|Found committed private LAN IP (line 9)
```
