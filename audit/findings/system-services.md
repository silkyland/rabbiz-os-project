# Domain Audit: System Services (`SVC`)

## Scope & Files Reviewed

### Files Inspected (Direct Read)
- Systemd unit files (`rootfs-overlay/etc/systemd/system/`):
  - `rootfs-overlay/etc/systemd/system/a21s-display-pin.service` (`direct-read`)
  - `rootfs-overlay/etc/systemd/system/a21s-drm-keeper.service` (`direct-read`)
  - `rootfs-overlay/etc/systemd/system/a21s-modem-init.service` (`direct-read`)
  - `rootfs-overlay/etc/systemd/system/abox-audio.service` (`direct-read`)
  - `rootfs-overlay/etc/systemd/system/fex-guestos.service` (`direct-read`)
  - `rootfs-overlay/etc/systemd/system/rabbiz-auth-helper.service` (`direct-read`)
  - `rootfs-overlay/etc/systemd/system/rabbiz-session.service` (`direct-read`)
  - `rootfs-overlay/etc/systemd/system/rabbiz-splash.service` (`direct-read`)
  - `rootfs-overlay/etc/systemd/system/s2mu106-charger.service` (`direct-read`)
  - `rootfs-overlay/etc/systemd/system/volume-keys.service` (`direct-read`)
- System scripts & daemons (`rootfs-overlay/usr/local/bin/`):
  - `rootfs-overlay/usr/local/bin/a21s-drm-keeper` (`direct-read`)
  - `rootfs-overlay/usr/local/bin/a21s-modem-ctl` (`direct-read`)
  - `rootfs-overlay/usr/local/bin/a21s-power-key` (`direct-read`)
  - `rootfs-overlay/usr/local/bin/a21s-qt-gpu` (`direct-read`)
  - `rootfs-overlay/usr/local/bin/a21s-qt-render` (`direct-read`)
  - `rootfs-overlay/usr/local/bin/a21s-telemetry` (`direct-read`)
  - `rootfs-overlay/usr/local/bin/a21s-volume` (`direct-read`)
  - `rootfs-overlay/usr/local/bin/a21s_holdmenu.py` (`direct-read`)
  - `rootfs-overlay/usr/local/bin/rabbiz-appearance` (`direct-read`)
  - `rootfs-overlay/usr/local/bin/rabbiz-labwc-session` (`direct-read`)
  - `rootfs-overlay/usr/local/bin/rabbiz-osk-ctl` (`direct-read`)
  - `rootfs-overlay/usr/local/bin/rabbiz-osk-switch` (`direct-read`)
  - `rootfs-overlay/usr/local/bin/rabbiz-screenshot` (`direct-read`)
  - `rootfs-overlay/usr/local/bin/rabbiz-session` (`direct-read`)
  - `rootfs-overlay/usr/local/bin/rabbiz-shell` (`direct-read`)
  - `rootfs-overlay/usr/local/bin/rabbiz-splash` (`direct-read`)
  - `rootfs-overlay/usr/local/bin/rabbiz-spotify-ctl` (`direct-read`)
  - `rootfs-overlay/usr/local/bin/rabbiz-wifi-connect` (`direct-read`)
  - `rootfs-overlay/usr/local/bin/volume-keys.py` (`direct-read`)
- C modem boot daemon (`tools/cbd/`):
  - `tools/cbd/a21s-cbd.c` (`direct-read`)
  - `tools/cbd/Makefile` (`direct-read`)
- Rust IPC & privileged auth helper:
  - `tools/rabbiz-de/crates/de-bin/src/auth_helper.rs` (`direct-read`)
  - `tools/rabbiz-de/crates/de-config/src/ipc.rs` (`direct-read`)
  - `tools/rabbiz-de/crates/de-core/src/runtime/ctl_ipc.rs` (`direct-read`)
  - `tools/rabbiz-de/crates/de-system/src/auth.rs` (`targeted-read`, lines 1–200 and 400–600, to verify `root_keyguard_paths`, `HelperRequest`, and `LockoutPolicy` invoked by `auth_helper.rs`)
- Static analysis triage inputs:
  - `audit/tools/shellcheck.txt` (`direct-read`)
  - `audit/tools/code-patterns.txt` (`direct-read`)

### Files NOT Inspected (Out of Scope / External)
- `rootfs-overlay/etc/systemd/system/getty@tty1.service`: symlink to `/dev/null` (masked unit, no content).
- `scripts/audio-up.sh` and `scripts/charger-up.sh`: referenced at runtime as `/root/audio-up.sh` and `/root/charger-up.sh` by `abox-audio.service` and `s2mu106-charger.service`, but live under `scripts/` (audited under Build/Deploy Scripts domain).
- `rootfs-overlay/etc/polkit-1/` and `rootfs-overlay/etc/udev/rules.d/`: referenced as cross-domain leads (audited under Privileges/Polkit/Udev domain).
- `socs/exynos850/drivers/cpif/` kernel driver side of `/dev/umts_boot0` ioctls (audited under Kernel/Drivers domain).

---

## Findings

