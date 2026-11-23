# Domain Audit: Security Policy (`SEC`)

## Scope & Files Reviewed

### Files Inspected

| File | Method | Notes |
|---|---|---|
| `rootfs-overlay/etc/polkit-1/rules.d/50-rabbiz-session.rules` | `direct-read` | Full file (15 lines) |
| `rootfs-overlay/etc/polkit-1/rules.d/51-rabbiz-shell.rules` | `direct-read` | Full file (15 lines) |
| `rootfs-overlay/etc/udev/rules.d/90-a21s-backlight.rules` | `direct-read` | Full file (2 lines) |
| `rootfs-overlay/etc/udev/rules.d/99-a21s-modem-fp.rules` | `direct-read` | Full file (3 lines) |
| `rootfs-overlay/etc/udev/rules.d/99-a21s-sensors-vib.rules` | `direct-read` | Full file (2 lines) |
| `rootfs-overlay/etc/udev/rules.d/99-hide-system-partitions.rules` | `direct-read` | Full file (12 lines) |
| `rootfs-overlay/etc/systemd/system/rabbiz-auth-helper.service` | `direct-read` | Full file (16 lines) |
| `rootfs-overlay/etc/systemd/system/rabbiz-session.service` | `direct-read` | Full file (63 lines) |
| `rootfs-overlay/etc/systemd/system/rabbiz-splash.service` | `direct-read` | Full file (17 lines) |
| `rootfs-overlay/etc/systemd/system/a21s-display-pin.service` | `direct-read` | Full file (18 lines) |
| `rootfs-overlay/etc/systemd/system/a21s-drm-keeper.service` | `direct-read` | Full file (16 lines) |
| `rootfs-overlay/etc/systemd/system/a21s-modem-init.service` | `direct-read` | Full file (12 lines) |
| `rootfs-overlay/etc/systemd/system/abox-audio.service` | `direct-read` | Full file (12 lines) |
| `rootfs-overlay/etc/systemd/system/fex-guestos.service` | `direct-read` | Full file (13 lines) |
| `rootfs-overlay/etc/systemd/system/s2mu106-charger.service` | `direct-read` | Full file (12 lines) |
| `rootfs-overlay/etc/systemd/system/volume-keys.service` | `direct-read` | Full file (11 lines) |
| `rootfs-overlay/etc/sysctl.d/10-a21s-quiet.conf` | `direct-read` | Full file (1 line) |
| `rootfs-overlay/etc/systemd/logind.conf.d/10-a21s-powerkey.conf` | `direct-read` | Full file (3 lines) |
| `rootfs-overlay/etc/xdg/autostart/rabbiz-keyring.desktop` | `direct-read` | Full file (9 lines) |
| `rootfs-overlay/usr/local/bin/a21s-power-key` | `direct-read` | Full file (67 lines) |
| `rootfs-overlay/usr/local/bin/a21s-modem-ctl` | `direct-read` | Full file (87 lines) |
| `rootfs-overlay/usr/local/bin/volume-keys.py` | `targeted-read` | Lines 1–120 (root evdev daemon & user runtime writes) |
| `scripts/xbox-ble-uinput.service` | `direct-read` | Full file (12 lines) |
| `tools/rabbiz-de/crates/de-config/src/privileged.rs` | `direct-read` | Full file (342 lines) |
| `tools/rabbiz-de/crates/de-core/src/protocols/privileged.rs` | `direct-read` | Full file (176 lines) |
| `tools/rabbiz-de/crates/de-core/src/protocols/session_lock.rs` | `direct-read` | Full file (229 lines) |
| `tools/rabbiz-de/crates/de-core/src/protocols/screencopy.rs` | `targeted-read` | Lines 45–315 (`can_view` and capture path) |
| `tools/rabbiz-de/crates/de-core/src/runtime/state.rs` | `targeted-read` | Lines 575–630, 800–830 (global filters & boot lock) |
| `tools/rabbiz-de/crates/de-core/src/runtime/power.rs` | `targeted-read` | Lines 315–495 (`fp_poll`, `power_cmd`) |
| `tools/rabbiz-de/crates/de-core/src/runtime/ctl_ipc.rs` | `targeted-read` | Lines 1–120, 560–690, 995–1060 (socket umask, mock fp, lock guard) |
| `tools/rabbiz-de/crates/de-core/src/shell_ui/lock_ui.rs` | `targeted-read` | Lines 1–800 (lockscreen, bouncer, quick actions) |
| `tools/rabbiz-de/crates/de-system/src/auth.rs` | `targeted-read` | Lines 1–800 (auth types, lockout policy, Argon2id, helper client) |
| `tools/rabbiz-de/crates/de-system/src/fingerprint.rs` | `direct-read` | Full file (62 lines) |
| `tools/rabbiz-de/crates/de-bin/src/auth_helper.rs` | `direct-read` | Full file (137 lines) |
| `socs/exynos850/drivers/fingerprint/a21s_et5xx.c` | `targeted-read` | Lines 290–335 (`trigger_unlock_store`) |
| `socs/exynos850/drivers/cpif/cpif_boot.c` | `targeted-read` | Lines 420–495 (`cpif_boot_ioctl` capability check verification) |
| `devices/samsung/a21s/kernel/a21s_linux_defconfig` | `rg-scan` | LSM/SELinux/AppArmor/seccomp config options |
| `docs/DE-PRIVILEGED-PROTOCOLS.md` | `direct-read` | Full file (127 lines) |
| `docs/DE-KEYGUARD-AUTH.md` | `direct-read` | Full file (113 lines) |

### Files / Areas NOT Inspected (and Why)

