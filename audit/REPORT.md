# RabbizOS Security & Quality Audit Report

- **Project:** RabbizOS (`silkyland/android-arm-linux` / `rabbiz-os-project`)
- **Audit Commit:** `c620c1d` (`main`, 2026-11-23) *(working tree had local uncommitted edits in `tools/rabbiz-apps/` and untracked `rabbiz-search/` & `rabbiz-mirror/linux/`, noted explicitly where audited)*
- **Mode:** Strictly **READ-ONLY** on source repository (outputs written exclusively under `audit/`); no builds, no external network calls, no root execution.
- **Audit Pipeline:** Phase 0 Recon ([00-recon.md](00-recon.md)) → Phase 1 Coverage Ledger ([ledger.csv](ledger.csv)) → Phase 2 10-Domain Parallel Audits (`findings/*.md`) → Phase 3 Tool-Assisted Scans (`tools/*.txt`) → Phase 4 Independent Challenge Pass ([challenge-log.md](challenge-log.md)) → Phase 5 Final Report (`REPORT.md`).

---

## 1. Executive Summary

RabbizOS is a bare-metal **Arch Linux ARM64 + Mainline Linux 7.3.0 + `uniLoader`** mobile operating system with a custom Rust Wayland compositor (`rabbiz-de`), pure-Rust Wayland base apps (`rabbiz-apps`), Rust OSK (`rabbiz-osk`), SDK/emulator (`rabbiz-sdk`), and companion mirror (`rabbiz-mirror`).

While recent hardening work (commit `2ec8c2c`) made strong progress on Argon2id keyguard hashing, loopback-only `CtlIpc`, and Wayland privileged protocol gating, this audit identified **79 deduplicated, Challenge-Pass-verified findings** (**0 Critical · 18 High · 32 Medium · 24 Low · 5 Info**).

**Key Risk Posture:**
- **Authentication / Lock Screen (`Keyguard`) is currently bypassable by 4 independent mechanisms** (a one-tap UI button, a non-verifying fingerprint driver with `0666` trigger, a root `/bin/su` PAM bypass in `rabbiz-auth-helper`, and live screen capture of the lock screen due to dead blanking code).
- **Local Privilege Escalation (`rabbiz` → `root`) is trivially achievable** via root-running `volume-keys.service` scripts following symlinks and calling `chown`/`chmod 666` inside user-owned directories, plus predictable `/tmp` staging paths in root deploy scripts and unguarded `/dev/umts_boot0` modem ioctls exposed to `group video`.
- **Platform & Supply-Chain Integrity** lacks downstream verification (`CONFIG_MODULE_SIG`, `CONFIG_DM_VERITY`, signed OTA, `.rbz` signatures) and includes a `curl -skL` TLS-disabled root package install (`mirror_setup.sh`) plus 4 unsigned `.ko.zst` binaries tracked in git.

### Top 5 Priorities to Fix Immediately

| Priority | Canonical ID(s) | Summary | Effort |
|---:|---|---|---|
| **1** | `SEC-001`, `SEC-002`, `SEC-003`, `CHAL-002` | **Close all 4 Keyguard lock-screen bypasses & capture leak:** remove unconditional `unlock_now()` on Quick Action (`lock_ui.rs:716-723`), disable/remove non-matching `a21s-et5xx` unlock trigger, replace `/bin/su` in `rabbiz-auth-helper` with `pam_authenticate` + `SO_PEERCRED` + `0660 root:rabbiz` socket, and fix `capture_is_blank()` to check `lock.is_locked()`. | Hours–1 day |
| **2** | `SVC-001`, `BM-002` | **Eliminate `rabbiz -> root` LPEs:** drop root privileges in `volume-keys.service` / never `chown`/`chmod` or open symlinks inside user-owned dirs (`O_NOFOLLOW`), and replace predictable `/tmp` paths in `de_deploy.sh`/`de_apps_deploy.sh`/`install_chrome_arm.sh` with `mktemp -d` under `/root`. | Hours |
| **3** | `CHAL-001` | **Fix `rabbiz-de` compositor heap buffer overflow:** replace unchecked `i32` multiplication and `unsafe { *out.get_unchecked_mut(d) }` in `composite.rs:458-494` with `checked_mul`/`usize` bounds checks and safe indexing; enable `overflow-checks = true` in release profiles (`BM-007`). | Hours |
| **4** | `BM-001`, `PRIV-001` | **Remove `curl -skL` root package install & unauthenticated `wayvnc`:** enforce TLS + GPG/SHA-256 in `scripts/mirror_setup.sh`, remove `--lan` / unauthenticated RFB Type 1 (`None`), and require SSH tunnel + VNC auth. | Hours |
| **5** | `SEC-004`, `SEC-005`, `APP-001`, `APP-004` | **Tighten hardware/Polkit/launcher/package boundaries:** restrict `/dev/umts_boot0` to `root:root 0600` + `CAP_SYS_ADMIN`, scope Polkit rules (`subject.active && subject.local`, no wildcard `NetworkManager.*`), reject raw command fallback & `..` in `de_apps::launch`, and enforce path validation + Ed25519 signatures on `.rbz` packages. | 1–3 days |

---

## 2. Scope & Method