```
ID: SVC-001
Title: Local Privilege Escalation (rabbiz -> root) via Symlink Following and chown/chmod in Root-Invoked Hardware Key Scripts
Severity: High
Confidence: Confirmed
Location: rootfs-overlay/etc/systemd/system/volume-keys.service:6, rootfs-overlay/usr/local/bin/a21s-volume:107-122, rootfs-overlay/usr/local/bin/a21s-power-key:35-42, rootfs-overlay/usr/local/bin/rabbiz-screenshot:40-77, rootfs-overlay/usr/local/bin/volume-keys.py:30-45
Evidence:
--- rootfs-overlay/etc/systemd/system/volume-keys.service:5-6 ---
[Service]
ExecStart=/usr/local/bin/volume-keys.py

--- rootfs-overlay/usr/local/bin/volume-keys.py:30,40-45,102,136,172,185 ---
MARKER = "/run/user/%d/a21s-key-combo" % pwd.getpwnam("rabbiz").pw_uid
...
def mark_volume_press():
    try:
        with open(MARKER, "w") as f:
            f.write("1")
...
run_async(["/usr/local/bin/a21s-power-key", "menu"])
run_async(["/usr/local/bin/rabbiz-screenshot"])
pending = (now + VOLUME_DELAY_SEC, ["/usr/local/bin/a21s-volume", step, "5"])

--- rootfs-overlay/usr/local/bin/a21s-volume:107-122 ---
    printf '%s:%s\n' "${dir}" "${now_ms}" > "${STAMP_FILE}" 2>/dev/null || true
    if [ "$(id -u)" -eq 0 ]; then
        chown "${RABBIZ_USER}:${RABBIZ_USER}" "${STAMP_FILE}" 2>/dev/null || true
        chmod 664 "${STAMP_FILE}" 2>/dev/null || true
    fi
...
exec 9>"${LOCK_FILE}"
if [ "$(id -u)" -eq 0 ]; then
    chown "${RABBIZ_USER}:${RABBIZ_USER}" "${LOCK_FILE}" 2>/dev/null || true
    chmod 666 "${LOCK_FILE}" 2>/dev/null || true
fi

--- rootfs-overlay/usr/local/bin/a21s-power-key:35-42 ---
CMD_FILE="${RUNTIME_DIR}/a21s-power.cmd"
NONCE="$(date +%s%N 2>/dev/null || date +%s)-$$"

printf '%s:%s\n' "$ACTION" "$NONCE" > "$CMD_FILE"
if [ "$(id -u)" -eq 0 ]; then
  chown rabbiz:rabbiz "$CMD_FILE" 2>/dev/null || true
  chmod 664 "$CMD_FILE" 2>/dev/null || true
fi

--- rootfs-overlay/usr/local/bin/rabbiz-screenshot:41-43,54-77 ---
if [ "$(id -u)" -eq 0 ]; then
    chown "${RABBIZ_USER}:${RABBIZ_USER}" "/home/${RABBIZ_USER}/Pictures" "${SCREENSHOT_DIR}" 2>/dev/null || true
fi
...
if [ "$(id -u)" -eq 0 ]; then
    runuser -u "${RABBIZ_USER}" -- env ... grim "${OUT_PATH}"
    chown "${RABBIZ_USER}:${RABBIZ_USER}" "${OUT_PATH}" 2>/dev/null || true
...
TMP_STATE="${STATE_FILE}.tmp.$$"
printf '%s|%s|%s\n' "${OUT_PATH}" "${FILENAME}" "${NONCE}" > "${TMP_STATE}"
mv -f "${TMP_STATE}" "${STATE_FILE}"
Impact: Arbitrary root-owned file overwrite, arbitrary file ownership takeover (chown to rabbiz:rabbiz), and world-writable permission change (chmod 666), enabling full local privilege escalation from the unprivileged `rabbiz` session user to `root`.
Exploit/Trigger: `volume-keys.service` runs as `root` and operates on files inside `/run/user/<uid>` and `/home/rabbiz/Pictures/Screenshots`, both of which are non-sticky directories owned by `rabbiz`. Linux `fs.protected_symlinks` only restricts symlink following in sticky world-writable directories (e.g., `/tmp`), NOT in user-owned non-sticky directories. If a process running as `rabbiz` replaces `a21s-volume.lock`, `a21s-volume.stamp`, `a21s-power.cmd`, `a21s-key-combo`, or `Pictures/Screenshots` with a symlink pointing to a root-owned file (such as a root-executed script or system configuration file), the next hardware volume/power key press causes `root` to follow the symlink, truncate/overwrite the target, and `chown`/`chmod 666` it to `rabbiz`.
Recommendation:
1. Run `volume-keys.service` as an unprivileged user (or `User=rabbiz` with `SupplementaryGroups=input`) so it never holds `root` privileges. If `ensure_amp` in `a21s-volume` needs privileged sysfs access, isolate that single hardware operation rather than running the entire input/screenshot/volume pipeline as `root`.
2. Never open, write, `chown`, or `chmod` files inside user-owned directories as `root`. Perform all writes inside `/run/user/<uid>` and `/home/rabbiz` after dropping privileges (`runuser -u rabbiz`), or use `O_NOFOLLOW` / `chown -h` and root-owned runtime directories (`/run/rabbiz-keys/`).
3. Avoid predictable `.tmp.$$` filenames; use `mktemp` inside a directory owned by the effective UID.
References: CWE-59 (Improper Link Resolution Before File Access), CWE-250 (Execution with Unnecessary Privileges)
```