- On-device runtime files not tracked in git (e.g., `/etc/pam.d/su`, `/etc/pam.d/login`, `/etc/sudoers`, `/etc/group`, `/etc/rabbiz-de/privileged-clients.toml`, `/root/audio-up.sh`, `/root/charger-up.sh`, `steamos-session.service`): not present in this repository; assumptions about standard Arch Linux ARM `/etc/pam.d/su` (`pam_rootok.so`) are explicitly labeled as assumptions below.
- `tools/rabbiz-de/crates/de-core/src/runtime/input/` (full input routing while locked) and `tools/rabbiz-de/crates/de-core/src/runtime/frame/render.rs` (Keyguard render layer isolation): out of immediate security-policy scope (assigned to DE/Compositor domain); not claimed safe here.
- `scripts/volume-keys.service` and `scripts/volume-keys.py`: legacy/duplicate copies in `scripts/`; inspected the deployed `rootfs-overlay/` counterparts instead.
- `tools/rabbiz-apps/` and `shell/` QML files: inspected only via `rg` for `trigger_unlock` / `umts_` references; full review belongs to App/Shell domains.

---

## Findings

### SEC-001
```
ID: SEC-001
Title: Unauthenticated Keyguard Lockscreen Bypass via Quick Action Button
Severity: High
Confidence: Confirmed
Location: tools/rabbiz-de/crates/de-core/src/shell_ui/lock_ui.rs:716-723
Evidence:
```rust
            Some(de_ui::lock::LockHit::Quick) => {
                if let Some(id) = self.lock.quick_id.clone() {
                    self.lock.unlock_now();
                    self.nav.current = Screen::Home;
                    self.shell_dirty = true;
                    self.try_launch(&id, Some(de_api::Rect::new(size.w / 2, size.h - 200, 1, 1)));
                }
            }
```
Impact: Any person with physical access to a locked device can completely bypass PIN or Passphrase Keyguard authentication and gain full interactive access to the unlocked session (including `Screen::Home` and all apps/data).
Exploit/Trigger: Device is locked with `lock_method` set to `"pin"`, `"passphrase"`, or `"biometric_with_fallback"`, and any app matching `"camera"`, `"note"`, or `"editor"` exists in the app catalog (`lock_ui.rs:597-612`). Tapping the bottom-right quick-action button on the lock screen immediately invokes `self.lock.unlock_now()` without checking `requires_pin()`, `LockoutPolicy`, or `self.is_session_locked()`.
Recommendation: Never call `self.lock.unlock_now()` from `LockHit::Quick` when `self.lock.requires_pin(lock_method)` or `self.is_session_locked()` is true. Either open the PIN/Passphrase bouncer first (queuing the quick-action launch on successful authentication) or implement a restricted lockscreen-only surface in `DisplayArea::Keyguard` that cannot transition to `Screen::Home` without authentication.
References: CWE-287 (Improper Authentication), CWE-306 (Missing Authentication for Critical Function)
```