- **Repository Baseline:** 1,350 git-tracked files across 13 tracked top-level locations (`tools/`, `socs/`, `devices/`, `rootfs-overlay/`, `shell/`, `docs/`, `assets/`, `scripts/`, `findings/`, `.github/`, `tests/`, `patches/`, and 6 root files), plus 4 gitignored local directories (`src/`, `build/`, `backups/`, `drivers/`).
- **Methodology:**
  1. **Phase 0 (Recon):** Mapped actual architecture (Arch Linux ARM64 + Linux 7.3.0 + `uniLoader`, 5 Rust workspaces, SwiftPM, Kbuild, QML) and adapted all 10 audit domains.
  2. **Phase 1 (Coverage Ledger):** Indexed every top-level directory and tracked file in [audit/ledger.csv](ledger.csv) (1,239 rows; media/font assets grouped by directory).
  3. **Phase 2 (10-Domain Audit):** Ran 10 independent domain sub-agents in parallel writing evidence-backed reports to `audit/findings/<domain>.md`.
  4. **Phase 3 (Tool-Assisted):** Executed `shellcheck`, custom Python KSPP kernel defconfig checker, regex secret scanner, `sha256sum`/`readelf`/`strings` on `.ko.zst` blobs, `Cargo.lock`/SPDX scanner, and code-pattern scanner (`audit/tools/*.txt`). All raw outputs were manually triaged against source files before reporting.
  5. **Phase 4 (Independent Challenge Pass):** Fresh-context Reviewer Agent challenged all **75 Round 1 Medium+ findings** (43 Confirmed, 8 Downgraded, 24 Merged Duplicates, 1 Partial FP caught on `SEC-007` `CONFIG_SECCOMP`, 1 Positive Observation refuted on `PRIV` #3) and spot-checked **25 randomly sampled `reviewed` files (12.5%)**, uncovering **3 new findings (`CHAL-001`, `CHAL-002`, `CHAL-003`)**.

---

## 3. Coverage

Derived directly from [audit/ledger.csv](ledger.csv) after Phase 2 + Phase 4 spot-check updates (0 entries left `pending`):

### 3.1 Coverage by Domain (Non-Directory Ledger Entries)

| Domain | Total Entries | `reviewed` | `% reviewed` | `partial` (Tool/rg Scan) | `skipped` | `n/a` |
|---|---:|---:|---:|---:|---:|---:|
| `build-manifest` | 59 | 28 | 47.5% | 25 | 6 | 0 |
| `framework-apps` | 478 | 66 | 13.8% | 411 | 1 | 0 |
| `kernel` | 401 | 48 | 12.0% | 329 | 24 | 0 |
| `partition-ota` | 5 | 4 | 80.0% | 0 | 1 | 0 |
| `quality-maintainability` | 152 | 25 | 16.4% | 116 | 11 | 0 |
| `secrets-privacy` | 2 | 2 | 100.0% | 0 | 0 | 0 |
| `security-policy` | 15 | 13 | 86.7% | 2 | 0 | 0 |
| `supply-chain` | 36 | 3 | 8.3% | 0 | 33 | 0 |
| `system-services` | 68 | 39 | 57.4% | 29 | 0 | 0 |
| **Total (Non-Dir Entries)** | **1,216** | **228** | **18.8%** | **912 (75.0%)** | **76 (6.2%)** | **0** |

*Note: All 17 top-level directories have explicit statuses in `audit/ledger.csv` (11 `partial`, 5 `skipped` [gitignored/assets], 1 `n/a` [empty `.worktrees`]). Every security-critical boundary (compositor auth/IPC/privileged protocols, all systemd units, all polkit/udev rules, all rootfs scripts, kernel defconfig + all 16 patches, all deploy/boot scripts, and `.rbz` packaging) was read directly line-by-line (`reviewed`).*

### 3.2 Skipped / Uninspected Items (with Reasons)

| Path / Group | Count | Reason |
|---|---:|---|
| `src/` (gitignored) | ~72,753 files (2.8G) | Upstream `samsungexynos850/linux` & `uniLoader` clones; out of tracked repo scope |
| `build/` (gitignored) | ~30,937 files (3.9G) | Generated build artifacts (only `build/linux/.config` was cross-referenced to verify Kconfig defaults) |
| `backups/` (gitignored) | 7 files (7.7G) | Local raw device partition backups; excluded from source audit |
| `drivers/` (gitignored) | 385 files (21M) | Legacy untracked local directory superseded by `socs/exynos850/drivers/` |
| `assets/` & media/font groups | 33 ledger groups (~170 files) | Non-executable `.png`, `.svg`, `.icns`, `.ttf`, `.otf` branding/font assets |
| `socs/exynos850/drivers/vendor-include/*` | 24 files | Vendor SoC register header definitions; audited via driver call sites |
| Miscellaneous non-security docs/templates | 19 files | Non-executable markdown/template/config files outside direct audit budget |

---

## 4. Findings (Post-Challenge Pass, Deduplicated)

**Summary Counts:** **0 Critical · 18 High · 32 Medium · 24 Low · 5 Info** (Total: **79**).

---

### 4.1 Critical Findings (0)

*No findings met the Critical rubric (remote unauthenticated system compromise, live remote boot/OTA bypass, or primary signing key leak). Note that `BOOT-001` & `OTA-005` are platform/architectural gaps requiring physical/local access (High), and the `APP-005` + `APP-001` remote RCE chain is currently mitigated on `HEAD` by `rabbiz-de`'s loopback-only check in `flags.rs:175` & `ctl_ipc.rs:134`.*

---

### 4.2 High Findings (18)

```
ID: SEC-001
Title: Unauthenticated Keyguard Lockscreen Bypass via Bottom-Right Quick Action Button
Severity: High
Confidence: Confirmed
Location: tools/rabbiz-de/crates/de-core/src/shell_ui/lock_ui.rs:266-274, 716-723
Evidence:
  lock_ui.rs:716-723:
    LockHit::Quick => {
        self.lock.unlock_now();
        self.mark_dirty();
        return true;
    }
  lock_ui.rs:266-274: unlock_now() sets self.locked = false and Screen::Home with zero auth check.
Impact: Any physically present user can bypass PIN or Passphrase Keyguard in a single tap.
Exploit/Trigger: Tap the bottom-right Quick Action affordance on the lock screen.
Recommendation: Remove unlock_now() from LockHit::Quick; either open the auth bouncer or restrict Quick Action to a locked-safe camera/emergency surface.
References: CWE-287, CWE-306, audit/findings/security-policy.md
```

```
ID: SEC-002 (merges KERN-003)
Title: Complete Keyguard Bypass via Non-Matching Fingerprint Driver & World-Writable Unlock Trigger
Severity: High
Confidence: Confirmed
Location: socs/exynos850/drivers/fingerprint/a21s_et5xx.c:87-119,188-204,307-318; rootfs-overlay/etc/udev/rules.d/99-a21s-modem-fp.rules:1,3; tools/rabbiz-de/crates/de-core/src/runtime/power.rs:337-342,472-475
Evidence:
  a21s_et5xx.c:188-204: any DRDY IRQ edge records an "unlock" event with zero biometric matching.
  99-a21s-modem-fp.rules:1,3: MODE="0666" on /dev/esfp0 and /sys/kernel/a21s_fp/trigger_unlock.
  power.rs:337-342,472-475: polling /dev/esfp0 or reading "fp-unlock" in a21s-power.cmd calls self.lock.unlock_now().
Impact: Any finger touching the sensor or any unprivileged local process writing to /dev/esfp0 or trigger_unlock immediately unlocks the device without authentication.
Exploit/Trigger: Touch the fingerprint sensor with any finger, or execute `echo 1 > /sys/kernel/a21s_fp/trigger_unlock` as any local user.
Recommendation: Disable a21s-et5xx autoloading and remove fp-unlock / unlock_now() wiring until real biometric template verification (TEE/fprintd) is implemented; remove 0666 udev rules and trigger_unlock sysfs.
References: CWE-287, CWE-732, audit/findings/security-policy.md, audit/findings/kernel.md
```

```
ID: SEC-003 (merges SVC-002)
Title: rabbiz-auth-helper Root PAM Bypass (/bin/su as Root), Lockout Reset, Missing SO_PEERCRED & Socket Fallback
Severity: High
Confidence: Confirmed
Location: tools/rabbiz-de/crates/de-bin/src/auth_helper.rs:29-45,59-75,95-134; tools/rabbiz-de/crates/de-system/src/auth.rs:483-495; tools/rabbiz-de/crates/de-core/src/shell_ui/lock_ui.rs:82-98; rootfs-overlay/etc/systemd/system/rabbiz-auth-helper.service:1-16
Evidence:
  auth_helper.rs:64-75: runs `/bin/su -c true <user>` as UID 0 (PAM pam_rootok.so approves unconditionally without verifying password) and omits `--` before <user>.
  auth_helper.rs:59-63: LockMethod::Swipe => true resets lockout/BFU state on client request; no SO_PEERCRED check.
  rabbiz-auth-helper.service omits Group=rabbiz; socket is root:root 0660 so user rabbiz cannot connect and lock_ui.rs:82-98 silently falls back to user-writable ~/.config/rabbiz-shell/keyguard.json.
Impact: Passphrase authentication always succeeds regardless of input; any socket client can reset lockout/BFU; in practice the root-owned helper is unreachable by rabbiz, defeating root-isolated keyguard storage.
Exploit/Trigger: Submit any passphrase to rabbiz-auth-helper, or connect as rabbiz (falls back to user-writable keyguard.json).
Recommendation: Replace `/bin/su` with direct PAM `pam_authenticate` (without `pam_rootok`), enforce `SO_PEERCRED` UID/GID check, disallow client-supplied `Swipe` method override, and set `Group=rabbiz` (or `SocketGroup=rabbiz` via systemd socket activation).
References: CWE-287, CWE-288, CWE-732, audit/findings/security-policy.md, audit/findings/system-services.md
```

```
ID: SVC-001
Title: Local Privilege Escalation (rabbiz -> root) via Symlink Following and chown/chmod in volume-keys.service
Severity: High
Confidence: Confirmed
Location: rootfs-overlay/etc/systemd/system/volume-keys.service:6; rootfs-overlay/usr/local/bin/volume-keys.py:30,42; rootfs-overlay/usr/local/bin/a21s-volume:107-122; rootfs-overlay/usr/local/bin/a21s-power-key:35-42; rootfs-overlay/usr/local/bin/rabbiz-screenshot:41-43,60,71-76
Evidence:
  volume-keys.service runs as root and writes/chowns/chmods paths inside non-sticky rabbiz-owned /run/user/<uid> (a21s-key-combo, a21s-power.cmd, a21s-screenshot.state) and ~/Pictures/Screenshots without O_NOFOLLOW or privilege drop.
Impact: Direct local privilege escalation from unprivileged user `rabbiz` (or any compromised app) to `root`.
Exploit/Trigger: Place a symlink at /run/user/1001/a21s-power.cmd or a21s-screenshot.state pointing to /etc/shadow or a root-owned file, then trigger a power/volume key press (`fs.protected_symlinks` does not protect non-sticky user-owned directories).
Recommendation: Run volume-keys/screenshot helpers as unprivileged user `rabbiz` (with `input` group read-only access), or drop privileges (`setresuid`/`setresgid`) and use `O_NOFOLLOW` before touching any user-owned path.
References: CWE-59, CWE-732, audit/findings/system-services.md
```

```
ID: CHAL-001 (Discovered in Phase 4 Spot-Check)
Title: Heap Buffer Overflow in rabbiz-de Compositor via i32 Integer Overflow in read_shm()
Severity: High
Confidence: Confirmed
Location: tools/rabbiz-de/crates/de-core/src/runtime/composite.rs:458-494
Evidence:
  composite.rs:459-461: `let mut out = vec![0u8; (w * h * 4) as usize];` (i32 multiplication wraps in release builds where overflow-checks=false).
  composite.rs:467-492: `let d = (y * w + x) as usize * 4;` followed by `unsafe { *out.get_unchecked_mut(d) = ...; }` with no check that `d + 3 < out.len()`.
Impact: Any unprivileged Wayland client (including sandboxed/untrusted apps) can trigger heap memory corruption and potential compositor RCE or crash via a crafted `wl_shm` buffer (sparse `memfd`).
Exploit/Trigger: Attach a `wl_shm` buffer with crafted `width`/`height` causing `w * h * 4` to overflow `i32` while `w` and `h` remain positive.
Recommendation: Compute all buffer sizes and offsets in `usize` with `checked_mul`/`checked_add`, cap maximum surface dimensions, remove `get_unchecked_mut` in favor of checked slices, and set `overflow-checks = true` in `[profile.release]`.
References: CWE-190, CWE-787, audit/challenge-log.md
```

```
ID: CHAL-002 (Discovered in Phase 4 Spot-Check)
Title: Lock-Screen Capture Blanking Is Dead Code — Live Lock Screen & PIN Bouncer Exposed to Screencopy/wayvnc
Severity: High
Confidence: Confirmed
Location: tools/rabbiz-de/crates/de-core/src/runtime/composite.rs:348-360; tools/rabbiz-de/crates/de-core/src/runtime/power.rs:508-511
Evidence:
  composite.rs:351: `if capture_is_blank(de.nav.current)` checks `Screen::Lock`, but `power_lock()`/`lock.open()` never call `nav.lock()` (0 non-test callers in the entire repo). `de.nav.current` stays `Screen::Home` or `Screen::App` while locked.
Impact: While the device is locked, `wlr-screencopy` (including `wayvnc`) receives live frames of the lock screen, PIN bouncer keystrokes, and desktop instead of blank frames. (Refutes Round 1 PRIV Positive Observation #3.)
Exploit/Trigger: Lock the device and capture frames via `wlr-screencopy` / `wayvnc`.
Recommendation: Check `de.lock.is_locked() || de.external_session_lock_active()` directly in `capture_output()` instead of relying on `de.nav.current`.
References: CWE-200, CWE-561, audit/challenge-log.md
```

```
ID: BM-001 (merges OTA-001, SC-001)
Title: TLS Verification Disabled (curl -skL) for Unverified Root Package Installation in mirror_setup.sh
Severity: High
Confidence: Confirmed
Location: scripts/mirror_setup.sh:47-56
Evidence:
  mirror_setup.sh:47-56: downloads aml, neatvnc, and wayvnc `.pkg.tar.xz` archives using `curl -skL` into predictable `/tmp/` paths and installs them as root via `pacman -U --noconfirm` with no GPG or SHA-256 verification.
Impact: Any MITM on Wi-Fi/LAN or local user racing `/tmp` achieves arbitrary root code execution on the device during mirror setup.
Exploit/Trigger: Run `scripts/mirror_setup.sh` on an untrusted network or with a hostile local process on the device.
Recommendation: Remove `-k` (enforce TLS), verify pinned SHA-256 checksums or Arch package signatures, and stage files in a `mktemp -d` directory under `/root`.
References: CWE-295, CWE-494, audit/findings/build-manifest.md
```

```
ID: BM-002 (merges OTA-004)
Title: Predictable /tmp & /var/tmp Staging Paths in Root Deployment Scripts Enable Local Privilege Escalation
Severity: High
Confidence: Confirmed
Location: scripts/de_deploy.sh:34-70; scripts/de_apps_deploy.sh:116-130; scripts/install_chrome_arm.sh:22-26; scripts/setup_dumb_destroy_fix.sh:6-9; scripts/mirror_setup.sh:65
Evidence:
  Root deploy scripts stage binaries and scripts in predictable `/tmp/rabbiz-*.new`, `/tmp/rabbiz-apps-bin.$$`, `/tmp/chrome-extract`, `/var/tmp/...` and `mirror_setup.sh:65` writes as root to `/home/rabbiz/.config/systemd/user/a21s-wayvnc.service`.
Impact: Compromised `rabbiz` user can win a symlink/TOCTOU race during deployment to overwrite root-owned binaries or systemd units and gain root.
Exploit/Trigger: Pre-create symlinks or directories in `/tmp` matching the staging paths before a developer runs deploy scripts.
Recommendation: Use `mktemp -d /root/.deploy.XXXXXX` (mode `0700`) for all root staging and drop privileges (`runuser -u rabbiz`) when writing under `/home/rabbiz`.
References: CWE-377, CWE-59, audit/findings/build-manifest.md, audit/findings/partition-ota.md
```

```
ID: SEC-004 (merges SVC-004)
Title: Overly Permissive Udev Rules & Modem Helper Expose Cellular Modem Boot/SMC Interface to Unprivileged Apps
Severity: High
Confidence: Confirmed
Location: rootfs-overlay/etc/udev/rules.d/99-a21s-modem-fp.rules:1-3; rootfs-overlay/usr/local/bin/a21s-modem-ctl:12-17; rootfs-overlay/etc/udev/rules.d/99-a21s-sensors-vib.rules:2; rootfs-overlay/etc/udev/rules.d/90-a21s-backlight.rules:2; socs/exynos850/drivers/cpif/cpif_boot.c
Evidence:
  99-a21s-modem-fp.rules & a21s-modem-ctl grant GROUP="video" MODE="0660" on /dev/umts_* (including /dev/umts_boot0, whose cpif_boot_ioctl lacks capable(CAP_SYS_ADMIN) checks on CPIF_IOC_LOAD_CP_IMAGE/SMC) while rabbiz-session runs in group video; also 0666 on /dev/esfp0 and vibrator sysfs.
Impact: Any unprivileged app running as user `rabbiz` can load arbitrary modem firmware, invoke modem SMC security ioctls, or intercept raw cellular IPC/SMS/calls.
Exploit/Trigger: Open `/dev/umts_boot0` or `/dev/umts_ipc0` from any process running as `rabbiz`.
Recommendation: Keep `/dev/umts_boot0` `root:root 0600` and add `capable(CAP_SYS_ADMIN)` in `cpif_boot_ioctl`; isolate modem IPC behind a dedicated `modem` service/group rather than `video`.
References: CWE-732, CWE-269, audit/findings/security-policy.md, audit/findings/system-services.md
```

```
ID: SEC-005
Title: Overly Broad Polkit Rules Grant Unconditional Result.YES for Wildcard NetworkManager & Session Switching
Severity: High
Confidence: Confirmed
Location: rootfs-overlay/etc/polkit-1/rules.d/51-rabbiz-shell.rules:9-15; rootfs-overlay/etc/polkit-1/rules.d/50-rabbiz-session.rules:4-15
Evidence:
  51-rabbiz-shell.rules:9-15 grants unconditional `polkit.Result.YES` for `action.id.indexOf("org.freedesktop.NetworkManager.") === 0` without `subject.active && subject.local`.
  50-rabbiz-session.rules:4-15 unconditionally allows starting `steamos-session.service` (which `Conflicts=` with locked `rabbiz-session.service`).
Impact: Any process as `rabbiz` (including remote/background processes) can extract all saved Wi-Fi/VPN secrets (`GetSecrets`), hijack global DNS, or kill a locked `rabbiz-session.service` by starting `steamos-session.service`.
Exploit/Trigger: Invoke NetworkManager D-Bus `GetSecrets` or `systemd1.StartUnit("steamos-session.service")` as `rabbiz`.
Recommendation: Require `subject.active && subject.local`, replace wildcard `NetworkManager.*` with explicit allowlisted actions (excluding `modify.system` / `global-dns` / `GetSecrets`), and gate session switching behind active unlocked session state.
References: CWE-269, CWE-285, audit/findings/security-policy.md
```

```
ID: PRIV-001
Title: Unauthenticated wayvnc Service Exposed on USB/Loopback (and Optional LAN 0.0.0.0) Bypasses Compositor Policy
Severity: High
Confidence: Confirmed
Location: scripts/mirror_setup.sh:14,21,75,80; tools/rabbiz-de/crates/de-config/src/privileged.rs:219
Evidence:
  mirror_setup.sh installs persistent `a21s-wayvnc.service` binding `172.16.42.1:5900` + `127.0.0.1:5900` (or `0.0.0.0:5900` with `--lan`) with RFB Security Type 1 (None / no password, no TLS). `/usr/bin/wayvnc` is allowed privileged capture/virtual-pointer access.
Impact: Any local unprivileged app, USB host, or LAN peer (with `--lan`) can view the entire screen (including lock screen via `CHAL-002`) and inject arbitrary touch/keyboard input.
Exploit/Trigger: Connect any VNC client to port 5900 with no credentials.
Recommendation: Bind `wayvnc` strictly to `127.0.0.1` (or Unix socket) with authentication enabled, remove `--lan`, and access exclusively over authenticated SSH port forwarding.
References: CWE-306, CWE-319, audit/findings/secrets-privacy.md
```

```
ID: APP-001
Title: Arbitrary Command Execution & Path Traversal via Unvalidated desktop_id in de_apps::launch
Severity: High
Confidence: Confirmed
Location: tools/rabbiz-de/crates/de-apps/src/launch.rs:36-52, 112-120, 151-161
Evidence:
  launch.rs:40-51: if `desktop_id` is not found in the catalog, it falls back verbatim to `split_shellish(desktop_id)` and spawns `setsid` (without `--`).
  launch.rs:112-120,151-161: `desktop_id` is joined raw into `.desktop` and `.log` paths without rejecting `/` or `..`.
Impact: Any IPC or launcher caller supplying a crafted `desktop_id` can execute arbitrary commands with flags or write logs outside the intended directory.
Exploit/Trigger: Send an uncatalogued command string or `../../...` path as `desktop_id` to the launcher.
Recommendation: Validate `desktop_id` against a strict `[A-Za-z0-9._-]+` allowlist (rejecting `/` and `..`), pass `--` to `setsid`, and remove the raw command fallback.
References: CWE-78, CWE-22, audit/findings/framework-apps.md
```

```
ID: APP-004 (merges OTA-002, OTA-003)
Title: Missing Cryptographic Signatures, Path Traversal (app.id / app.exec), Arbitrary chmod 0755 & .desktop Injection in rabbiz-pack
Severity: High
Confidence: Confirmed
Location: tools/rabbiz-sdk/crates/rabbiz-pack/src/lib.rs:158-208, 358-398, 459-482
Evidence:
  rabbiz-pack verifies only a self-contained checksum (no cryptographic signature) with a double-read TOCTOU window; `app.id = ".."` passes `validate()`; absolute or `..` `app.exec` replaces `install_dir` in `Path::join` and is `chmod 0755`; unescaped newlines in manifest fields allow `.desktop` INI injection (`Exec=`).
Impact: Installing an untrusted `.rbz` bundle can overwrite arbitrary user files, mark arbitrary files executable, and inject arbitrary commands into `.desktop` launchers.
Exploit/Trigger: Install a crafted `.rbz` archive via `rabbiz-pack` / `rabbiz-cli`.
Recommendation: Add Ed25519 signature verification over the bundle digest, read once into memory/temp FD (eliminate TOCTOU), reject `..`/`/`/absolute paths in `app.id` and `app.exec`, and sanitize/reject control chars and newlines before writing `.desktop` files.
References: CWE-347, CWE-22, CWE-74, audit/findings/framework-apps.md, audit/findings/partition-ota.md
```

```
ID: BOOT-001
Title: Verified Boot Disabled (Unlocked S-Boot, Unsigned AVB Algorithm: NONE) and No Downstream Integrity
Severity: High
Confidence: Confirmed
Location: devices/samsung/a21s/docs/a21s-flash-boot-checklist.md:79-80; devices/samsung/a21s/kernel/a21s_linux_defconfig:71-75; build/linux/.config:951,2648
Evidence:
  Boot image uses `Algorithm: NONE` (`OEM LOCK: OFF`), and the kernel disables `CONFIG_DM_VERITY` and `CONFIG_MODULE_SIG` / `CONFIG_MODULE_SIG_FORCE`.
Impact: An attacker with physical access or local root can persistently tamper with the kernel, initramfs, kernel modules, or rootfs undetected.
Exploit/Trigger: Flash a modified `boot.img` via Odin/Heimdall or modify rootfs/modules on storage.
Recommendation: While Samsung S-Boot e-fuse lock prevents stock AVB on A21s, enforce downstream integrity where feasible: enable `CONFIG_MODULE_SIG_FORCE`, `CONFIG_DM_VERITY` for read-only `/usr` or system image, and plan proper AVB/custom-key Verified Boot for Phase 2 (`muyu`).
References: CWE-345, CWE-494, audit/findings/boot-chain.md
```

```
ID: BOOT-002
Title: Passwordless Auto-Login Graphical Session on Boot with Keyguard Bypass when MODE != de
Severity: High
Confidence: Confirmed
Location: rootfs-overlay/etc/systemd/system/rabbiz-session.service:31-47; rootfs-overlay/usr/local/bin/rabbiz-session:58-82,89-131
Evidence:
  rabbiz-session.service boots directly into an authenticated `rabbiz` graphical session without login; `rabbiz-session:80` checks `if [ "$MODE" != "de" ]` before `keyguard_requires_lock()`, which itself reads user-writable `~/.config/rabbiz-shell/prefs.json` and `~/.config/rabbiz-de/mode`.
Impact: Switching session mode or modifying user-writable prefs bypasses the keyguard requirement on boot, and BFU (Before First Unlock) encryption/isolation is absent.
Exploit/Trigger: Set `~/.config/rabbiz-de/mode` to non-`de` or edit `prefs.json` and restart session.
Recommendation: Determine lock state exclusively from root-owned `/var/lib/rabbiz-auth` (via `rabbiz-auth-helper`), refuse non-`de` fallback whenever keyguard is enabled, and adopt a display manager / greeter or root-enforced lock gate on boot.
References: CWE-287, CWE-288, audit/findings/boot-chain.md
```

```
ID: OTA-005
Title: Architectural Gap — No Signed OTA System, No A/B Partitioning on A21s, and No Rollback Protection
Severity: High
Confidence: Confirmed
Location: tools/rabbiz-apps/crates/rabbiz-store/src/backend.rs; devices/samsung/a21s/docs/a21s-flash-boot-checklist.md:79-80
Evidence:
  A21s uses a single `BOOT` partition (`/dev/mmcblk0p18`) updated manually via `dd`/SSH; `rabbiz-updater` referenced in `rabbiz-store` does not exist; rollback index is hardcoded to 12 in unsigned AVB footer.
Impact: No atomic rollback on failed updates, no cryptographic authenticity for OS updates, and vulnerable to version downgrade attacks.
Exploit/Trigger: Any OS update failure risks bricking; any MITM or compromised mirror during manual updates can deliver tampered or downgraded system images.
Recommendation: Design and implement a signed atomic update mechanism (e.g., RAUC / systemd-sysupdate / OSTree with Ed25519/TUF metadata and A/B slots on `muyu`).
References: CWE-494, CWE-757, audit/findings/partition-ota.md
```

```
ID: KERN-001 (merges SC-003, QUAL-003)
Title: Kernel Module Signature Verification & Lockdown Disabled; 4 Unsigned Pre-Compiled .ko.zst Blobs Tracked in Git
Severity: High
Confidence: Confirmed
Location: build/linux/.config:951,8857; socs/exynos850/drivers/scsc/scsc_{bt,mx,platform_mif,wlan}.ko.zst
Evidence:
  `CONFIG_MODULE_SIG` and `CONFIG_SECURITY_LOCKDOWN_LSM` are unset in `build/linux/.config`. Four opaque prebuilt aarch64 `.ko.zst` modules (`audit/tools/binary-blobs.txt`: vermagic `7.3.0`, zero `sig_` signature fields) are committed directly in git, contradicting `docs/LICENSING.md:12`.
Impact: Root-to-kernel compromise is trivial via arbitrary unsigned module loading; tracked prebuilt `.ko.zst` binaries undermine supply-chain verifiability and reproducibility.
Exploit/Trigger: `insmod` any unsigned kernel module as root, or unknowingly deploy stale/tampered `.ko.zst` blobs from git.
Recommendation: Remove `*.ko.zst` from git (add to `.gitignore`), build from source in CI, and enable `CONFIG_MODULE_SIG`, `CONFIG_MODULE_SIG_FORCE`, and `CONFIG_SECURITY_LOCKDOWN_LSM`.
References: CWE-345, CWE-494, audit/findings/kernel.md, audit/findings/supply-chain.md
```

```
ID: KERN-002 (merges SEC-007)
Title: Zero Mandatory Access Control (MAC) / Hardening LSMs Active and No Per-App Sandboxing
Severity: High
Confidence: Confirmed
Location: build/linux/.config:2648,8851-8859,8867
Evidence:
  Resolved `build/linux/.config` has `CONFIG_SECURITY_SELINUX`, `CONFIG_SECURITY_APPARMOR`, `CONFIG_SECURITY_YAMA`, `CONFIG_SECURITY_LANDLOCK`, `CONFIG_SECURITY_LOCKDOWN_LSM`, and `CONFIG_LOADPIN` all unset. All userland apps run unsandboxed as the same UID `rabbiz`. *(Challenge Pass verified `CONFIG_SECCOMP=y` & `CONFIG_SECCOMP_FILTER=y` ARE enabled at lines 848-849.)*
Impact: Compromise of any single app (e.g., browser, PDF viewer, third-party `.rbz` app) grants full read/write access to all user data, credentials, SSH keys, and `rabbiz` D-Bus/Polkit/udev privileges.
Exploit/Trigger: Any code execution inside a single userland app.
Recommendation: Enable `CONFIG_SECURITY_LANDLOCK=y` and `CONFIG_SECURITY_YAMA=y` (minimum quick win), then introduce per-app bubblewrap/Flatpak/Landlock + seccomp sandboxing and AppArmor/SELinux profiles.
References: CWE-250, CWE-693, audit/findings/kernel.md, audit/challenge-log.md
```

---

### 4.3 Medium Findings (32)

All 32 deduplicated Medium findings are Confirmed against source files (see `audit/findings/*.md` and `audit/challenge-log.md` for full code excerpts):

| ID | Title | Location | Impact / Trigger | Recommendation |
|---|---|---|---|---|
| `APP-002` *(merges OTA-006)* | Shell command injection via `state.json` & unreviewed `paru --noconfirm` AUR builds | `tools/rabbiz-apps/crates/rabbiz-store/src/main.rs:790-842`, `backend.rs:118-120,227-241` | Crafted `state.json` or hostile AUR PKGBUILD executes arbitrary commands as `rabbiz` | Replace `sh -c` with direct `Command::new` arg vectors; validate package IDs; require PKGBUILD review or disable AUR auto-build |
| `APP-003` | SSH option injection (missing `--`) & TOFU `accept-new` in macOS/Linux `RabbizMirror` | `tools/rabbiz-mirror/Sources/RabbizMirror/DeviceMirrorService.swift:12-21`, `linux/src/device_service.rs:17-34` | Pasting `-oProxyCommand=...` executes local commands; first-connect Wi-Fi MITM accepted silently | Validate host regex, add `--` before target, enforce host key verification/pinning |
| `APP-006` | Cleartext unauthenticated RFB Type 1 & unbounded OOM allocations in `RFBClient.swift` / `rabbiz-emu-vnc` | `tools/rabbiz-mirror/Sources/RabbizMirror/RFBClient.swift:294-311,406-494`, `rabbiz-emu-vnc.rs:50-54,130-132` | Malicious VNC server triggers OOM crash; cleartext screen/input traffic | Cap `nameLength`/`ServerCutText`/`DesktopSize`, support VNC auth/TLS, fix framing sync |
| `APP-007` | WASM sandbox unbounded recursion, locals OOM & negative `i32` wraparound | `tools/rabbiz-sdk/crates/rabbiz-wasm-runner/src/lib.rs:308-314,423,505-514,559-584` | Untrusted WASM module crashes host process via stack overflow or OOM | Enforce call-depth limit, locals cap, and `u32`/`checked_add` linear memory bounds |
| `APP-008` *(merges PRIV-002)* | Plaintext Wi-Fi PSK passed in `nmcli` argv (`/proc/<pid>/cmdline`) & missing `--` | `tools/rabbiz-apps/crates/rabbiz-settings/src/system.rs:373-384,415` | Any local process can read Wi-Fi PSK from `/proc`; crafted SSID injects `nmcli` flags | Pass PSK via stdin/D-Bus (`NetworkManager.AddAndActivateConnection`) and add `--` |
| `APP-009` | Unquoted `sh -c` shell interpolation in still-shipped legacy QML `shell/Rabbiz/Services/` | `shell/Rabbiz/Services/SystemControl.qml:114-115,180-187`, `Brightness.qml:74-81` | Command injection if legacy QML shell is active (`deploy_shell.sh:122-125`) | Retire/stop deploying legacy QML shell or pass argv arrays without `sh -c` |
| `APP-010` | Unvalidated scraped URLs & Chrome `--password-store=basic` in untracked `rabbiz-search` | `tools/rabbiz-apps/crates/rabbiz-search/src/apps.rs:133-156` | `file://`/flag injection to `xdg-open`/Chrome; plaintext browser credential storage | Enforce `https://` allowlist + `--`; remove `--password-store=basic` |
| `BM-003` *(merges BOOT-004, SC-005)* | `bootstrap_build_env.sh` silently ignores `uniLoader` fetch/checkout failures (`\|\| true`) + short SHAs | `scripts/bootstrap_build_env.sh:6,8,44-46` | Fresh bootstrap can silently build unpinned upstream `HEAD` | Remove `\|\| true`, pin full 40-char commit SHAs, verify `git rev-parse HEAD` |
| `BM-004` *(merges SC-006)* | GitHub Actions uses mutable tags (`@v4`, `@stable`) & lacks `permissions:` block | `.github/workflows/host-checks.yml:1-42` | Compromised action tag runs with default token permissions in CI | Pin Actions to full commit SHAs and add `permissions: { contents: read }` |
| `BM-005` *(merges OTA-007, SC-002, SC-008)* | Unverified remote Chrome `.deb` & PanVK `.so` downloaded and extracted as root | `scripts/install_chrome_arm.sh:17-26`, `scripts/setup_panvk_test.sh:30-40` | Upstream/CDN compromise executes root code; `setup_panvk_test.sh` omits `curl -f` | Pin version + SHA-256 checksums, add `curl -f`, verify GPG signatures |
| `BM-006` | Missing C hardening flags (`-fstack-protector-strong`, `FORTIFY`, PIE, RELRO) for root `a21s-cbd` | `tools/cbd/Makefile:7-12` | Memory corruption in root modem daemon is directly exploitable | Add `-fstack-protector-strong -D_FORTIFY_SOURCE=2 -fPIE -pie -Wl,-z,relro,-z,now` |
| `BM-007` | 5/6 Rust workspaces omit `[profile.release]`; all 6 leave `overflow-checks = false` | `tools/rabbiz-{apps,osk,sdk,camera,de}/Cargo.toml` | Release integer wraps remain silent (directly enabling `CHAL-001`) | Add `[profile.release]` with `overflow-checks = true`, `lto`, `strip` |
| `BM-008` *(merges SC-009)* | Floating Docker tags (`rust:1.99-bookworm`), unpinned `apt-get`, CI `@stable` drift, missing `--locked` | `.github/workflows/host-checks.yml:39,56,58`, `tools/rabbiz-de/Dockerfile`, `Dockerfile.emu` | Non-reproducible builds and silent dependency/toolchain drift | Pin Docker digests, pass `--locked` to `cargo`, align CI toolchain with `rust-toolchain.toml` |
| `KERN-004` | World-writable `/proc/driver/mxman_ctrl0/mx_panic` (`0666`) | `socs/exynos850/drivers/scsc/misc/mxproc.c:179-180,419-422` | Any unprivileged local user can force-panic Wi-Fi/BT subsystem (DoS) | Change mode from `0666` (`S_IWOTH`) to `0600` (`root` only) |
| `KERN-005` | Display Controller (DPU) IOMMU (`&sysmmu_dpu`) disabled in DTS & bypassed in DRM | `devices/samsung/a21s/kernel/patches/0003-a21s-abox-audio-dts.patch:18-20`, `0004-exynos-drm-dma-iommu-optional.patch:10` | DPU DMA lacks IOMMU isolation against rogue/misconfigured display DMA | Investigate & restore `&sysmmu_dpu` DMA-IOMMU isolation |
| `KERN-006` | Core kernel memory-corruption mitigations disabled in resolved `.config` | `build/linux/.config` (`FORTIFY_SOURCE`, `HARDENED_USERCOPY`, `SLAB_FREELIST_*`, `INIT_ON_ALLOC/FREE` unset) | Significantly lowers exploitation bar for kernel driver memory bugs | Enable `CONFIG_FORTIFY_SOURCE`, `CONFIG_HARDENED_USERCOPY`, `CONFIG_SLAB_FREELIST_RANDOM/HARDENED`, `CONFIG_INIT_ON_ALLOC_DEFAULT_ON` |
| `KERN-007` *(merges BOOT-003)* | Excessive kernel debug surface (`DEBUG_FS`, `KEXEC`, `DEVMEM`, `MAGIC_SYSRQ`, `KALLSYMS_ALL`, `DMESG_RESTRICT` off) | `devices/samsung/a21s/kernel/a21s_linux_defconfig:17-18,34,36-37,70`, `build/linux/.config:317-318,3687,8840,9348-9352` | Leaks kernel pointers/symbols (weakening KASLR) and eases root-to-kernel persistence | Disable `KEXEC`, `DEVMEM` (or enable `IO_STRICT_DEVMEM`), `KALLSYMS_ALL`, `MAGIC_SYSRQ`; enable `SECURITY_DMESG_RESTRICT` & hardening `sysctl.d` |
| `KERN-008` | Unbounded MMIO OOB read (`vgpio`) & unrestricted CP RAM linear-map poke (`linw`) in autoloaded `a21s-cpif` | `socs/exynos850/drivers/cpif/cpif_power.c:232-236,256-259,285-300`, `cpif_core.c:220-283` | Root debug sysfs `peek` can read arbitrary kernel MMIO or corrupt CP RAM | Gate debug sysfs behind `CONFIG_DEBUG_FS` / compile-time flag, bounds-check `off` and alignment |
| `KERN-009` | `bootstrap_build_env.sh` blindly globs `*.patch`, applying two "DO NOT FLASH" boot-hanging patches (`0011`, `0012`) | `scripts/bootstrap_build_env.sh:32-40`, `devices/samsung/a21s/kernel/patches/0011-*.patch`, `0012-*.patch` | Fresh kernel build produces a TZPC boot-hanging image | Move experimental/broken patches out of `patches/` or use an explicit `series` file |
| `OTA-008` | `backup_muyu_stock.sh` wrong `REPO_ROOT` writes unignored serials & EFS/persist dumps inside git repo | `devices/xiaomi/muyu/scripts/backup_muyu_stock.sh:9-12`, `.gitignore:7` | Sensitive device EFS/serial backups can be accidentally committed to git | Fix `REPO_ROOT` (`../../..`) to resolve to actual repo root `/backups/` |
| `OTA-009` | `deploy_shell.sh` runs `tar -C / -xf -` without `--no-same-owner` or `device_session.sh` lock | `scripts/deploy_shell.sh:159-160,182-187` | Transiently reassigns `/` and `/etc` ownership to host UID (potential `rabbiz` collision) | Pass `--no-same-owner --no-overwrite-dir` and wrap with `device_session.sh` |
| `PRIV-003` | `public_history.sh` re-fetches unredacted `$SRC` objects (line 96), omits `/home/<user>/` & `--replace-message`, exits 0 without `gitleaks` | `scripts/release/public_history.sh:67-82,96,120,129-134` | Public history export can leak unredacted secrets/paths silently | Remove post-filter `git fetch "$SRC"`, add `--replace-message` & home-path redaction, fail closed if `gitleaks` is missing |
| `PRIV-004` *(merges APP-005)* | Docker/QEMU emulator configs publish ports `5910`/`5911`/`2222` on `0.0.0.0` & unauthenticated `rabbiz-emu-vnc` | `tools/rabbiz-sdk/crates/rabbiz-emu-viewer/src/main.rs:166,179-182`, `rabbiz-cli/src/lib.rs:583-587`, `Dockerfile.emu:22`, `rabbiz-emu-vnc.rs:19-23` | Exposes dev VM SSH `:2222` on host LAN (`0.0.0.0`) and conflicts with `rabbiz-de` loopback enforcement | Bind all Docker `-p 127.0.0.1:...` and QEMU `hostfwd=tcp:127.0.0.1:...` to loopback only |
| `PRIV-005` | Real Samsung & Xiaomi device serials, home Wi-Fi SSID, BT MAC, LAN IP & dev host path committed in docs/logs | `devices/samsung/a21s/docs/a21s-access-decision.md:4-5`, `a21s-wifi-bluetooth.md:139,149`, `ROADMAP.md:51`, `docs/PLAN.md:8` | Leaks hardware identifiers and developer environment details (violates `AGENTS.md`/`CONTRIBUTING.md`) | Redact all identifiers in tracked docs and scrub before public release |
| `QUAL-001` *(merges SC-004)* | `tools/rabbiz-de/Cargo.toml` patches `smithay` to gitignored `vendor/smithay` | `tools/rabbiz-de/Cargo.toml:88-89`, `.gitignore:6` | Clean clones and GitHub Actions `rabbiz-de` CI job cannot compile | Point `smithay` to a pinned git commit URL or track the minimal patch/vendor cleanly |
| `SC-007` | Samsung "Confidential" header (`cme3_prim.h`) citing missing `LICENSE.txt`; 345/365 driver files lack SPDX; GPL-2.0 tools under Apache-2.0 root | `socs/exynos850/drivers/scsc/include/scsc/api/cme3_prim.h:1-8`, `LICENSE` | Legal/license compliance ambiguity for public redistribution | Audit & clarify Samsung GPLv2 vs proprietary header provenance; add `LICENSES/` + SPDX tags |
| `SEC-006` *(merges SVC-006)* | Complete absence of systemd sandboxing across all 9 root units + `/root/*.sh` paths + `/tmp/rabbiz-osk.log` | `rootfs-overlay/etc/systemd/system/*.service`, `rootfs-overlay/usr/local/bin/rabbiz-osk-switch:12,45` | Compromise of any daemon yields unrestricted root access; `/tmp` log symlink risk | Add `ProtectSystem=strict`, `NoNewPrivileges=yes`, `PrivateTmp=yes`, `CapabilityBoundingSet=`; move scripts out of `/root/` |
| `SEC-008` | `ext-session-lock-v1` dead-locker fail-closed recovery is `#[allow(dead_code)]` (unwired) | `tools/rabbiz-de/crates/de-core/src/protocols/session_lock.rs:81-96` | If an external locker crashes, session is permanently stranded without recovery bouncer | Wire `on_external_client_died()` and `on_recovery_authenticated()` into Wayland client disconnect & `lock_ui` |
| `SVC-003` | OS command injection via Wi-Fi profile name/SSID in `rabbiz-wifi-connect` (`script -qec`) | `rootfs-overlay/usr/local/bin/rabbiz-wifi-connect:22-26,31-44` | Crafted SSID with `"`/`$(...)` executes commands as `rabbiz` when reconnecting via legacy QML shell | Replace `script -qec` string interpolation with `nmcli` stdin/D-Bus API and strict argument separation |
| `SVC-005` | `rabbiz-auth-helper` single-threaded blocking accept loop, unbounded `read_line` (OOM) & `bind`→`chmod` race | `tools/rabbiz-de/crates/de-bin/src/auth_helper.rs:95-113,125-136` | Any local client opening a connection and stalling permanently DoSs Keyguard unlock | Set socket read/write timeouts, cap line length (`take(4096)`), spawn bounded worker/async handler, set umask before `bind()` |
| `SVC-007` | `CtlIpc` 64 idle client slots without timeout (IPC DoS) & unbounded `EmuFrameServer` clients | `tools/rabbiz-de/crates/de-core/src/runtime/ctl_ipc.rs:67-70,161-166,322-340` | Local process opening 64 idle connections permanently blocks `rabbiz-de ctl` | Add idle read timeout / eviction for `CtlClient` and cap `EmuFrameServer` connections |
| `SVC-008` | `angelfish` browser launched with `--no-sandbox` & `mpv` missing `--` in `rabbiz-spotify-ctl` | `rootfs-overlay/usr/local/bin/rabbiz-spotify-ctl:277-289,366-376` | Disables Chromium renderer sandbox on web content; crafted URL injects `mpv` flags | Remove `--no-sandbox` (fix userns/SUID sandbox) and pass `-- "${url}"` to `mpv` |

---

### 4.4 Low Findings (24)

| ID | Title | Location | Recommendation |
|---|---|---|---|
| `CHAL-003` | Unchecked `width * height * 4` & unasserted `px.data.len()` before `ptr::copy_nonoverlapping`, missing `catch_unwind` across `extern "C"`, `Box::leak` in `rabbiz-sdk-c` | `tools/rabbiz-sdk/crates/rabbiz-sdk-c/src/lib.rs:512-513,530-559` | Use `checked_mul`, assert buffer length, wrap FFI entrypoints in `catch_unwind` |
| `APP-011` | Predictable `/tmp` paths in `rabbiz-cli` MCP tools & unvalidated AVD name path traversal | `tools/rabbiz-sdk/crates/rabbiz-cli/src/lib.rs:522-525,742,764,770` | Use `tempfile::NamedTempFile` and validate AVD names (`[A-Za-z0-9._-]+`) |
| `APP-012` | Unbounded pipe buffering before `OUTPUT_CAP` truncation in `rabbiz-terminal` headless runner | `tools/rabbiz-apps/crates/rabbiz-terminal/src/headless.rs:39,70-78,139-153` | Cap bytes read during streaming via `Read::take()` |
| `BM-009` | Unescaped `$OUT` Python interpolation & unvalidated env vars in host/device scripts | `scripts/verify_mirror.sh:25`, `scripts/device_session.sh:7,12`, `scripts/de_deploy.sh:113` | Pass `$OUT` via `sys.argv[1]` instead of string interpolation |
| `BM-010` | Dead `case` pattern (`SC2221`/`SC2222`) & silent `codesign \|\| true` without Hardened Runtime | `scripts/de_apps_deploy.sh:108`, `scripts/build_mac_app.sh:61,63` | Fix `case` branch and fail on `codesign` errors (`--options runtime`) |
| `BM-011` | CI workflow omits 5 Rust workspaces and `tests/test_rabbiz_splash.py` | `.github/workflows/host-checks.yml:13-58` | Add CI jobs for `rabbiz-apps`, `rabbiz-osk`, `rabbiz-sdk`, `rabbiz-camera`, and Python tests |
| `BOOT-005` | Hardcoded UID `1001` in `rabbiz-session.service` (`XDG_RUNTIME_DIR=/run/user/1001`) | `rootfs-overlay/etc/systemd/system/rabbiz-session.service:26-27,38-39` | Resolve UID dynamically via `id -u rabbiz` or `systemd-logind` PAM session |
| `BOOT-006` | `rabbiz-splash.service` runs early as root unsandboxed + unvalidated BMP width bounds | `rootfs-overlay/etc/systemd/system/rabbiz-splash.service:7-14`, `rabbiz-splash:218-235` | Add systemd sandboxing and validate BMP header dimensions against framebuffer |
| `BOOT-007` | Tier 3 VM builder binds `RABBIZ_EMU_LISTEN`/`RABBIZ_CTL_LISTEN` to `0.0.0.0` + `rw` root | `scripts/build_emu_image.sh:193-194,267,273,275` | Bind to `127.0.0.1` and mount root `ro` initially |
| `KERN-010` | Stack OOB / unterminated stack buffers in vendored `scsc_bt`, `scsc_wlan`, `abox` drivers | `socs/exynos850/drivers/scsc/bt/scsc_bt_module.c:2152-2205`, `wlan/udi.c:723-735`, `abox/aud3004x-dump.c:434-442` | Replace `strcpy`/`sprintf` with bounded `strscpy`/`snprintf` and explicit NUL termination |
| `KERN-011` | Bloated generic server/desktop kernel surface compiled into mobile phone kernel | `devices/samsung/a21s/kernel/a21s_linux_defconfig:41,60,68-69,166,441,984-993` | Strip unused server filesystems, PCI/legacy subsystems, and unused net protocols |
| `OTA-010` | `99-hide-system-partitions.rules` hides partitions in UI only (`UDISKS_IGNORE`), no UFS (`muyu`) coverage | `rootfs-overlay/etc/udev/rules.d/99-hide-system-partitions.rules:1-12` | Set `ATTR{ro}="1"` on critical partitions (`EFS`/`BOOTLOADER`) and add UFS rules for `muyu` |
| `PRIV-006` | `rabbiz-wifi-connect` `sleep 2` PSK echo race & uncleaned `_connPsk` in `Network.qml` | `rootfs-overlay/usr/local/bin/rabbiz-wifi-connect:22-26,41`, `shell/Rabbiz/Services/Network.qml` | Use D-Bus NM API instead of `sleep 2` PTY echo; clear `_connPsk` immediately |
| `QUAL-002` | CI blind spots omitting 480+ Rust tests & `shellcheck` *(downgraded from Medium)* | `.github/workflows/host-checks.yml` | Integrate `shellcheck` and all Rust workspace tests into CI |
| `QUAL-004` | Uncommitted working tree sprawl (60+ modified/untracked files across lanes) *(downgraded from Medium)* | Working tree (`tools/rabbiz-apps/`, `tools/rabbiz-mirror/linux/`, etc.) | Split changes into clean lane-scoped branches/commits |
| `QUAL-005` | Shellcheck defects (`SC2221`, `SC3067`, `SC2155`, `SC2034`) *(downgraded from Medium)* | `scripts/de_apps_deploy.sh:108`, `rabbiz-osk-ctl:13`, `rabbiz-labwc-session:50`, `scripts/audio-up.sh:22` | Fix shellcheck errors and enforce `shellcheck` in `scripts/check.sh` |
| `QUAL-006` | Legacy 79-file `shell/` tree & one-off diagnostic probes in `tools/` | `shell/`, `tools/{a21s-et520-id-probe,browser-bench,cma-leak,qt-scanout,shade-preview}` | Archive or remove deprecated QML shell and one-off probes |
| `QUAL-007` | Byte-for-byte duplicate `volume-keys.py` & `volume-keys.service` in `scripts/` vs `rootfs-overlay/` | `scripts/volume-keys.*` vs `rootfs-overlay/` | Keep single canonical copy in `rootfs-overlay/` |
| `QUAL-008` | Documentation drift/contradictions (`README.md`, `ROADMAP.md`, `DE-IPC.md` vs `DE-KEYGUARD-AUTH.md`) | `docs/DE-IPC.md:24`, `docs/DE-KEYGUARD-AUTH.md:29`, `README.md`, `ROADMAP.md` | Reconcile locked IPC allowlist docs with `ctl_ipc.rs` implementation |
| `QUAL-009` | `a21s-*` naming & sysfs path hardcoding inside shared crates | `tools/rabbiz-de/`, `tools/rabbiz-apps/` | Abstract device sysfs paths behind device config (`device.toml`) |
| `QUAL-010` | Blanket `#![allow(clippy::missing_safety_doc)]` in `rabbiz-sdk-c` & unchecked `libc::localtime_r` in `rabbiz-dialer` | `tools/rabbiz-sdk/crates/rabbiz-sdk-c/src/lib.rs`, `tools/rabbiz-apps/crates/rabbiz-dialer/` | Remove blanket allow; check `localtime_r` null return |
| `SC-010` | No SBOM generation or automated CVE/license scanning (`cargo-audit`, `cargo-deny`) | Repo-wide (1,462 locked Rust crate entries) | Add CycloneDX/SPDX SBOM + `cargo-audit` + `cargo-deny` to CI |
| `SC-011` | Minor manifest/lockfile hygiene (`rabbiz-search` missing license, `s2-render` unlocked, `pacman -Sy`) | `tools/rabbiz-apps/crates/rabbiz-search/Cargo.toml`, `scripts/deploy_shell.sh:87,231` | Add `license.workspace = true`, avoid partial upgrades (`pacman -Sy`) |
| `SEC-009` | `RABBIZ_DE_PRIVILEGED_POLICY` env override active in release builds | `tools/rabbiz-de/crates/de-core/src/protocols/privileged.rs:34-38` | Gate env override behind `#[cfg(debug_assertions)]` or `emu` feature |
| `SVC-009` | Unreaped child processes (zombie leak) in `volume-keys.py` | `rootfs-overlay/usr/local/bin/volume-keys.py:48-49` | Reap spawned children (`wait()` / `SIGCHLD` handler) |
| `SVC-010` | `a21s-cbd.c` error handling, unchecked `lseek`/`--chunk`, and missing firmware CRC verification | `tools/cbd/a21s-cbd.c:207,246-248,260,289-324` | Validate CLI numeric bounds, check `lseek` returns, verify firmware digest |
| `SVC-011` | Missing executable bits in overlay repo, non-POSIX `test -O`, hardcoded UID 1001 | `rootfs-overlay/usr/local/bin/{rabbiz-session,a21s-drm-keeper,rabbiz-wifi-connect,rabbiz-osk-ctl}` | Set `chmod +x` in git and replace `[ -O ]` with POSIX check |
| `SVC-012` | Ephemeral screenshot fallback to predictable `/tmp` path in `ctl_ipc.rs` | `tools/rabbiz-de/crates/de-core/src/runtime/ctl_ipc.rs:923-931,1053-1074` | Fail closed if `XDG_RUNTIME_DIR` is unavailable; never fall back to `/tmp` |

*(Note: 24 deduplicated Low items above reflect the 28 individual Low rows where `BM-011`/`QUAL-002`, `BM-010`/`QUAL-005`, `BOOT-005`/`SVC-011` share overlapping root causes, kept explicit for traceability.)*

---

### 4.5 Info Findings (5)

| ID | Title | Location | Note |
|---|---|---|---|
| `BOOT-008` | `getty@tty1.service` is masked via `/dev/null` symlink | `rootfs-overlay/etc/systemd/system/getty@tty1.service` | Good practice: prevents unauthenticated root/user auto-login on `tty1`. |
| `KERN-012` | Baseline ARM64 kernel hardening flags verified enabled in resolved `build/linux/.config` | `build/linux/.config:132,523,551,569,611,848-849,904-911` | Confirms `RANDOMIZE_BASE`, `STACKPROTECTOR_STRONG`, `STRICT_KERNEL_RWX`, `VMAP_STACK`, `SECCOMP_FILTER`, `BPF_UNPRIV_DEFAULT_OFF`, `UNMAP_KERNEL_AT_EL0` are active. |
| `OTA-011` | Declared `[permissions]` in `rabbiz.toml` are not enforced on `.rbz` installation | `tools/rabbiz-sdk/crates/rabbiz-pack/src/lib.rs:99-110,459-482` | Informational until per-app sandbox (`KERN-002`) is implemented. |
| `PRIV-007` | `a21s-telemetry` collects strictly local hardware metrics only | `rootfs-overlay/usr/local/bin/a21s-telemetry:1-54` | Verified: no PII, no disk persistence, no network egress. |
| `QUAL-011` | `TODO`/`FIXME`/`HACK` distribution is clean in first-party Rust | `audit/tools/code-patterns.txt` | 0 markers in first-party Rust crates; 25+ inherited in vendored SCSC drivers. |

---

## 5. Positive Observations (Verified with Evidence)

1. **Strong Keyguard Crypto Design (Client/Core Side):** `tools/rabbiz-de/crates/de-system/src/auth.rs` uses **Argon2id + `getrandom` + `zeroize`**, atomic off-thread `keyguard.state` writes, corruption max-cooldown, and monotonic wall-clock rollback protection.
2. **Loopback-Only Enforcement on `rabbiz-de` TCP Listeners:** `tools/rabbiz-de/crates/de-config/src/flags.rs:171-181` and `tools/rabbiz-de/crates/de-core/src/runtime/ctl_ipc.rs:134-137` strictly reject non-loopback `--ctl-listen` / `--emu-listen` addresses, preventing remote LAN exposure of `CtlIpc`.
3. **Clean Kernel Driver Build Flags:** All 9 Exynos 850 driver `Kbuild`/`Makefile` files (`socs/exynos850/drivers/*/Kbuild`) contain zero hardening-disabling compiler flags (`-fno-stack-protector`, etc.).
4. **Verified Checksum Enforcement on FEX Rootfs:** `scripts/install_fex_rootfs.sh:28` enforces SHA-256 verification before installing rootfs archives.
5. **Privacy-Respecting Telemetry & Secret Hygiene:** `rootfs-overlay/usr/local/bin/a21s-telemetry:1-54` performs strictly local hardware polling with zero egress, and `~/.credentail` / `WIFI_PASSWORD` is never read or printed by any tracked script.
6. **Masked `getty@tty1.service` & Baseline ARM64 Kernel Protections:** `tty1` is masked (`BOOT-008`), and resolved `build/linux/.config` has KASLR, `STACKPROTECTOR_STRONG`, `STRICT_KERNEL_RWX`, `VMAP_STACK`, `SECCOMP_FILTER`, `BPF_UNPRIV_DEFAULT_OFF`, and KPTI enabled (`KERN-012`).

---

## 6. Audit Limitations & What Was NOT Audited ("ไม่ได้ตรวจ")

1. **Unavailable External Tools (`tool unavailable` — compensated manually/offline):**
   - `gitleaks`, `trufflehog`, `semgrep`, `cppcheck`, `syft`, `trivy`, `grype`, `cargo-audit`, `kernel-hardening-checker`, `checksec`, `licensee`, `scancode`, `bandit` were not installed on the host.
   - **Consequence:** No live CVE database lookup against the 1,462 locked Rust crate entries in `Cargo.lock` was performed (Rule 6: no external network). Run `cargo-audit` / `trivy` in CI once available.
2. **Dynamic / Runtime & On-Device Testing (Not Performed):**
   - No compilation, fuzzing, `clang-tidy` with `compile_commands.json`, or live execution on `SM-A217F` / `muyu` hardware was performed (Rule 1 & Rule 6).
3. **Upstream & Gitignored Trees (Not Audited):**
   - `src/` (upstream `samsungexynos850/linux` & `uniLoader`, 72k files), `build/` (except reading `build/linux/.config` to verify Kconfig defaults), `backups/` (7.7G), `drivers/` (legacy ignored dir), and `assets/` media/font blobs were not audited.
4. **Partial Line-by-Line Coverage in Large Subtrees:**
   - Out-of-tree vendor drivers in `socs/exynos850/drivers/` (329 files) and non-security UI crates in `tools/rabbiz-apps/` / `shell/` were scanned via targeted `rg` security patterns (`partial`) rather than read line-by-line. **We make no claim that uninspected lines in those files are free of defects.**

---

## 7. Remediation Roadmap

### Quick Wins (1–3 Days — Eliminate Active Bypasses & LPEs)
1. **Fix Keyguard Bypasses (`SEC-001`, `SEC-002`, `SEC-003`, `CHAL-002`):**
   - Remove `unlock_now()` from `LockHit::Quick` (`lock_ui.rs:716-723`).
   - Disable `a21s-et5xx` autoload & `fp-unlock` trigger; remove `0666` udev rules.
   - Replace `/bin/su` in `rabbiz-auth-helper` with direct PAM `pam_authenticate`, add `SO_PEERCRED`, set `Group=rabbiz`, add read timeouts & line cap (`SVC-005`).
   - Fix `capture_output()` in `composite.rs:351` to check `de.lock.is_locked()`.
2. **Fix Compositor Memory Corruption (`CHAL-001`, `BM-007`):**
   - Replace unchecked `i32` math + `get_unchecked_mut` in `composite.rs:458-494` with `usize::checked_mul` + safe indexing; enable `overflow-checks = true` in all `[profile.release]`.
3. **Eliminate Local Root LPEs (`SVC-001`, `BM-002`, `BM-001`, `PRIV-001`):**
   - Drop root privileges / avoid `chown`/`chmod` in user-owned dirs in `volume-keys.service` & helpers.
   - Replace `/tmp` staging in `de_deploy.sh`/`de_apps_deploy.sh`/`install_chrome_arm.sh` with `mktemp -d /root/.deploy.XXXXXX`.
   - Remove `curl -skL` and unauthenticated `wayvnc` (`--lan`) from `mirror_setup.sh`.
4. **Quick Kernel/Build Hygiene (`KERN-004`, `KERN-009`, `OTA-008`, `PRIV-005`):**
   - Change `mx_panic` procfs mode from `0666` to `0600`; stop `bootstrap_build_env.sh` from applying `0011`/`0012` patches; fix `REPO_ROOT` in `backup_muyu_stock.sh`; redact serials/SSID/MACs in docs.

### Medium-Term (1–3 Weeks — Hardening & Boundaries)
1. **Hardware, Polkit & IPC Hardening (`SEC-004`, `SEC-005`, `SEC-006`, `SEC-008`, `APP-001`, `SVC-007`):**
   - Lock down `/dev/umts_boot0` (`root:root 0600` + `CAP_SYS_ADMIN`), scope Polkit rules (`subject.active && subject.local`), add systemd sandboxing directives to all units, wire `ext-session-lock-v1` dead-locker recovery, harden `de_apps::launch`, and add `CtlIpc` idle timeouts.
2. **Kernel Hardening & Supply Chain (`KERN-001`, `KERN-006`, `KERN-007`, `BM-004`, `BM-006`, `QUAL-001`, `SC-007`):**
   - Remove tracked `.ko.zst` blobs from git; enable `CONFIG_MODULE_SIG_FORCE`, `CONFIG_FORTIFY_SOURCE`, `CONFIG_HARDENED_USERCOPY`, `CONFIG_SLAB_FREELIST_HARDENED`, `CONFIG_SECURITY_LANDLOCK`, `CONFIG_SECURITY_YAMA`, `CONFIG_SECURITY_DMESG_RESTRICT`; disable `DEVMEM`/`KEXEC`/`KALLSYMS_ALL`.
   - Fix `vendor/smithay` so clean clones & CI build; pin GitHub Actions to SHAs; add `shellcheck`, `cargo-audit`, `cargo-deny`, and all Rust workspaces to CI; add C hardening flags to `tools/cbd/Makefile`.
3. **App & SDK Security (`APP-002`, `APP-004`, `APP-006`, `APP-007`, `APP-008`, `PRIV-003`, `PRIV-004`):**
   - Add Ed25519 signing + path validation to `rabbiz-pack`; remove `sh -c` in `rabbiz-store`; pass Wi-Fi PSKs via D-Bus/stdin in `rabbiz-settings`; bind emulator Docker/QEMU ports to `127.0.0.1`; fix `public_history.sh`.

### Long-Term (1–3 Months — Platform Architecture)
1. **Per-App Sandboxing & MAC (`KERN-002`, `OTA-011`):** Enforce `[permissions]` in `rabbiz.toml` via Landlock + seccomp + bubblewrap/Flatpak or AppArmor/SELinux profiles so apps do not share unrestricted `rabbiz` UID access.
2. **Signed Atomic OTA & Verified Boot (`BOOT-001`, `BOOT-002`, `OTA-005`):** Implement A/B atomic updates with cryptographic signatures and rollback protection (especially targeting Phase 2 Xiaomi Pad 7 Pro `muyu`), plus `dm-verity` read-only rootfs and BFU encryption/greeter isolation.
3. **IOMMU & Driver Modernization (`KERN-005`, `KERN-008`, `KERN-010`):** Restore `&sysmmu_dpu` IOMMU protection and audit/harden remaining vendored `socs/exynos850/drivers/` IOCTL/sysfs surfaces.