```
ID: SVC-002
Title: Complete Passphrase Keyguard Authentication Bypass and Lockout Reset in `rabbiz-auth-helper`
Severity: High
Confidence: Confirmed
Location: tools/rabbiz-de/crates/de-bin/src/auth_helper.rs:29-93, rootfs-overlay/etc/systemd/system/rabbiz-auth-helper.service:5-7, tools/rabbiz-de/crates/de-system/src/auth.rs:483-495
Evidence:
--- tools/rabbiz-de/crates/de-bin/src/auth_helper.rs:29-45 ---
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

--- tools/rabbiz-de/crates/de-bin/src/auth_helper.rs:47-82 ---
fn handle_request(req: &HelperRequest) -> HelperResponse {
    let Some((cred_path, state_path)) = root_keyguard_paths(&req.user) else {
        return HelperResponse::Unavailable;
    };
    ...
    let ok = match req.method {
        LockMethod::Swipe => true,
        ...
        LockMethod::Passphrase => verify_pam_passphrase(&req.user, &req.secret),
    };

    let outcome = policy.record_attempt(ok, now_s);
    ...
    let _ = save_lockout_state_atomic(&state_path, policy.state());

--- tools/rabbiz-de/crates/de-system/src/auth.rs:483-495 ---
pub fn root_keyguard_paths(user: &str) -> Option<(PathBuf, PathBuf)> {
    let clean = user.trim();
    if clean.is_empty()
        || clean == "."
        || clean == ".."
        || clean.contains('/')
        || clean.contains('\0')
    {
        return None;
    }
    let base = Path::new("/var/lib/rabbiz-auth").join(clean);
    Some((base.join("keyguard.json"), base.join("keyguard.state")))
}
Impact:
1. Complete authentication bypass in Passphrase mode: `rabbiz-auth-helper` runs as `root` (no `User=` in `rabbiz-auth-helper.service`). On Arch Linux and standard Linux distributions, `/etc/pam.d/su` includes `auth sufficient pam_rootok.so`, so `/bin/su -c true <user>` invoked by UID 0 immediately succeeds without checking the passphrase (and `su` reads passwords from `/dev/tty`, not piped stdin). Any passphrase—including an empty string—unlocks the device.
2. Brute-force lockout bypass & cross-user interference: `auth_helper.rs` performs no `SO_PEERCRED` check and trusts client-supplied `req.user` and `req.method`. Any client able to connect to `/run/rabbiz-auth/helper.sock` can send `{"user":"<target>","method":"swipe","secret":""}` to force `ok = true` and reset `consecutive_failures = 0` in `keyguard.state` (before cooldown), or exhaust another user's lockout attempts.
3. Argument injection into `/bin/su`: `root_keyguard_paths()` permits usernames beginning with `-`, and `verify_pam_passphrase()` passes `user` to `/bin/su` without `--`.
Exploit/Trigger: Configure Keyguard to `Passphrase` mode and submit any arbitrary string on the lock screen (or connect to `/run/rabbiz-auth/helper.sock` and send `method: "swipe"` between failed PIN attempts to reset the failure counter).
Recommendation:
1. Replace `/bin/su` with direct PAM authentication via `libpam` (using a dedicated `/etc/pam.d/rabbiz-keyguard` service that does NOT include `pam_rootok.so` and uses a proper PAM conversation callback passing the secret in memory).
2. Enforce `SO_PEERCRED` (`getsockopt(SO_PEERCRED)`) on every accepted Unix socket connection and derive the target user from the peer UID (via `getpwuid`), rejecting mismatched `req.user` unless peer UID is 0.
3. Do not trust `req.method` from the client: store the authoritative configured `LockMethod` inside root-owned `/var/lib/rabbiz-auth/<user>/` and refuse `LockMethod::Swipe` if PIN or Passphrase is configured.
4. Validate usernames strictly against POSIX username rules (`^[a-z_][a-z0-9_-]*$`).
References: CWE-287 (Improper Authentication), CWE-307 (Improper Restriction of Excessive Authentication Attempts), CWE-88 (Improper Neutralization of Argument Delimiters)
```

```
ID: SVC-003
Title: OS Command Injection via Wi-Fi Profile Name / SSID in `rabbiz-wifi-connect`
Severity: High
Confidence: Confirmed
Location: rootfs-overlay/usr/local/bin/rabbiz-wifi-connect:22-26,31-44
Evidence:
--- rootfs-overlay/usr/local/bin/rabbiz-wifi-connect:22-44 ---
ask() {
    local what="$1"; shift
    ( sleep 2; printf '%s\n' "$PSK" ) \
        | timeout 45 script -qec "stty -echo; nmcli --ask $what" /dev/null
}

# 1) Find an existing profile for this SSID: `nmcli device wifi connect` otherwise
#    creates "SSID 1", "SSID 2"... on every join.
PROFILE=""
for name in $(nmcli -t -f NAME,TYPE connection show 2>/dev/null \
        | awk -F: '$2 == "802-11-wireless" { print $1 }'); do
    if [ "$(nmcli -t -g 802-11-wireless.ssid connection show "$name" 2>/dev/null)" = "$SSID" ]; then
        PROFILE="$name"
        break
    fi
done

if [ -n "$PROFILE" ]; then
    if [ -n "$PSK" ]; then
        ask "connection modify \"$PROFILE\" wifi-sec.psk" || true
    fi
    exec timeout 40 nmcli connection up id "$PROFILE"
fi
Impact: Arbitrary shell command execution as the invoking user (`rabbiz`) when reconnecting to or updating the PSK of a Wi-Fi network whose NetworkManager profile name (normally derived from the AP's broadcast SSID) contains shell metacharacters (`$(...)`, backticks, or `"`). Additionally, unquoted `$(nmcli ...)` on lines 31–32 splits profile names containing spaces and expands glob characters (`*`, `?`, `[...]`).
Exploit/Trigger: A nearby malicious Wi-Fi access point broadcasts an SSID containing shell command substitution syntax (e.g., `$(...)` or `"; ... #`). Once a profile exists for that SSID and the user attempts to connect/update the PSK via `rabbiz-wifi-connect`, `script -qec` invokes `/bin/sh -c` with `$PROFILE` interpolated inside double quotes, executing the embedded payload.
Recommendation:
1. Do not construct shell command strings for `script -c`. Instead, pass `$PROFILE` and `$SSID` via environment variables or positional arguments to a non-interpolated script (e.g., `PROFILE="$PROFILE" timeout 45 script -qec 'stty -echo; exec nmcli --ask connection modify "$PROFILE" wifi-sec.psk' /dev/null`), or use Python `pty` + `subprocess` with an argument list (`shell=False`) or NetworkManager's D-Bus API directly.
2. Replace `for name in $(...)` with `while IFS= read -r name; do ... done < <(...)` to prevent word splitting and glob expansion.
References: CWE-78 (Improper Neutralization of Special Elements used in an OS Command)
```

```
ID: SVC-004
Title: World-Writable Fingerprint Unlock Trigger & Sensor Device (`0666`) and Unprivileged `video` Group Access to Modem Boot Device
Severity: High
Confidence: Confirmed
Location: rootfs-overlay/usr/local/bin/a21s-modem-ctl:12-17, rootfs-overlay/etc/systemd/system/rabbiz-session.service:34
Evidence:
--- rootfs-overlay/usr/local/bin/a21s-modem-ctl:12-17 ---
fix_perms() {
  chmod 666 "$FP_SYS/trigger_unlock" "$FP_SYS/sleep_pin" 2>/dev/null || true
  chmod 666 /dev/esfp0 2>/dev/null || true
  chgrp video /dev/umts_* 2>/dev/null || true
  chmod 660 /dev/umts_* 2>/dev/null || true
}