### SEC-002
```
ID: SEC-002
Title: Unauthenticated Keyguard Lockscreen Bypass via Fingerprint IRQ Polling, World-Writable `trigger_unlock` Sysfs, and `a21s-power.cmd`
Severity: High
Confidence: Confirmed
Location: tools/rabbiz-de/crates/de-core/src/runtime/power.rs:337-342, tools/rabbiz-de/crates/de-core/src/runtime/power.rs:472-475, rootfs-overlay/etc/udev/rules.d/99-a21s-modem-fp.rules:3, socs/exynos850/drivers/fingerprint/a21s_et5xx.c:307-318
Evidence:
`tools/rabbiz-de/crates/de-core/src/runtime/power.rs:337-342`:
```rust
        match self.power.fp_poll() {
            Some(true) if self.locked() || self.sleeping() => {
                de_system::fingerprint::arm_trigger();
                self.power_wake();
                self.lock.unlock_now();
            }
```
`tools/rabbiz-de/crates/de-core/src/runtime/power.rs:472-475`:
```rust
            "fp-unlock" => {
                self.power_wake();
                self.lock.unlock_now();
            }
```
`rootfs-overlay/etc/udev/rules.d/99-a21s-modem-fp.rules:3`:
```udev
ACTION=="add", SUBSYSTEM=="misc", KERNEL=="esfp0", RUN+="/bin/chmod 666 /sys/kernel/a21s_fp/trigger_unlock /sys/kernel/a21s_fp/sleep_pin"
```
`socs/exynos850/drivers/fingerprint/a21s_et5xx.c:307-318`:
```c
static ssize_t trigger_unlock_store(struct kobject *kobj,
				    struct kobj_attribute *attr,
				    const char *buf, size_t count)
{
	unsigned int val = 1;

	if (kstrtouint(buf, 0, &val) && buf[0] != '\n')
		return -EINVAL;
	if (val)
		et5xx_record_event(g_fp);
	return count;
}
```
(Also duplicated in `rootfs-overlay/usr/local/bin/a21s-modem-ctl:13` and `scripts/deploy_shell.sh:194`.)
Impact: Keyguard PIN/Passphrase authentication, the Before-First-Unlock (BFU) strong-auth requirement, and lockout cooldowns are completely bypassed both physically (any unregistered finger touching the ET520 sensor increments `irq_count` without matching) and locally (any local user/process can write `1` to world-writable `/sys/kernel/a21s_fp/trigger_unlock` or `fp-unlock:<nonce>` to `/run/user/1001/a21s-power.cmd` to immediately unlock the session).
Exploit/Trigger:
1. Physical: Touch the rear fingerprint sensor with any finger while the device is locked (even in `Pin` or `Passphrase` mode, BFU, or during cooldown).
2. Local: Any process on the system writes `1\n` to `/sys/kernel/a21s_fp/trigger_unlock` (mode `0666`), or any process as `rabbiz` writes `fp-unlock:<nonce>` to `/run/user/<uid>/a21s-power.cmd`.
Recommendation:
1. Gate fingerprint unlock in `power.rs` on `lock_method == "biometric_with_fallback"` AND `self.lock.lockout.can_use_biometric(now_s)` AND real cryptographic/template match verification (e.g., `fprintd`); never unlock on raw `irq_count` increments when strong auth (`Pin`/`Passphrase`) is configured.
2. Remove the `"fp-unlock"` command from the unauthenticated `a21s-power.cmd` file interface.
3. Remove `chmod 666` on `/sys/kernel/a21s_fp/trigger_unlock` and `sleep_pin` (keep root-only `0600` or remove `trigger_unlock` from non-debug builds).
References: CWE-287 (Improper Authentication), CWE-732 (Incorrect Permission Assignment for Critical Resource)
```

### SEC-003
```
ID: SEC-003
Title: `rabbiz-auth-helper` Root PAM Bypass (`/bin/su` as Root), Lockout Reset via `LockMethod::Swipe`, Missing `SO_PEERCRED`, and Unusable Socket Permissions
Severity: High
Confidence: Confirmed
Location: tools/rabbiz-de/crates/de-bin/src/auth_helper.rs:29-45, tools/rabbiz-de/crates/de-bin/src/auth_helper.rs:59-75, tools/rabbiz-de/crates/de-bin/src/auth_helper.rs:95-134, rootfs-overlay/etc/systemd/system/rabbiz-auth-helper.service:5-13, tools/rabbiz-de/crates/de-core/src/shell_ui/lock_ui.rs:82-98
Evidence:
`tools/rabbiz-de/crates/de-bin/src/auth_helper.rs:29-45`:
```rust
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
```
`tools/rabbiz-de/crates/de-bin/src/auth_helper.rs:59-75`:
```rust
    let ok = match req.method {
        LockMethod::Swipe => true,
        LockMethod::Pin | LockMethod::BiometricWithFallback => {
            ...
        }
        LockMethod::Passphrase => verify_pam_passphrase(&req.user, &req.secret),
    };
```
`rootfs-overlay/etc/systemd/system/rabbiz-auth-helper.service:5-13`:
```ini
[Service]
Type=simple
ExecStart=/usr/local/bin/rabbiz-auth-helper
RuntimeDirectory=rabbiz-auth
RuntimeDirectoryMode=0755
StateDirectory=rabbiz-auth
StateDirectoryMode=0700
Restart=on-failure
RestartSec=2
```
Impact:
1. **Passphrase authentication bypass (Assumption: standard Arch Linux `/etc/pam.d/su` includes `auth sufficient pam_rootok.so`)**: Because `rabbiz-auth-helper.service` runs as `root`, executing `/bin/su -c true <user>` succeeds immediately with exit code `0` without prompting for or verifying `passphrase` — accepting any passphrase including empty strings. In addition, passing `user` without `--` allows option injection to `/bin/su` if `user` begins with `-`.
2. **Lockout & BFU reset bypass**: `handle_request()` unconditionally accepts `LockMethod::Swipe => true` (`auth_helper.rs:60`), which causes `policy.record_attempt(true, now_s)` to reset `consecutive_failures = 0`, clear `cooldown_until_s`, and set `bfu_unlocked = true` in `/var/lib/rabbiz-auth/<user>/keyguard.state`.
3. **Missing peer identity check**: `handle_client()` (`auth_helper.rs:95-113`) never checks `SO_PEERCRED` (`uid`), allowing any client with socket access to authenticate or reset lockout state for arbitrary users (including `root`).
4. **Silent fallback to user-writable credential store**: Because `rabbiz-auth-helper.service` sets no `Group=` and `auth_helper.rs:133` sets mode `0o660` without `chown`, `/run/rabbiz-auth/helper.sock` is owned `root:root` (`0660`). `rabbiz-de` (running as `User=rabbiz`) receives `EACCES`, `verify_via_helper_socket()` returns `Unavailable`, and `lock_ui.rs:86-98` silently falls back to `~/.config/rabbiz-shell/keyguard.json` (writable by any app running as `rabbiz`).
Exploit/Trigger:
- Passphrase bypass: Any `LockMethod::Passphrase` request to `rabbiz-auth-helper` succeeds regardless of the supplied secret.
- Lockout reset: Any client able to connect to `/run/rabbiz-auth/helper.sock` sends `{"user":"<target>","method":"swipe","secret":""}`.
- Silent fallback: Occurs automatically under the shipped `rabbiz-auth-helper.service` unit configuration.
Recommendation:
1. Replace `/bin/su` with direct PAM authentication (`libpam` / `pam_authenticate` using a dedicated `/etc/pam.d/rabbiz-keyguard` service that does NOT include `pam_rootok.so`), or validate via `pam_unix` without `pam_rootok`. Never pass untrusted usernames to `su` without `--` and strict username validation (`[a-z_][a-z0-9_-]*`).
2. Reject `LockMethod::Swipe` in `rabbiz-auth-helper` (return `Unavailable` / error; never treat `Swipe` as an authenticated attempt that resets lockout/BFU).
3. Enforce `SO_PEERCRED` in `handle_client()`: resolve the peer UID via `getpwuid` and require `req.user` to match the connecting user (unless peer UID is `0`).
4. Configure socket group ownership (e.g., a dedicated `rabbiz-auth` group or systemd socket activation with `SocketGroup=rabbiz` / `SocketMode=0660`), and make `lock_ui.rs` fail closed rather than silently falling back to user-writable `~/.config/rabbiz-shell/keyguard.json` when the helper is expected.
References: CWE-287 (Improper Authentication), CWE-306 (Missing Authentication for Critical Function), CWE-88 (Argument Injection)
```

### SEC-004
```
ID: SEC-004
Title: Overly Permissive Udev Rules Expose Cellular Modem Boot/SMC Interface (`/dev/umts_*`) to `video` Group and World-Writable Hardware Controls (`0666`)
Severity: High
Confidence: Confirmed
Location: rootfs-overlay/etc/udev/rules.d/99-a21s-modem-fp.rules:1-3, rootfs-overlay/etc/udev/rules.d/99-a21s-sensors-vib.rules:2, rootfs-overlay/etc/udev/rules.d/90-a21s-backlight.rules:2, rootfs-overlay/etc/systemd/system/rabbiz-session.service:34, socs/exynos850/drivers/cpif/cpif_boot.c:420-494
Evidence:
`rootfs-overlay/etc/udev/rules.d/99-a21s-modem-fp.rules:1-3`:
```udev
KERNEL=="esfp0", MODE="0666"
KERNEL=="umts_*", GROUP="video", MODE="0660"
ACTION=="add", SUBSYSTEM=="misc", KERNEL=="esfp0", RUN+="/bin/chmod 666 /sys/kernel/a21s_fp/trigger_unlock /sys/kernel/a21s_fp/sleep_pin"
```
`rootfs-overlay/etc/udev/rules.d/99-a21s-sensors-vib.rules:2`:
```udev
ACTION=="add", SUBSYSTEM=="leds", KERNEL=="vibrator", RUN+="/bin/chmod 0666 /sys/class/leds/vibrator/brightness /sys/kernel/a21s_vib/duration_ms"
```
`rootfs-overlay/etc/udev/rules.d/90-a21s-backlight.rules:2`:
```udev
ACTION=="add", SUBSYSTEM=="cpu", KERNEL=="cpu[0-9]*", RUN+="/bin/chgrp video /sys/devices/system/cpu/%k/cpufreq/scaling_governor", RUN+="/bin/chmod g+w /sys/devices/system/cpu/%k/cpufreq/scaling_governor"
```
`rootfs-overlay/etc/systemd/system/rabbiz-session.service:34`:
```ini
SupplementaryGroups=seat input video render audio
```
`socs/exynos850/drivers/cpif/cpif_boot.c` (verified via `rg`): `cpif_boot_ioctl()` has zero `capable(CAP_SYS_ADMIN)` checks while exposing `CPIF_IOC_LOAD_CP_IMAGE` (`memcpy_toio` into CP RAM, line 425), `CPIF_IOC_REQ_SECURITY` (line 446), `CPIF_IOC_COMPLETE_NORMAL_BOOTUP` (line 450), and `CPIF_IOC_SET_CP_STATUS` (line 459).
Impact:
1. Any unprivileged process or third-party app running as `rabbiz` (member of `video`) can directly open `/dev/umts_boot0` to overwrite cellular modem RAM, invoke SMC security ioctls, or crash/reset the modem; and can directly open `/dev/umts_ipc0/1` and `/dev/umts_rfs0` to send raw cellular SIPC commands (calls, SMS, radio control) or tamper with modem NV/RFS storage.
2. Any local user/process (even outside `video`) can access `/dev/esfp0` (raw fingerprint chardev), reset/sleep the fingerprint sensor via `/sys/kernel/a21s_fp/sleep_pin`, trigger lockscreen unlock via `trigger_unlock` (see SEC-002), or abuse the vibrator (`0666`).
3. Any app in `video` can alter system-wide CPU frequency scaling governors (`scaling_governor`).
Exploit/Trigger: Any unprivileged process running as `rabbiz` (or any local UID for `0666` nodes) opens `/dev/umts_boot0`, `/dev/umts_ipc*`, `/dev/esfp0`, or writes to the sysfs files directly.
Recommendation:
1. Keep `/dev/umts_boot0` strictly `root:root` `0600` (only `a21s-modem-init.service` / `a21s-cbd` needs it) and add `if (!capable(CAP_SYS_ADMIN)) return -EPERM;` in `cpif_boot.c`.
2. Restrict `/dev/umts_ipc*` and `/dev/umts_rfs*` to a dedicated `modem` service group (`0660`) used only by the telephony daemon (not `video` / `rabbiz`).
3. Restrict `/dev/esfp0` and `/sys/kernel/a21s_fp/sleep_pin` to `root` or a dedicated `fprintd` service user (`0600`/`0660`), never `0666`. Also remove the matching `chmod 666` / `chgrp video` lines in `rootfs-overlay/usr/local/bin/a21s-modem-ctl:13-16` and `scripts/deploy_shell.sh:194-196`.
4. Replace `0666` on vibrator sysfs with a dedicated group (`0660`) and mediate `scaling_governor` / backlight changes through a privileged system service (or `power-profiles-daemon`) rather than granting `video` direct write access to `cpufreq`.
References: CWE-732 (Incorrect Permission Assignment for Critical Resource), CWE-276 (Incorrect Default Permissions)
```

### SEC-005
```
ID: SEC-005
Title: Overly Broad Polkit Rules Grant Unconditional `Result.YES` for All NetworkManager Actions and Session Unit Management
Severity: High
Confidence: Confirmed
Location: rootfs-overlay/etc/polkit-1/rules.d/51-rabbiz-shell.rules:9-15, rootfs-overlay/etc/polkit-1/rules.d/50-rabbiz-session.rules:4-15, rootfs-overlay/etc/systemd/system/rabbiz-session.service:19
Evidence:
`rootfs-overlay/etc/polkit-1/rules.d/51-rabbiz-shell.rules:9-15`:
```javascript
polkit.addRule(function(action, subject) {
    if (subject.user !== "rabbiz")
        return polkit.Result.NOT_HANDLED;
    if (action.id.indexOf("org.freedesktop.NetworkManager.") === 0)
        return polkit.Result.YES;
    return polkit.Result.NOT_HANDLED;
});
```
`rootfs-overlay/etc/polkit-1/rules.d/50-rabbiz-session.rules:4-15`:
```javascript
polkit.addRule(function(action, subject) {
    if (action.id === "org.freedesktop.systemd1.manage-units" &&
        subject.user === "rabbiz") {
        var unit = action.lookup("unit");
        if (unit === "steamos-session.service" ||
            unit === "rabbiz-session.service" ||
            unit === "a21s-phone-session.service") { // pre-rename unit alias
            return polkit.Result.YES;
        }
    }
    return polkit.Result.NOT_HANDLED;
});
```
`rootfs-overlay/etc/systemd/system/rabbiz-session.service:19`:
```ini
Conflicts=steamos-session.service
```
Impact:
1. **NetworkManager wildcard (`51-rabbiz-shell.rules`)**: Grants every process running as `rabbiz` (including un-sandboxed third-party apps and non-local/SSH sessions, since `subject.active` and `subject.local` are not checked) unconditional `Result.YES` for all `org.freedesktop.NetworkManager.*` actions — including `settings.modify.system` (exfiltrating all saved Wi-Fi PSKs and VPN credentials via D-Bus `GetSecrets`, modifying system-wide connections/routes), `settings.modify.global-dns` (hijacking DNS for all users and root services), `settings.modify.hostname`, and `wifi.share.open`.
2. **Session unit handover (`50-rabbiz-session.rules`)**: Does not check `subject.active`, `subject.local`, or `action.lookup("verb")`. Because `rabbiz-session.service` declares `Conflicts=steamos-session.service`, any background process or SSH session as `rabbiz` can run `systemctl start steamos-session.service` while the phone is locked, terminating `rabbiz-session.service` (and its Keyguard) and switching the seat to `steamos-session.service`.
Exploit/Trigger:
- Any process running as `rabbiz` invokes NetworkManager D-Bus methods (`GetSecrets`, `Update`, `SetGlobalDns`) or runs `systemctl start steamos-session.service` / `systemctl stop rabbiz-session.service`.
Recommendation:
1. In `51-rabbiz-shell.rules`: require `subject.local && subject.active` and replace the wildcard prefix with an explicit allow-list of only the minimum actions needed by the shell (e.g., `org.freedesktop.NetworkManager.network-control`, `org.freedesktop.NetworkManager.enable-disable-wifi`, `org.freedesktop.NetworkManager.settings.modify.own`), explicitly excluding `settings.modify.system`, `settings.modify.global-dns`, and `settings.modify.hostname`.
2. In `50-rabbiz-session.rules`: require `subject.local && subject.active`, restrict `action.lookup("verb")` to `"start"` (or `"restart"`), and gate session switching so it cannot be triggered while Keyguard is locked (e.g., via a Polkit auth prompt or a lock-aware session-switch helper).
References: CWE-269 (Improper Privilege Management), CWE-863 (Incorrect Authorization)
```

### SEC-006
```
ID: SEC-006
Title: Complete Absence of Systemd Service Sandboxing Across All System Units (9 of 10 Run as `root`)
Severity: Medium
Confidence: Confirmed
Location: rootfs-overlay/etc/systemd/system/rabbiz-auth-helper.service:5-13, rootfs-overlay/etc/systemd/system/volume-keys.service:5-8, rootfs-overlay/etc/systemd/system/a21s-drm-keeper.service:10-13, rootfs-overlay/etc/systemd/system/a21s-modem-init.service:6-9, rootfs-overlay/etc/systemd/system/a21s-display-pin.service:5-15, rootfs-overlay/etc/systemd/system/rabbiz-splash.service:7-14, rootfs-overlay/etc/systemd/system/abox-audio.service:5-9, rootfs-overlay/etc/systemd/system/s2mu106-charger.service:5-9, rootfs-overlay/etc/systemd/system/fex-guestos.service:6-10, scripts/xbox-ble-uinput.service:6-9
Evidence:
None of the 10 system units in `rootfs-overlay/etc/systemd/system/` (nor `scripts/xbox-ble-uinput.service`) contain any sandboxing directives (`NoNewPrivileges`, `ProtectSystem`, `ProtectHome`, `PrivateTmp`, `CapabilityBoundingSet`, `RestrictAddressFamilies`, `SystemCallFilter`, `LockPersonality`, `MemoryDenyWriteExecute`).
Example — `rootfs-overlay/etc/systemd/system/volume-keys.service:5-8`:
```ini
[Service]
ExecStart=/usr/local/bin/volume-keys.py
Restart=always
RestartSec=3
```
And `rootfs-overlay/usr/local/bin/volume-keys.py:30, 40-45` runs as `root` and writes directly to a file inside a directory owned by unprivileged user `rabbiz`:
```python
MARKER = "/run/user/%d/a21s-key-combo" % pwd.getpwnam("rabbiz").pw_uid
...
def mark_volume_press():
    try:
        with open(MARKER, "w") as f:
            f.write("1")
```
Additionally, `abox-audio.service:8` (`ExecStart=/root/audio-up.sh`) and `s2mu106-charger.service:8` (`ExecStart=/root/charger-up.sh`) invoke unmanaged scripts from `/root/`.
Impact: Any vulnerability or tampering in long-running daemons (`volume-keys.py`, `rabbiz-auth-helper`, `a21s-drm-keeper`, `fex-guestos`, `xbox-ble-uinput.py`) immediately grants unrestricted `root` access with full capabilities, full filesystem write access, and network access. Writing as `root` into user-owned `/run/user/<uid>/` (in `volume-keys.py` and `a21s-power-key`) creates a privilege-escalation surface if kernel symlink protection is disabled or misconfigured.
Exploit/Trigger: Compromise of any of the 9 root services, or race/symlink manipulation in `/run/user/<uid>/` if `fs.protected_symlinks` is not enforced.
Recommendation:
1. Add baseline systemd hardening to every service unit: `NoNewPrivileges=yes`, `ProtectSystem=strict`, `ProtectHome=yes`, `PrivateTmp=yes`, `PrivateDevices=yes` (where device nodes are not needed), `ProtectKernelTunables=yes`, `ProtectKernelModules=yes`, `ProtectControlGroups=yes`, `RestrictAddressFamilies=`, `CapabilityBoundingSet=`, and `SystemCallFilter=@system-service`.
2. Drop `volume-keys.service` from `root` to a dedicated low-privilege user in group `input` (or `rabbiz`), and move `a21s-key-combo` / `a21s-power.cmd` to a root-owned `/run/rabbiz-power/` directory (`0755`) if written by a privileged service.
3. Move `/root/audio-up.sh` and `/root/charger-up.sh` to `/usr/local/libexec/rabbiz/` with `0755` `root:root` permissions.
References: CWE-250 (Execution with Unnecessary Privileges), CIS Linux Benchmark / systemd.exec(5)
```

### SEC-007
```
ID: SEC-007
Title: Absence of Kernel MAC (SELinux/AppArmor), Kernel Seccomp Config, and Per-App Sandboxing
Severity: Medium
Confidence: Confirmed
Location: devices/samsung/a21s/kernel/a21s_linux_defconfig:8, devices/samsung/a21s/kernel/a21s_linux_defconfig:1140-1141, docs/DE-PRIVILEGED-PROTOCOLS.md:36-40
Evidence:
`rg` on `devices/samsung/a21s/kernel/a21s_linux_defconfig` for `CONFIG_SECURITY|CONFIG_SECCOMP|CONFIG_LSM|CONFIG_APPARMOR|CONFIG_SELINUX|CONFIG_BPF_LSM`:
```
8:CONFIG_BPF_LSM=y
1140:CONFIG_SECURITY=y
1141:CONFIG_SECURITY_PATH=y
```
`CONFIG_SECURITY_SELINUX`, `CONFIG_SECURITY_APPARMOR`, `CONFIG_SECCOMP`, `CONFIG_SECCOMP_FILTER`, and `CONFIG_SECURITY_LOCKDOWN_LSM` are not explicitly enabled in `a21s_linux_defconfig` (note: `CONFIG_SECCOMP` may or may not default to `y` depending on kernel Kconfig defaults — requires checking built `.config`).
`docs/DE-PRIVILEGED-PROTOCOLS.md:36-40`:
```markdown
**The hard limit.** Every app runs as the same Unix user (`rabbiz`) as the system helpers. Same-user code can
launch an allowed binary itself (`grim`, `wayvnc`, `wl-paste`) and use it as a proxy, can `ptrace` or preload
into one when the kernel permits it, and can connect to any socket path the user owns. Path allow-lists
therefore raise the bar against careless or naive abuse; they do **not** stop a determined same-user program.
```
Impact: There is no process-level isolation between third-party apps (e.g., browser, games, installed apps) and the shell/system helpers. Any compromised app running as `rabbiz` can read/modify all user data (including `~/.config/rabbiz-shell/keyguard.{json,state}`), connect to `$XDG_RUNTIME_DIR/rabbiz-de-ctl.sock`, `ptrace` or `LD_PRELOAD` allow-listed OSK binaries to bypass Wayland privileged protocol gating, and abuse Polkit/udev permissions granted to `rabbiz`.
Exploit/Trigger: Execution of any untrusted or compromised third-party application in the `rabbiz` session.
Recommendation:
1. Explicitly enable `CONFIG_SECCOMP=y`, `CONFIG_SECCOMP_FILTER=y`, `CONFIG_SECURITY_APPARMOR=y` (or `CONFIG_SECURITY_LANDLOCK=y` / `CONFIG_SECURITY_YAMA=y` with `kernel.yama.ptrace_scope=1`), and `CONFIG_SECURITY_LOCKDOWN_LSM=y` in `devices/samsung/a21s/kernel/a21s_linux_defconfig`.
2. Prioritize M5 per-app sandboxing (`bubblewrap` / Flatpak + `wp_security_context_v1` in `rabbiz-de`) so third-party apps run in isolated mount/PID/network namespaces with a restricted Wayland socket and cannot access `~/.config/rabbiz-shell/` or `rabbiz-de-ctl.sock`.
References: CWE-250 (Execution with Unnecessary Privileges), CWE-693 (Protection Mechanism Failure)
```

### SEC-008
```
ID: SEC-008
Title: `ext-session-lock-v1` Dead-Locker Fail-Closed Recovery and External `LockSurface` Rendering Are Unwired
Severity: Medium
Confidence: Confirmed
Location: tools/rabbiz-de/crates/de-core/src/protocols/session_lock.rs:81-96, tools/rabbiz-de/crates/de-core/src/protocols/session_lock.rs:107-111, tools/rabbiz-de/crates/de-core/src/protocols/session_lock.rs:139-149
Evidence:
`tools/rabbiz-de/crates/de-core/src/protocols/session_lock.rs:81-96`:
```rust
    #[allow(dead_code)] // Triggered by client disconnect hook once wired
    pub fn on_external_client_died(&mut self) {
        if self.owner == LockOwner::ExternalActive {
            self.owner = LockOwner::RecoveryRequired;
        }
    }

    #[allow(dead_code)] // Triggered by recovery bouncer once wired
    pub fn on_recovery_authenticated(&mut self) {
        if self.owner == LockOwner::RecoveryRequired {
            self.owner = LockOwner::Unlocked;
        }
    }
```
`rg` across `tools/rabbiz-de/crates/de-core/src/` confirmed zero non-test callers of `on_external_client_died()`, `on_recovery_authenticated()`, or `needs_recovery_bouncer()`.
Impact: If an allow-listed external `ext-session-lock-v1` client is used (via `/etc/rabbiz-de/privileged-clients.toml`) and crashes without calling `unlock_and_destroy`, `session_lock_tracker` remains stuck in `LockOwner::ExternalActive` forever instead of transitioning to `RecoveryRequired` and presenting the built-in recovery bouncer — stranding the user in a locked session with no recovery path until reboot/restart. Conversely, if a built-in unlock path (such as SEC-001/SEC-002) clears `self.lock`, `self.session_lock_tracker` and `self.lock` become desynchronized.
Exploit/Trigger: Configure an external `ext-session-lock-v1` client in `/etc/rabbiz-de/privileged-clients.toml`, lock the session, and terminate the client process (`SIGKILL`).
Recommendation: Wire `on_external_client_died()` into the Wayland client disconnect hook (tracking the owning `ClientId`), present the built-in PIN/Passphrase recovery bouncer when `needs_recovery_bouncer()` is true, and call `on_recovery_authenticated()` upon successful verification.
References: Wayland `ext-session-lock-v1` specification, `docs/DE-KEYGUARD-AUTH.md` §5
```

### SEC-009
```
ID: SEC-009
Title: Privileged Wayland Policy Path Overridable via `RABBIZ_DE_PRIVILEGED_POLICY` Environment Variable in Release Builds
Severity: Low
Confidence: Confirmed
Location: tools/rabbiz-de/crates/de-core/src/protocols/privileged.rs:34-38
Evidence:
```rust
/// Load the policy once at startup. `RABBIZ_DE_PRIVILEGED_POLICY` overrides the file path (tests, bring-up).
pub(crate) fn init_policy() {
    let path = std::env::var_os("RABBIZ_DE_PRIVILEGED_POLICY")
        .map_or_else(|| PathBuf::from(POLICY_FILE), PathBuf::from);
```
Impact: In release builds, any environment configuration influencing `rabbiz-de` startup (e.g., `~/.config/environment.d/` or session wrapper env) can redirect the privileged Wayland protocol allow-list away from root-owned `/etc/rabbiz-de/privileged-clients.toml` to a user-writable TOML file granting any binary `screencopy`, `data_control`, `virtual_pointer`, `virtual_keyboard`, `layer_shell`, and `session_lock`. Currently bounded by the same-user threat model, but weakens the root-owned policy boundary once session/app isolation is added.
Exploit/Trigger: Set `RABBIZ_DE_PRIVILEGED_POLICY=/path/to/custom.toml` in the environment of `rabbiz-de`.
Recommendation: Restrict `RABBIZ_DE_PRIVILEGED_POLICY` to `#[cfg(any(debug_assertions, test))]` or behind an explicit non-default Cargo feature so production builds always load `/etc/rabbiz-de/privileged-clients.toml`.
References: CWE-426 (Untrusted Search Path)
```

---

## Positive Observations

1. **Default-Deny, Least-Privilege Wayland Protocol Filtering** (`tools/rabbiz-de/crates/de-config/src/privileged.rs:108-181`, `tools/rabbiz-de/crates/de-core/src/protocols/privileged.rs:68-126`, `tools/rabbiz-de/crates/de-core/src/runtime/state.rs:580-613`): All six privileged Wayland globals (`LayerShell`, `VirtualKeyboard`, `DataControl`, `SessionLock`, `Screencopy`, `VirtualPointer`) are filtered via `new_with_filter` / `can_view` against the kernel-resolved `/proc/<pid>/exe` (never `argv[0]`). Built-in defaults leave `Screencopy`, `DataControl`, `VirtualPointer`, and `SessionLock` empty on the public socket, malformed policy files fail closed to built-ins, and `spawn_private_helper()` provides a private `WAYLAND_SOCKET` pair for compositor-spawned helpers.
2. **Well-Engineered Keyguard Crypto & Lockout State Machine** (`tools/rabbiz-de/crates/de-system/src/auth.rs:118-185, 218-278, 291-475, 505-509`): PIN hashing uses Argon2id (`m=19456 KiB, t=2, p=1`, random 16-byte `getrandom` salt) with constant-time comparison (`subtle::ConstantTimeEq`) and automatic migration from legacy SHA-256. Secret buffers are zeroized via `zeroize` (including `Drop` on `HelperRequest`). Lockout state uses atomic `0600` temp-file+`fsync`+rename persistence, defends against wall-clock rollback, and fails closed to a 300 s cooldown on corrupt/unreadable state.
3. **TOCTOU-Safe Control Socket Creation & Lock-State IPC Guard** (`tools/rabbiz-de/crates/de-core/src/runtime/ctl_ipc.rs:107-115, 999-1041`): `CtlIpc::bind_at_with_tcp()` sets `umask(0o177)` around `UnixListener::bind()` so `$XDG_RUNTIME_DIR/rabbiz-de-ctl.sock` is created `0600` atomically, refuses `/tmp` fallback when `XDG_RUNTIME_DIR` is unset, and rejects `Launch`, `Screenshot`, `SetPanel(open=true)`, and input-injection commands while locked.
4. **Honest Documentation of Threat Boundaries** (`docs/DE-PRIVILEGED-PROTOCOLS.md:36-47`, `docs/DE-KEYGUARD-AUTH.md:42-49`, `rootfs-overlay/etc/udev/rules.d/99-hide-system-partitions.rules:5`): Known same-user limitations, the `a21s-wayvnc` loopback bypass, and the cosmetic nature of `UDISKS_IGNORE` are clearly documented rather than misrepresented as security boundaries.

---

## Reviewed Files Machine List

```files
rootfs-overlay/etc/polkit-1/rules.d/50-rabbiz-session.rules|reviewed|direct-read|Polkit session handover rule audited (SEC-005)
rootfs-overlay/etc/polkit-1/rules.d/51-rabbiz-shell.rules|reviewed|direct-read|Polkit NetworkManager wildcard rule audited (SEC-005)
rootfs-overlay/etc/udev/rules.d/90-a21s-backlight.rules|reviewed|direct-read|Udev backlight and cpufreq governor rule audited (SEC-004)
rootfs-overlay/etc/udev/rules.d/99-a21s-modem-fp.rules|reviewed|direct-read|Udev modem and fingerprint permissions audited (SEC-002, SEC-004)
rootfs-overlay/etc/udev/rules.d/99-a21s-sensors-vib.rules|reviewed|direct-read|Udev vibrator 0666 permissions audited (SEC-004)
rootfs-overlay/etc/udev/rules.d/99-hide-system-partitions.rules|reviewed|direct-read|Udev UDISKS_IGNORE rule audited (positive)
rootfs-overlay/etc/systemd/system/rabbiz-auth-helper.service|reviewed|direct-read|Root auth helper unit audited (SEC-003, SEC-006)
rootfs-overlay/etc/systemd/system/rabbiz-session.service|reviewed|direct-read|Session unit groups and Conflicts audited (SEC-004, SEC-005)
rootfs-overlay/etc/systemd/system/rabbiz-splash.service|reviewed|direct-read|Root boot splash unit audited (SEC-006)
rootfs-overlay/etc/systemd/system/a21s-display-pin.service|reviewed|direct-read|Root display pin unit audited (SEC-006)
rootfs-overlay/etc/systemd/system/a21s-drm-keeper.service|reviewed|direct-read|Root DRM keeper unit audited (SEC-006)
rootfs-overlay/etc/systemd/system/a21s-modem-init.service|reviewed|direct-read|Root modem init unit audited (SEC-006)
rootfs-overlay/etc/systemd/system/abox-audio.service|reviewed|direct-read|Root audio unit audited (SEC-006)
rootfs-overlay/etc/systemd/system/fex-guestos.service|reviewed|direct-read|Root squashfuse unit audited (SEC-006)
rootfs-overlay/etc/systemd/system/s2mu106-charger.service|reviewed|direct-read|Root charger unit audited (SEC-006)
rootfs-overlay/etc/systemd/system/volume-keys.service|reviewed|direct-read|Root volume-keys daemon unit audited (SEC-006)
rootfs-overlay/etc/sysctl.d/10-a21s-quiet.conf|reviewed|direct-read|Sysctl quiet printk config audited
rootfs-overlay/etc/systemd/logind.conf.d/10-a21s-powerkey.conf|reviewed|direct-read|Logind powerkey config audited
rootfs-overlay/etc/xdg/autostart/rabbiz-keyring.desktop|reviewed|direct-read|Keyring autostart desktop entry audited
rootfs-overlay/usr/local/bin/a21s-power-key|reviewed|direct-read|Power key and fp-unlock script audited (SEC-002, SEC-006)
rootfs-overlay/usr/local/bin/a21s-modem-ctl|reviewed|direct-read|Modem/FP helper script audited (SEC-002, SEC-004)
rootfs-overlay/usr/local/bin/volume-keys.py|reviewed|targeted-read|Root evdev daemon writes to user runtime dir audited (SEC-006)
scripts/xbox-ble-uinput.service|reviewed|direct-read|Root BLE uinput service unit audited (SEC-006)
tools/rabbiz-de/crates/de-config/src/privileged.rs|reviewed|direct-read|Wayland privileged protocol policy audited (positive)
tools/rabbiz-de/crates/de-core/src/protocols/privileged.rs|reviewed|direct-read|Wayland privileged protocol enforcement audited (SEC-009, positive)
tools/rabbiz-de/crates/de-core/src/protocols/session_lock.rs|reviewed|direct-read|ext-session-lock-v1 handler audited (SEC-008)
tools/rabbiz-de/crates/de-core/src/protocols/screencopy.rs|reviewed|targeted-read|Screencopy can_view filter verified
tools/rabbiz-de/crates/de-core/src/runtime/state.rs|reviewed|targeted-read|Global filter registration and boot lock verified
tools/rabbiz-de/crates/de-core/src/runtime/power.rs|reviewed|targeted-read|Fingerprint polling and power_cmd unlock paths audited (SEC-002)
tools/rabbiz-de/crates/de-core/src/runtime/ctl_ipc.rs|reviewed|targeted-read|Control socket umask and lock guard audited (positive)
tools/rabbiz-de/crates/de-core/src/shell_ui/lock_ui.rs|reviewed|targeted-read|Lock screen bouncer and quick-action unlock audited (SEC-001, SEC-003)
tools/rabbiz-de/crates/de-system/src/auth.rs|reviewed|targeted-read|Keyguard crypto, lockout policy, and helper client audited (positive)
tools/rabbiz-de/crates/de-system/src/fingerprint.rs|reviewed|direct-read|ET520 sysfs interface audited (SEC-002)
tools/rabbiz-de/crates/de-bin/src/auth_helper.rs|reviewed|direct-read|Root auth helper daemon audited (SEC-003)
socs/exynos850/drivers/fingerprint/a21s_et5xx.c|reviewed|targeted-read|trigger_unlock_store sysfs handler audited (SEC-002)
socs/exynos850/drivers/cpif/cpif_boot.c|reviewed|targeted-read|cpif_boot_ioctl capability checks audited (SEC-004)
devices/samsung/a21s/kernel/a21s_linux_defconfig|reviewed|rg-scan|Kernel LSM/SELinux/AppArmor/seccomp config audited (SEC-007)
docs/DE-PRIVILEGED-PROTOCOLS.md|reviewed|direct-read|Privileged protocol design doc reviewed
docs/DE-KEYGUARD-AUTH.md|reviewed|direct-read|Keyguard authentication RFC reviewed
```