--- rootfs-overlay/etc/systemd/system/rabbiz-session.service:31,34 ---
User=rabbiz
SupplementaryGroups=seat input video render audio
Impact:
1. Any local process or user can write `1` to `/sys/kernel/a21s_fp/trigger_unlock` (triggering biometric unlock events) or open `/dev/esfp0` directly to interact with or sniff the ET520 fingerprint sensor, bypassing hardware biometric trust boundaries.
2. Because the graphical session user `rabbiz` (and every app running under it) holds supplementary group `video`, `chmod 660` + `chgrp video` on `/dev/umts_*` grants every unprivileged user application direct read/write and `ioctl` access to `/dev/umts_boot0` (modem power/reset/firmware upload via `CPIF_IOC_LOAD_CP_IMAGE` / `CPIF_IOC_POWER_ON`) and cellular IPC nodes.
Exploit/Trigger: Any unprivileged local process can open `/sys/kernel/a21s_fp/trigger_unlock` or `/dev/esfp0` for writing, or any process running as `rabbiz` (group `video`) can open `/dev/umts_boot0` and issue modem boot/control ioctls.
Recommendation:
1. Restrict `/dev/esfp0` and `/sys/kernel/a21s_fp/*` to `root` or a dedicated `fprintd` service user (`0660 root:fprintd` or `0600 root:root`), never `0666`. Also update matching udev rules (`rootfs-overlay/etc/udev/rules.d/99-a21s-modem-fp.rules`).
2. Keep `/dev/umts_boot0` `0600 root:root` (only `a21s-cbd` under `a21s-modem-init.service` needs boot access). Assign `/dev/umts_ipc*` / RIL nodes to a dedicated `modem` group rather than `video`.
References: CWE-732 (Incorrect Permission Assignment for Critical Resource), CWE-276 (Incorrect Default Permissions)
```

```
ID: SVC-005
Title: Blocking Single-Threaded Loop, Unbounded Read (DoS/OOM), and Socket Group Misconfiguration in `rabbiz-auth-helper`
Severity: Medium
Confidence: Confirmed
Location: tools/rabbiz-de/crates/de-bin/src/auth_helper.rs:95-113,125-136, rootfs-overlay/etc/systemd/system/rabbiz-auth-helper.service:5-11
Evidence:
--- tools/rabbiz-de/crates/de-bin/src/auth_helper.rs:95-101,125-136 ---
fn handle_client(stream: UnixStream) {
    let mut reader = BufReader::new(&stream);
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        line.zeroize();
        return;
    }
...
    let _ = std::fs::remove_file(path);
    let listener = match UnixListener::bind(path) { ... };
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o660));
    for conn in listener.incoming().flatten() {
        handle_client(conn);
    }

--- rootfs-overlay/etc/systemd/system/rabbiz-auth-helper.service:5-11 ---
[Service]
Type=simple
ExecStart=/usr/local/bin/rabbiz-auth-helper
RuntimeDirectory=rabbiz-auth
RuntimeDirectoryMode=0755
StateDirectory=rabbiz-auth
StateDirectoryMode=0700
Impact:
1. Permanent Keyguard DoS: `handle_client` runs synchronously in the main accept loop with no `set_read_timeout`/`set_write_timeout`. Any client that connects and does not send a newline permanently stalls `rabbiz-auth-helper`, making the device impossible to unlock.
2. Memory exhaustion (OOM): `reader.read_line(&mut line)` has no byte cap (unlike `de-config/src/ipc.rs` which uses `.take(MAX_MESSAGE_BYTES + 1)`). Streaming data without `\n` causes unbounded heap growth in a root daemon.
3. Socket permission race & group mismatch: `set_permissions(0o660)` runs after `UnixListener::bind()` (TOCTOU window), and no `Group=` is configured in `rabbiz-auth-helper.service` or via `chown`, leaving `helper.sock` owned by `root:root` (`0660`) so unprivileged `rabbiz` cannot connect unless externally chgrp'd.
Exploit/Trigger: Any local process permitted to connect to `/run/rabbiz-auth/helper.sock` opens a connection and either stalls or sends continuous non-newline bytes.
Recommendation:
1. Set strict read/write timeouts (e.g., 2 s) on accepted `UnixStream`s, cap reads with `reader.take(4096)` before `read_line`, and handle clients in bounded worker threads or a non-blocking poll loop.
2. Set `umask(0o117)` (or `0o177`) around `bind()`, or use systemd socket activation (`rabbiz-auth-helper.socket` with `SocketMode=0660` and `SocketGroup=rabbiz` / dedicated group) and `RuntimeDirectoryMode=0750`.
References: CWE-400 (Uncontrolled Resource Consumption), CWE-770 (Allocation of Resources Without Limits or Throttling), CWE-362 (Race Condition)
```

```
ID: SVC-006
Title: Missing Systemd Sandboxing Across Root Daemons, Non-Standard `/root/*.sh` Units, and Predictable `/tmp` Log File
Severity: Medium
Confidence: Confirmed
Location: rootfs-overlay/etc/systemd/system/rabbiz-auth-helper.service:5-14, rootfs-overlay/etc/systemd/system/volume-keys.service:5-9, rootfs-overlay/etc/systemd/system/a21s-drm-keeper.service:10-14, rootfs-overlay/etc/systemd/system/abox-audio.service:8, rootfs-overlay/etc/systemd/system/s2mu106-charger.service:8, rootfs-overlay/usr/local/bin/rabbiz-osk-switch:12,45
Evidence:
--- rootfs-overlay/etc/systemd/system/abox-audio.service:5-9 ---
[Service]
Type=oneshot
RemainAfterExit=yes
ExecStart=/root/audio-up.sh
TimeoutStartSec=60

--- rootfs-overlay/etc/systemd/system/s2mu106-charger.service:5-9 ---
[Service]
Type=oneshot
RemainAfterExit=yes
ExecStart=/root/charger-up.sh
TimeoutStartSec=30

--- rootfs-overlay/usr/local/bin/rabbiz-osk-switch:12,42-45 ---
LOG="/tmp/rabbiz-osk.log"
...
    setsid runuser -u rabbiz -- env \
      XDG_RUNTIME_DIR="$RUNTIME" WAYLAND_DISPLAY=wayland-0 \
      DBUS_SESSION_BUS_ADDRESS="unix:path=${RUNTIME}/bus" \
      /usr/local/bin/rabbiz-osk >"$LOG" 2>&1 &
Impact:
1. None of the 8 root-running systemd services (`a21s-display-pin`, `a21s-drm-keeper`, `a21s-modem-init`, `abox-audio`, `fex-guestos`, `rabbiz-auth-helper`, `rabbiz-splash`, `s2mu106-charger`, `volume-keys`) configure systemd sandboxing (`NoNewPrivileges=yes`, `ProtectSystem=strict`, `ProtectHome=yes`, `PrivateTmp=yes`, `CapabilityBoundingSet=`, `RestrictAddressFamilies=`). Any bug in a daemon immediately yields unrestricted root access.
2. `abox-audio.service` and `s2mu106-charger.service` execute scripts from `/root/` rather than `/usr/local/sbin/` or `/usr/libexec/`, coupling boot services to root's home directory.
3. `rabbiz-osk-switch` (documented as "Run as root") opens predictable `/tmp/rabbiz-osk.log` with root's shell redirection `>"$LOG"` before `runuser` drops privileges, creating a `/tmp` pre-creation / DoS (or symlink, if `fs.protected_symlinks=0`) risk.
Exploit/Trigger: Post-exploitation blast radius expansion if any root daemon is compromised; or local user pre-creating `/tmp/rabbiz-osk.log` before root runs `rabbiz-osk-switch start`.
Recommendation:
1. Add least-privilege systemd hardening to all service units (e.g., `NoNewPrivileges=yes`, `ProtectSystem=strict`, `ProtectHome=yes`, `PrivateTmp=yes`, `ProtectKernelLogs=yes`, `ProtectControlGroups=yes`, `RestrictNamespaces=yes`, and minimal `CapabilityBoundingSet=` / `ReadWritePaths=`).
2. Relocate `/root/audio-up.sh` and `/root/charger-up.sh` to `/usr/local/libexec/rabbiz/` (mode `0755`, owned by `root:root`) and track them in `rootfs-overlay/`.
3. In `rabbiz-osk-switch`, write the log inside `$RUNTIME` (`$RUNTIME/rabbiz-osk.log`) after dropping privileges to `rabbiz`, or use `systemd-cat`.
References: CWE-250 (Execution with Unnecessary Privileges), CWE-377 (Insecure Temporary File)
```

```
ID: SVC-007
Title: Local Control IPC Denial of Service via Persistent Idle Clients in `CtlIpc` and Unbounded Clients in `EmuFrameServer`
Severity: Medium
Confidence: Confirmed
Location: tools/rabbiz-de/crates/de-core/src/runtime/ctl_ipc.rs:67-70,161-193,202-241,322-340
Evidence:
--- tools/rabbiz-de/crates/de-core/src/runtime/ctl_ipc.rs:67-70,161-166 ---
struct CtlClient {
    stream: CtlStream,
    rx_buf: Vec<u8>,
}
...
    fn accept_pending(&mut self) {
        const MAX_CLIENTS: usize = 64;
        for _ in 0..8 {
            if self.clients.len() >= MAX_CLIENTS {
                break;
            }

--- tools/rabbiz-de/crates/de-core/src/runtime/ctl_ipc.rs:322-339 ---
    fn accept_pending(&mut self) {
        for _ in 0..4 {
            match self.listener.accept() {
                Ok((stream, _)) => {
                    let _ = stream.set_nonblocking(true);
                    let _ = stream.set_nodelay(true);
                    self.clients.push(EmuClient {
                        stream,
                        rx_buf: Vec::with_capacity(512),
                    });
                }
Impact:
1. In `CtlIpc`, clients that connect and never send `\n` (and never close) remain in `self.clients` indefinitely because `CtlClient` tracks no `connected_at` / idle deadline. Opening 64 idle connections permanently prevents `CtlIpc` from accepting any new control requests (e.g., `rabbiz-de ctl lock`, OSD updates, screenshots).
2. In `EmuFrameServer` (used with `--emu-listen`), `accept_pending` enforces no `MAX_CLIENTS` limit at all, allowing unbounded file descriptor and memory consumption from local TCP connections.
Exploit/Trigger: Any process running as `rabbiz` (or any local user if `--ctl-listen` / `--emu-listen` loopback TCP is enabled) opens 64 idle sockets to `rabbiz-de-ctl.sock` (or unbounded sockets to `--emu-listen`).
Recommendation:
1. Add `accepted_at: Instant` to `CtlClient` and evict clients that do not deliver a complete line within a short deadline (e.g., 2–5 seconds).
2. Enforce a `MAX_EMU_CLIENTS` cap (e.g., 8 or 16) and idle timeout on `EmuFrameServer`.
References: CWE-400 (Uncontrolled Resource Consumption), CWE-770 (Allocation of Resources Without Limits or Throttling)
```

```
ID: SVC-008
Title: Chromium Browser Launched with `--no-sandbox` and `mpv` Argument Injection in `rabbiz-spotify-ctl`
Severity: Medium
Confidence: Confirmed
Location: rootfs-overlay/usr/local/bin/rabbiz-spotify-ctl:277-289,366-376
Evidence:
--- rootfs-overlay/usr/local/bin/rabbiz-spotify-ctl:366-376 ---
cmd_web_app() {
    local target_url="${1:-https://open.spotify.com}"
    if command -v angelfish >/dev/null 2>&1; then
        nohup env \
            QT_QPA_PLATFORM=wayland \
            QT_QUICK_BACKEND=software \
            QT_QUICK_CONTROLS_MOBILE=1 \
            QTWEBENGINE_CHROMIUM_FLAGS="--no-sandbox --disable-gpu" \
            angelfish "${target_url}" >/dev/null 2>&1 </dev/null &

--- rootfs-overlay/usr/local/bin/rabbiz-spotify-ctl:277-289 ---
    nohup mpv \
        ...
        --input-ipc-server="${MPV_SOCK}" \
        "${url}" \
        >"${MPV_LOG_FILE}" 2>&1 </dev/null &
Impact:
1. Running QtWebEngine/Chromium (`angelfish`) with `--no-sandbox` against remote web content (`https://open.spotify.com` or arbitrary URLs) removes the renderer sandbox, turning any browser renderer/JS vulnerability into direct code execution as `rabbiz`.
2. Passing `"${url}"` to `mpv` without `--` allows a crafted URL starting with `-` (e.g., `--script=...`) passed to `rabbiz-spotify-ctl play-url` to be parsed as an `mpv` option (argument injection), which in `mpv` can load arbitrary Lua scripts.
Exploit/Trigger: Clicking "Spotify Web" or invoking `rabbiz-spotify-ctl web-app` runs an unsandboxed browser; invoking `rabbiz-spotify-ctl play-url "--..."` passes attacker-controlled options to `mpv`.
Recommendation:
1. Remove `--no-sandbox` from `QTWEBENGINE_CHROMIUM_FLAGS` (ensure kernel user namespaces / setuid sandbox helper are properly enabled instead).
2. Add `--` before `"${url}"` in the `mpv` invocation and validate that `${url}` begins with `https://`, `http://`, or an allowed local path prefix.
References: CWE-88 (Improper Neutralization of Argument Delimiters), CWE-693 (Protection Mechanism Failure)
```

```
ID: SVC-009
Title: Unreaped Child Processes (Zombie Leak) in `volume-keys.py`
Severity: Low
Confidence: Confirmed
Location: rootfs-overlay/usr/local/bin/volume-keys.py:48-49
Evidence:
--- rootfs-overlay/usr/local/bin/volume-keys.py:48-49 ---
def run_async(argv):
    subprocess.Popen(argv, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
Impact: Every volume press, mute toggle, power-menu hold, or screenshot combo spawns a child process via `subprocess.Popen` whose exit status is never reaped. Because `volume-keys.py` is a long-running system daemon, zombie processes accumulate indefinitely until the service restarts or the PID table is exhausted.
Exploit/Trigger: Normal device usage over extended uptime (each volume or screenshot key press adds one zombie entry).
Recommendation: Register `signal.signal(signal.SIGCHLD, signal.SIG_IGN)` at startup (or track `Popen` handles and call `.poll()` on each event-loop iteration) so the kernel automatically reaps terminated children.
References: CWE-404 (Improper Resource Shutdown or Release)
```

```
ID: SVC-010
Title: `a21s-cbd.c` Error-Handling, Argument Validation, and Firmware Integrity Defects
Severity: Low
Confidence: Confirmed
Location: tools/cbd/a21s-cbd.c:207,246-248,260,289-324,358,377,380-395
Evidence:
--- tools/cbd/a21s-cbd.c:206-207,246-248 ---
		else if (!strcmp(argv[i], "--chunk") && i + 1 < argc)
			chunk = strtoul(argv[++i], NULL, 0);
...
		off_t end = lseek(src, 0, SEEK_END);
		total = radio_size ? radio_size : (uint64_t)(end - (off_t)radio_off);
		lseek(src, (off_t)radio_off, SEEK_SET);

--- tools/cbd/a21s-cbd.c:357-359,375-387,395 ---
		ret = xioctl(fd, CPIF_IOC_REQ_SECURITY, &msr, "REQ_SECURITY");
		if (ret < 0 && ret != -EOPNOTSUPP)
			goto out;
...
	ret = xioctl(fd, CPIF_IOC_COMPLETE_NORMAL_BOOTUP, NULL,
		     "COMPLETE_NORMAL_BOOTUP");
	if (ret < 0 && ret != -EOPNOTSUPP)
		goto out;

	for (i = 0; i < 100; i++) {
		ret = xioctl(fd, CPIF_IOC_GET_CP_STATUS, NULL, "GET_CP_STATUS");
		if (ret == CPIF_STATE_ONLINE) {
			printf("[+] modem ONLINE\n");
			break;
		}
		usleep(100 * 1000);
	}
...
	return ret < 0 ? 1 : 0;
Impact:
1. Broken `EOPNOTSUPP` handling (lines 358, 377): POSIX `ioctl()` returns `-1` and sets `errno`, never `-EOPNOTSUPP`, so `ret != -EOPNOTSUPP` is always true on error and the intended fallback never triggers.
2. Silent timeout success (lines 380–395): If the modem never reaches `CPIF_STATE_ONLINE` after 100 iterations, `ret` holds a non-negative state value (e.g., `CPIF_STATE_OFFLINE`) and `main()` exits `0`.
3. Unvalidated `--chunk` (line 207): `--chunk 0` or huge values are not rejected; unchecked `lseek()` return value (line 246) can underflow `total` when `end < 0` or `end < (off_t)radio_off`.
4. Unchecked TOC metadata (lines 164, 289–324): `toc[k].crc` is never verified, and `cp_off + done` / `radio_off + toc[k].img_offset` lack overflow checks.
Exploit/Trigger: Invoking `a21s-cbd` with malformed options/radio images or encountering `EOPNOTSUPP` / modem boot timeout during `a21s-modem-init.service`.
Recommendation:
1. Check `errno != EOPNOTSUPP` on lines 358 and 377.
2. Track `int online = 0;` in the status polling loop and return `1` if the modem did not come online.
3. Validate `chunk >= 4096 && chunk <= (16 * 1024 * 1024)`, check `end >= 0 && (uint64_t)end >= radio_off`, and add overflow checks for `cp_off + done` and `radio_off + toc[k].img_offset`.
4. Verify `toc[k].crc` (or document clearly that integrity verification is delegated to TrustZone via `CPIF_IOC_REQ_SECURITY`, and avoid printing `(TOC validated)` in `--load-only` mode).
References: CWE-252 (Unchecked Return Value), CWE-190 (Integer Overflow or Wraparound)
```

```
ID: SVC-011
Title: Missing Executable Bits in Overlay Repo, Non-POSIX `test -O`, and Hardcoded UID 1001 in `rabbiz-session.service`
Severity: Low
Confidence: Confirmed
Location: rootfs-overlay/usr/local/bin/rabbiz-session:1, rootfs-overlay/usr/local/bin/a21s-drm-keeper:1, rootfs-overlay/usr/local/bin/rabbiz-wifi-connect:1, rootfs-overlay/usr/local/bin/rabbiz-osk-ctl:13, rootfs-overlay/etc/systemd/system/rabbiz-session.service:26-27,38-39
Evidence:
--- ls -la rootfs-overlay/usr/local/bin/ ---
-rw-r--r-- 1 dit dit   723 ... a21s-drm-keeper
-rw-r--r-- 1 dit dit  2112 ... rabbiz-osk-ctl
-rw-r--r-- 1 dit dit  8310 ... rabbiz-session
-rw-r--r-- 1 dit dit  2181 ... rabbiz-wifi-connect

--- rootfs-overlay/usr/local/bin/rabbiz-osk-ctl:1,13 ---
#!/bin/sh
...
[ -O "$RD" ] || RD="/run/user/$(id -u)"

--- rootfs-overlay/etc/systemd/system/rabbiz-session.service:26-27,38-39 ---
Wants=user@1001.service
After=user@1001.service systemd-user-sessions.service
...
Environment=XDG_RUNTIME_DIR=/run/user/1001
Environment=DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1001/bus
Impact:
1. Directly overlaying `rootfs-overlay/` onto a rootfs without a deploy script that runs `chmod +x` causes `rabbiz-session.service` and `a21s-drm-keeper.service` to fail with `EACCES`.
2. `[ -O "$RD" ]` in `rabbiz-osk-ctl` is undefined in POSIX `sh` (ShellCheck SC3067) and fails on strict POSIX shells (e.g., `dash`).
3. Hardcoding UID `1001` in `rabbiz-session.service` breaks session D-Bus/runtime dir wiring if the `rabbiz` account is created with UID `1000` on a fresh rootfs.
Exploit/Trigger: Fresh rootfs assembly directly from `rootfs-overlay/`, or running `rabbiz-osk-ctl` under a strict POSIX `/bin/sh`.
Recommendation:
1. `chmod +x` the four scripts in git (`a21s-drm-keeper`, `rabbiz-osk-ctl`, `rabbiz-session`, `rabbiz-wifi-connect`).
2. Change `rabbiz-osk-ctl` shebang to `#!/usr/bin/env bash` or replace `[ -O "$RD" ]` with `[ "$(stat -c %u "$RD" 2>/dev/null)" = "$(id -u)" ]`.
3. Let `PAMName=login` (`pam_systemd`) populate `XDG_RUNTIME_DIR` and `DBUS_SESSION_BUS_ADDRESS` dynamically (or use systemd specifiers where supported) instead of hardcoding `1001`.
References: CWE-732 (Incorrect Permission Assignment), ShellCheck SC3067
```

```
ID: SVC-012
Title: Ephemeral Screenshot Fallback to Predictable `/tmp` Path in `ctl_ipc.rs`
Severity: Low
Confidence: Confirmed
Location: tools/rabbiz-de/crates/de-core/src/runtime/ctl_ipc.rs:923-931,1053-1074
Evidence:
--- tools/rabbiz-de/crates/de-core/src/runtime/ctl_ipc.rs:1053-1074 ---
fn screenshot_path(save: bool, timestamp_ms: u128) -> PathBuf {
    if save {
        let pictures = std::env::var_os("XDG_PICTURES_DIR")
            ...
            .unwrap_or_else(|| PathBuf::from("/tmp"));
        pictures
            .join("Screenshots")
            .join(format!("Screenshot_{timestamp_ms}.png"))
    } else {
        let runtime = std::env::var_os("XDG_RUNTIME_DIR")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/tmp"));
        runtime.join("rabbiz-de-shot.png")
    }
}
Impact: If `XDG_RUNTIME_DIR` (or `HOME`/`XDG_PICTURES_DIR`) is unset, `ctl_screenshot` writes PNG data to a predictable path `/tmp/rabbiz-de-shot.png` via `std::fs::write()` (no `O_EXCL`/`O_NOFOLLOW`, default umask `0644`), potentially exposing screen contents to other local users or failing on pre-created files. (Mitigated in normal production sessions because `CtlIpc::bind_with_tcp` disables IPC when `XDG_RUNTIME_DIR` is unset, though `--ctl-listen` / `--emu-listen` can still reach `ctl_screenshot`.)
Exploit/Trigger: Running `rabbiz-de` without `XDG_RUNTIME_DIR` (e.g., headless/virtual mode with `--emu-listen`) and triggering `Screenshot { save: false }`.
Recommendation: Return `Option<PathBuf>` from `screenshot_path` and fail closed (returning an IPC error) if neither `XDG_RUNTIME_DIR` nor `HOME` is set, matching `de-config/src/ipc.rs:26-32`. Create screenshot files with mode `0600` (`OpenOptionsExt::mode(0o600)`).
References: CWE-377 (Insecure Temporary File)
```

---

## Positive Observations

1. **Atomic `0600` Socket Creation in `CtlIpc`**: `tools/rabbiz-de/crates/de-core/src/runtime/ctl_ipc.rs:107-123` wraps `UnixListener::bind(&path)` with `libc::umask(0o177)` (restoring the previous umask immediately after) and follows with `set_permissions(0o600)`, avoiding a TOCTOU permission window on `$XDG_RUNTIME_DIR/rabbiz-de-ctl.sock`.
2. **Strict Loopback Enforcement for Dev/Emulator TCP Listeners**: `tools/rabbiz-de/crates/de-core/src/runtime/ctl_ipc.rs:134-137,298-301` explicitly rejects any non-loopback IP address for `--ctl-listen` and `--emu-listen`.
3. **Lock-Screen and Backend Gating in `CtlIpc`**: `tools/rabbiz-de/crates/de-core/src/runtime/ctl_ipc.rs:999-1041` (`check_ipc_permission`) blocks sensitive commands (`Launch`, `Screenshot`, `Inject*`, `SetMock`, `SetPanel { open: true }`) whenever Keyguard or `ext-session-lock-v1` is active, and gates input injection and mock commands on `BackendSelect::Virtual` + `cfg!(any(feature = "emu", test))`.
4. **Bounded IPC Framing & No `/tmp` Fallback for Control Socket**: `tools/rabbiz-de/crates/de-config/src/ipc.rs:19,26-32,220-251,276,308` enforces `MAX_MESSAGE_BYTES = 16 KiB` on both transport reads (`take`) and decoding, and refuses to fall back to `/tmp` when `XDG_RUNTIME_DIR` is unset.
5. **Fail-Closed Keyguard Session Fallback**: `rootfs-overlay/usr/local/bin/rabbiz-session:89-131` (`keyguard_requires_lock` + `to_labwc`) refuses to fall back to `labwc` if a non-swipe lock method is configured or if `prefs.json` cannot be parsed.
6. **Secret Zeroization & Argon2id**: `tools/rabbiz-de/crates/de-bin/src/auth_helper.rs:42,99,106` and `tools/rabbiz-de/crates/de-system/src/auth.rs:400-446,505-509,567` consistently use `zeroize` on secret buffers and Argon2id for PIN verification.

---

## Reviewed Files Machine List

```files
rootfs-overlay/etc/systemd/system/a21s-display-pin.service|reviewed|direct-read|Domain 6 systemd unit audit
rootfs-overlay/etc/systemd/system/a21s-drm-keeper.service|reviewed|direct-read|Domain 6 systemd unit audit
rootfs-overlay/etc/systemd/system/a21s-modem-init.service|reviewed|direct-read|Domain 6 systemd unit audit
rootfs-overlay/etc/systemd/system/abox-audio.service|reviewed|direct-read|Domain 6 systemd unit audit
rootfs-overlay/etc/systemd/system/fex-guestos.service|reviewed|direct-read|Domain 6 systemd unit audit
rootfs-overlay/etc/systemd/system/getty@tty1.service|skipped|not-applicable|Symlink to /dev/null (masked unit)
rootfs-overlay/etc/systemd/system/rabbiz-auth-helper.service|reviewed|direct-read|Domain 6 systemd unit audit
rootfs-overlay/etc/systemd/system/rabbiz-session.service|reviewed|direct-read|Domain 6 systemd unit audit
rootfs-overlay/etc/systemd/system/rabbiz-splash.service|reviewed|direct-read|Domain 6 systemd unit audit
rootfs-overlay/etc/systemd/system/s2mu106-charger.service|reviewed|direct-read|Domain 6 systemd unit audit
rootfs-overlay/etc/systemd/system/volume-keys.service|reviewed|direct-read|Domain 6 systemd unit audit
rootfs-overlay/usr/local/bin/a21s-drm-keeper|reviewed|direct-read|Domain 6 system script audit
rootfs-overlay/usr/local/bin/a21s-modem-ctl|reviewed|direct-read|Domain 6 system script audit
rootfs-overlay/usr/local/bin/a21s-power-key|reviewed|direct-read|Domain 6 system script audit
rootfs-overlay/usr/local/bin/a21s-qt-gpu|reviewed|direct-read|Domain 6 system script audit
rootfs-overlay/usr/local/bin/a21s-qt-render|reviewed|direct-read|Domain 6 system script audit
rootfs-overlay/usr/local/bin/a21s-telemetry|reviewed|direct-read|Domain 6 system script audit
rootfs-overlay/usr/local/bin/a21s-volume|reviewed|direct-read|Domain 6 system script audit
rootfs-overlay/usr/local/bin/a21s_holdmenu.py|reviewed|direct-read|Domain 6 system script audit
rootfs-overlay/usr/local/bin/rabbiz-appearance|reviewed|direct-read|Domain 6 system script audit
rootfs-overlay/usr/local/bin/rabbiz-labwc-session|reviewed|direct-read|Domain 6 system script audit
rootfs-overlay/usr/local/bin/rabbiz-osk-ctl|reviewed|direct-read|Domain 6 system script audit
rootfs-overlay/usr/local/bin/rabbiz-osk-switch|reviewed|direct-read|Domain 6 system script audit
rootfs-overlay/usr/local/bin/rabbiz-screenshot|reviewed|direct-read|Domain 6 system script audit
rootfs-overlay/usr/local/bin/rabbiz-session|reviewed|direct-read|Domain 6 system script audit
rootfs-overlay/usr/local/bin/rabbiz-shell|reviewed|direct-read|Domain 6 system script audit
rootfs-overlay/usr/local/bin/rabbiz-splash|reviewed|direct-read|Domain 6 system script audit
rootfs-overlay/usr/local/bin/rabbiz-spotify-ctl|reviewed|direct-read|Domain 6 system script audit
rootfs-overlay/usr/local/bin/rabbiz-wifi-connect|reviewed|direct-read|Domain 6 system script audit
rootfs-overlay/usr/local/bin/volume-keys.py|reviewed|direct-read|Domain 6 system script audit
tools/cbd/a21s-cbd.c|reviewed|direct-read|Domain 6 C modem boot daemon audit
tools/cbd/Makefile|reviewed|direct-read|Domain 6 C modem boot daemon build file
tools/rabbiz-de/crates/de-bin/src/auth_helper.rs|reviewed|direct-read|Domain 6 privileged auth helper audit
tools/rabbiz-de/crates/de-config/src/ipc.rs|reviewed|direct-read|Domain 6 IPC wire contract audit
tools/rabbiz-de/crates/de-core/src/runtime/ctl_ipc.rs|reviewed|direct-read|Domain 6 compositor control IPC audit
tools/rabbiz-de/crates/de-system/src/auth.rs|reviewed|targeted-read|Verified root_keyguard_paths and HelperRequest used by auth_helper.rs
```
