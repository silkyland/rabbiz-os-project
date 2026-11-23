# Phase 4: Independent Adversarial Challenge Log (`CHAL`)

**Repository:** `/home/dit/Sites/Labs/android-arm-linux` (RabbizOS)  
**Mode:** Strictly read-only verification against source files (no builds, no network, no root commands).  
**Scope:**
1. Adversarial challenge of **all 75 Medium+ findings** across the 10 Round 1 domain reports (`audit/findings/*.md`). *(Note: 75 total Medium+ findings were found across the 10 reports: 35 High + 40 Medium.)*
2. Independent 10%+ spot-check of **25 randomly sampled `reviewed` files** (12.5% of individual reviewed files), including full inspection of `tools/rabbiz-sdk/crates/rabbiz-sdk-c/src/lib.rs` (31 `unsafe` blocks).

---

## 1. Summary Metrics

| Metric | Count | Notes |
|---|---:|---|
| **Total Round 1 Medium+ Findings Challenged** | **75** | 35 High + 40 Medium across all 10 domain reports |
| **Confirmed (as-is)** | **43** | 16 High + 27 Medium |
| **Upgraded** | **0** | No finding meets the Critical rubric (no remote system compromise, live OTA/boot bypass, or primary signing key leak) |
| **Downgraded (Canonical)** | **8** | 5 High → Medium (`BM-003`, `APP-002`, `APP-003`, `QUAL-001`, `SVC-003`); 3 Medium → Low (`QUAL-002`, `QUAL-004`, `QUAL-005`) |
| **Merged Cross-Domain Duplicates** | **24** | Mapped to 18 canonical IDs (including `APP-005` & `SC-002` downgraded High → Medium on merge, and `SEC-007` partial FP on `CONFIG_SECCOMP`) |
| **Rejected (Full False Positive)** | **0** | 1 partial FP caught & corrected (`SEC-007` `CONFIG_SECCOMP` claim); 1 Round 1 Positive Observation refuted (`PRIV` #3 lock-screen capture blanking) |
| **New Findings Discovered in Spot-Check** | **3** | 2 High (`CHAL-001`, `CHAL-002`) + 1 Low (`CHAL-003`) |
| **Final Deduplicated Medium+ Count** | **50** | **0 Critical · 18 High · 32 Medium** (plus 3 downgraded to Low and 1 new Low) |

---

## 2. Complete Verdict Table (All 75 Round 1 Medium+ Findings)

Sorted by Canonical ID, grouping merged cross-domain duplicates directly under their canonical parent.

| Finding ID | Domain | Orig Sev | Final Sev | Verdict | Canonical ID | Source-Verified Reasoning |
|---|---|---:|---:|---|---|---|
| `APP-001` | APP | High | **High** | `CONFIRMED` | `APP-001` | Verified `de-apps/src/launch.rs:40-51,112-120,151-161`: unknown `desktop_id` falls back verbatim to `split_shellish` + `setsid` (no `--`), and `desktop_id` is joined raw into `.desktop` and `.log` paths (`../` traversal). |
| `APP-002` | APP | High | **Medium** | `DOWNGRADED` | `APP-002` | Verified `rabbiz-store/src/main.rs:790-842` & `backend.rs:118-120,227-241`: `sh -c` interpolation + `paru --noconfirm`. Downgraded to Medium (matching `OTA-006`) because `state.json` is same-UID `rabbiz`-owned (requires prior local write, unset `HOME`, or compromised AUR package). |
| `OTA-006` | OTA | Medium | — | `MERGED_DUP (into APP-002)` | `APP-002` | Exact duplicate of `APP-002` (`rabbiz-store` `state.json` shell injection and `paru --noconfirm`). |
| `APP-003` | APP | High | **Medium** | `DOWNGRADED` | `APP-003` | Verified `DeviceMirrorService.swift:12-21` & `linux/src/device_service.rs:17-34`: missing `--` before `target` allows `-oProxyCommand=` and uses `StrictHostKeyChecking=accept-new`. Downgraded to Medium because trigger requires user pasting a crafted host string into their own GUI or active Wi-Fi MITM on first SSH connect. |
| `APP-004` | APP | High | **High** | `CONFIRMED` | `APP-004` | Verified `rabbiz-pack/src/lib.rs:158-208,389-398,459-482`: self-checksum only (no signature), `app.id = ".."` passes `validate()`, absolute/`..` `app.exec` replaces `install_dir` in `Path::join` + `chmod 0755`, unescaped newlines allow `.desktop` INI injection, and double-read TOCTOU. |
| `OTA-002` | OTA | High | — | `MERGED_DUP (into APP-004)` | `APP-004` | Duplicate of `APP-004` (missing `.rbz` cryptographic signature + double-read TOCTOU in `install_rbz_local`). |
| `OTA-003` | OTA | High | — | `MERGED_DUP (into APP-004)` | `APP-004` | Duplicate of `APP-004` (`app.id`/`app.exec` path traversal, arbitrary `chmod 0755`, and `.desktop` INI injection in `rabbiz-pack`). |
| `APP-006` | APP | Medium | **Medium** | `CONFIRMED` | `APP-006` | Verified `RFBClient.swift:294-311,406-407,492-494` & `rabbiz-emu-vnc.rs:50-54,130-132`: RFB Security Type 1 (None) required, unbounded `nameLength`/`ServerCutText`/`DesktopSize` OOM allocations in Swift client, framing desync in `rabbiz-emu-vnc`. |
| `APP-007` | APP | Medium | **Medium** | `CONFIRMED` | `APP-007` | Verified `rabbiz-wasm-runner/src/lib.rs:308-314,423,505-514,559-584`: unbounded recursion stack abort, unbounded locals `vec!`, and negative `i32 as usize` wraparound in `I32Load`/`I32Store` release builds. |
| `APP-008` | APP | Medium | **Medium** | `CONFIRMED` | `APP-008` | Verified `rabbiz-settings/src/system.rs:373-384,415`: passes plaintext Wi-Fi PSK in `nmcli` argv (visible in `/proc/<pid>/cmdline`, regressing `rabbiz-wifi-connect`) and omits `--` before `&ssid`. |
| `PRIV-002` | PRIV | Medium | — | `MERGED_DUP (into APP-008)` | `APP-008` | Exact duplicate of `APP-008` (`rabbiz-settings/src/system.rs:370-381` Wi-Fi PSK in `nmcli` argv). |
| `APP-009` | APP | Medium | **Medium** | `CONFIRMED` | `APP-009` | Verified `SystemControl.qml:114-115,180-187` & `Brightness.qml:74-81`: unquoted shell interpolation in legacy QML shell still deployed by `scripts/deploy_shell.sh:122-125`. |
| `APP-010` | APP | Medium | **Medium** | `CONFIRMED` | `APP-010` | Verified `rabbiz-search/src/apps.rs:133-156` (untracked): unvalidated scraped URLs passed to `google-chrome`/`xdg-open` without scheme allowlist or `--`, plus `--password-store=basic`. |
| `BM-001` | BM | High | **High** | `CONFIRMED` | `BM-001` | Verified `scripts/mirror_setup.sh:47-56`: `curl -skL` disables TLS verification when downloading Arch `.pkg.tar.xz` archives installed as root via `pacman -U --noconfirm` from predictable `/tmp/`. |
| `OTA-001` | OTA | High | — | `MERGED_DUP (into BM-001)` | `BM-001` | Exact duplicate of `BM-001` (`scripts/mirror_setup.sh:47-56` `curl -skL` + root `pacman -U`). |
| `SC-001` | SC | High | — | `MERGED_DUP (into BM-001)` | `BM-001` | Exact duplicate of `BM-001` (`scripts/mirror_setup.sh:47-56` `curl -skL` + root `pacman -U`). |
| `BM-002` | BM | High | **High** | `CONFIRMED` | `BM-002` | Verified `de_deploy.sh:34-70`, `de_apps_deploy.sh:116-130`, `install_chrome_arm.sh:22-26`, `setup_dumb_destroy_fix.sh:6-9`: predictable `/tmp` & `/var/tmp` staging paths allow `rabbiz -> root` TOCTOU LPE during deploy. *(Spot-check also found `mirror_setup.sh:65` writes as root to `/home/rabbiz/.config/systemd/user/a21s-wayvnc.service`.)* |
| `OTA-004` | OTA | High | — | `MERGED_DUP (into BM-002)` | `BM-002` | Duplicate of `BM-002` (adds `install_chrome_arm.sh` & `setup_dumb_destroy_fix.sh` instances to the same predictable `/tmp`/`/var/tmp` root deploy staging class). |
| `BM-003` | BM | High | **Medium** | `DOWNGRADED` | `BM-003` | Verified `scripts/bootstrap_build_env.sh:6,8,44-46`: `|| true` on `uniLoader` fetch/checkout + short SHAs. Downgraded to Medium (matching `BOOT-004` & `SC-005`) because exploitation requires upstream repo compromise or >20-commit drift during fresh bootstrap. |
| `BOOT-004` | BOOT | Medium | — | `MERGED_DUP (into BM-003)` | `BM-003` | Exact duplicate of `BM-003` (`bootstrap_build_env.sh` `uniLoader` `|| true` + short commit hashes). |
| `SC-005` | SC | Medium | — | `MERGED_DUP (into BM-003)` | `BM-003` | Exact duplicate of `BM-003` (`bootstrap_build_env.sh` `uniLoader` `|| true` + short commit hashes). |
| `BM-004` | BM | Medium | **Medium** | `CONFIRMED` | `BM-004` | Verified `.github/workflows/host-checks.yml:1-42`: mutable `@v4`/`@stable` action refs and no `permissions:` block. |
| `SC-006` | SC | Medium | — | `MERGED_DUP (into BM-004)` | `BM-004` | Duplicate of `BM-004` (mutable CI actions + missing `permissions:`; toolchain drift is covered by `BM-008`, workspace coverage by `QUAL-002`/`BM-011`). |
| `BM-005` | BM | Medium | **Medium** | `CONFIRMED` | `BM-005` | Verified `install_chrome_arm.sh:17-26` & `setup_panvk_test.sh:30-40`: unpinned/unverified remote binaries downloaded over TLS (with verification intact) without GPG/SHA-256 checks; `setup_panvk_test.sh` also omits `-f`. |
| `OTA-007` | OTA | Medium | — | `MERGED_DUP (into BM-005)` | `BM-005` | Duplicate of `BM-005` (`scripts/install_chrome_arm.sh:17-26` unverified Chrome `.deb`). |
| `SC-002` | SC | High | — | `MERGED_DUP (into BM-005)` | `BM-005` | Duplicate of `BM-005` (`install_chrome_arm.sh`); also downgraded from High → Medium because TLS verification is enabled (unlike `curl -k` in `BM-001`), requiring upstream/CDN compromise. |
| `SC-008` | SC | Medium | — | `MERGED_DUP (into BM-005)` | `BM-005` | Duplicate of `BM-005` (`scripts/setup_panvk_test.sh:30-40` unverified `.so` download without `-f` or SHA-256). |
| `BM-006` | BM | Medium | **Medium** | `CONFIRMED` | `BM-006` | Verified `tools/cbd/Makefile:7-12`: root C modem boot daemon omits `-fstack-protector-strong`, `-D_FORTIFY_SOURCE=2`, `-fPIE -pie`, full RELRO, and ignores `LDFLAGS`. |
| `BM-007` | BM | Medium | **Medium** | `CONFIRMED` | `BM-007` | Verified 5/6 Rust workspaces omit `[profile.release]`; all 6 leave `overflow-checks = false` (directly enabling `CHAL-001` in `composite.rs`!). |
| `BM-008` | BM | Medium | **Medium** | `CONFIRMED` | `BM-008` | Verified floating `rust:1.99-bookworm` Docker tag, `dtolnay/rust-toolchain@stable` in CI vs `1.99.1` in `rust-toolchain.toml`, unpinned `apt-get`, and omitted `--locked`. |
| `SC-009` | SC | Medium | — | `MERGED_DUP (into BM-008)` | `BM-008` | Duplicate of `BM-008` (unpinned Docker base image tags and `apt-get` versions in `tools/rabbiz-de/Dockerfile` and `tools/rabbiz-sdk/Dockerfile.emu`). |
| `BOOT-001` | BOOT | High | **High** | `CONFIRMED` | `BOOT-001` | Verified unlocked S-Boot (`OEM LOCK: OFF`), unsigned AVB (`Algorithm: NONE`), and no `CONFIG_DM_VERITY` / `CONFIG_MODULE_SIG`. Kept High (NOT Critical) because S-Boot e-fuse lock is a Samsung Exynos 850 platform constraint for mainline Linux and requires physical/local root access. |
| `BOOT-002` | BOOT | High | **High** | `CONFIRMED` | `BOOT-002` | Verified `rabbiz-session.service:31-47` & `rabbiz-session:58-82,89-131`: passwordless auto-login on boot; `if [ "$MODE" != "de" ]` at line 80 bypasses `keyguard_requires_lock()`, which itself trusts user-writable `prefs.json`. |
| `KERN-001` | KERN | High | **High** | `CONFIRMED` | `KERN-001` | Verified `build/linux/.config:951,8857` (`CONFIG_MODULE_SIG` & `CONFIG_SECURITY_LOCKDOWN_LSM` not set) and 4 unsigned `.ko.zst` blobs tracked in git under `socs/exynos850/drivers/scsc/`. |
| `SC-003` | SC | High | — | `MERGED_DUP (into KERN-001)` | `KERN-001` | Duplicate of `KERN-001` (4 pre-compiled, unsigned `.ko.zst` kernel modules tracked in git). |
| `QUAL-003` | QUAL | Medium | — | `MERGED_DUP (into KERN-001)` | `KERN-001` | Duplicate of `KERN-001` (tracked `*.ko.zst` binaries and `.gitignore` gap; note minor `.gitignore` omission for `tools/rabbiz-camera/target/`). |
| `KERN-002` | KERN | High | **High** | `CONFIRMED` | `KERN-002` | Verified `build/linux/.config:2648,8851-8859,8867`: zero MAC/hardening LSMs (`SELINUX`, `APPARMOR`, `YAMA`, `LANDLOCK`, `LOCKDOWN`, `LOADPIN` all unset) and `CONFIG_DM_VERITY` unset. |
| `SEC-007` | SEC | Medium | — | `MERGED_DUP (into KERN-002)` | `KERN-002` | Duplicate of `KERN-002` **with partial False Positive caught**: `SEC-007` claimed `CONFIG_SECCOMP` was missing by scanning `savedefconfig` (`a21s_linux_defconfig`), but `build/linux/.config:848-849` confirms `CONFIG_SECCOMP=y` & `CONFIG_SECCOMP_FILTER=y`. Valid MAC/LSM/per-app sandboxing gap merges into `KERN-002`. |
| `KERN-004` | KERN | Medium | **Medium** | `CONFIRMED` | `KERN-004` | Verified `socs/exynos850/drivers/scsc/misc/mxproc.c:421`: `mx_panic` created with `S_IWOTH` (`0666`), allowing any unprivileged user to trigger `mxman_force_panic()` (Wi-Fi/BT DoS). |
| `KERN-005` | KERN | Medium | **Medium** | `CONFIRMED` | `KERN-005` | Verified `0003-a21s-abox-audio-dts.patch:18-20` & `0004-exynos-drm-dma-iommu-optional.patch:10`: `&sysmmu_dpu` disabled and Exynos DRM IOMMU attachment bypassed. |
| `KERN-006` | KERN | Medium | **Medium** | `CONFIRMED` | `KERN-006` | Verified `build/linux/.config`: `FORTIFY_SOURCE`, `HARDENED_USERCOPY`, `SLAB_FREELIST_HARDENED/RANDOM`, `INIT_ON_ALLOC_DEFAULT_ON` are all unset. |
| `KERN-007` | KERN | Medium | **Medium** | `CONFIRMED` | `KERN-007` | Verified `build/linux/.config:317-318,3687,8840,9348-9352` & `10-a21s-quiet.conf`: `DEBUG_FS`, `KEXEC`, `DEVMEM`, `MAGIC_SYSRQ`, `KALLSYMS_ALL` enabled; `DMESG_RESTRICT` unset; no hardening `sysctl.d`. |
| `BOOT-003` | BOOT | Medium | — | `MERGED_DUP (into KERN-007)` | `KERN-007` | Duplicate of `KERN-007` (`kexec`, `debugfs`, `Magic SysRq`, `KALLSYMS_ALL`, and legacy `bootargs`). |
| `KERN-008` | KERN | Medium | **Medium** | `CONFIRMED` | `KERN-008` | Verified `socs/exynos850/drivers/cpif/cpif_power.c:232-236,256-259,285-300`: `vgpio` has zero bounds check on `off` (arbitrary kernel MMIO OOB read), off-by-3 alignment checks, and unconditional CP RAM linear-map poke (`linw`) in autoloaded driver. |
| `KERN-009` | KERN | Medium | **Medium** | `CONFIRMED` | `KERN-009` | Verified `scripts/bootstrap_build_env.sh:32-40` globs `*.patch` and unconditionally applies `0011` and `0012` (confirmed `!!! DO NOT FLASH !!!` TZPC boot-hang in `0012:5-8`). |
| `OTA-005` | OTA | High | **High** | `CONFIRMED` | `OTA-005` | Verified no OTA framework (`rabbiz-updater` is only UI log strings), single `BOOT` partition (`/dev/mmcblk0p18`), manual `dd`, and no rollback protection. Kept High (architectural gap, not a remote exploit). |
| `OTA-008` | OTA | Medium | **Medium** | `CONFIRMED` | `OTA-008` | Verified `devices/xiaomi/muyu/scripts/backup_muyu_stock.sh:9-12` (`REPO_ROOT` resolves to `devices/xiaomi/muyu/`, not repo root) while `.gitignore:7` anchors `/backups/`, writing serials & EFS/persist dumps into an unignored git path. |
| `OTA-009` | OTA | Medium | **Medium** | `CONFIRMED` | `OTA-009` | Verified `scripts/deploy_shell.sh:159-160,182-187`: `tar -C / -xf -` without `--no-same-owner` transiently reassigns `/` and `/etc` ownership to host UID (potential `rabbiz` UID collision) and omits `device_session.sh` lock. |
| `PRIV-001` | PRIV | High | **High** | `CONFIRMED` | `PRIV-001` | Verified `scripts/mirror_setup.sh:14,21,75,80`: persistent `a21s-wayvnc.service` binds `172.16.42.1` + `127.0.0.1` (or `0.0.0.0` via `--lan`) with RFB Type 1 (None) while `/usr/bin/wayvnc` is allow-listed in `privileged.rs:219`, defeating compositor privilege policy. |
| `PRIV-003` | PRIV | Medium | **Medium** | `CONFIRMED` | `PRIV-003` | Verified `scripts/release/public_history.sh:67-82,96,120,129-134`: line 96 `git fetch "$SRC"` re-imports unredacted objects into `$OUT/.git/objects` after `filter-repo`; misses `/home/<user>/` and `--replace-message`; exits 0 without `gitleaks`. |
| `PRIV-004` | PRIV | Medium | **Medium** | `CONFIRMED` | `PRIV-004` | Verified `rabbiz-emu-viewer/src/main.rs:166,179-182` (`docker run -p 5910:5910 -p 5911:5911`), `Dockerfile.emu:22`, `rabbiz-cli/src/lib.rs:585` (`hostfwd=tcp::PORT-:GUEST`), and `rabbiz-emu-vnc.rs:19-23`. Hardened `rabbiz-de` (`flags.rs:175`) rejects `0.0.0.0` binds, mitigating 5911 RCE on current binaries while leaving SSH `:2222` exposed on `0.0.0.0` and Docker/QEMU configs unsafe. |
| `APP-005` | APP | High | — | `MERGED_DUP (into PRIV-004)` | `PRIV-004` | Duplicate of `PRIV-004` (`rabbiz-cli/src/lib.rs:583-587` QEMU `hostfwd` on `0.0.0.0`). Downgraded High → Medium on merge because current `rabbiz-de` rejects non-loopback `--ctl-listen`/`--emu-listen` (`flags.rs:175`, `ctl_ipc.rs:134`), breaking the claimed remote RCE chain on port 5911 while leaving dev VM SSH `:2222` exposed. |
| `PRIV-005` | PRIV | Medium | **Medium** | `CONFIRMED` | `PRIV-005` | Verified `a21s-access-decision.md:4-5` (real Samsung serial), `a21s-wifi-bluetooth.md:139,149` (home SSID, BT MAC), LAN IPs, host path in `docs/PLAN.md:8` (plus `muyu` serial in `ROADMAP.md:51` noted in `QUAL-008`). |
| `QUAL-001` | QUAL | High | **Medium** | `DOWNGRADED` | `QUAL-001` | Verified `tools/rabbiz-de/Cargo.toml:88-89` patches `smithay` to gitignored `vendor/smithay` (`.gitignore:6`), breaking clean clones & CI. Downgraded High → Medium (matching `SC-004`): build/CI breakage, not a security privilege escalation or secret leak. |
| `SC-004` | SC | Medium | — | `MERGED_DUP (into QUAL-001)` | `QUAL-001` | Exact duplicate of `QUAL-001` (gitignored `tools/rabbiz-de/vendor/smithay` breaks clean builds and CI). |
| `QUAL-002` | QUAL | Medium | **Low** | `DOWNGRADED` | `QUAL-002` | Verified `.github/workflows/host-checks.yml` omits 5 Rust workspaces & `shellcheck`. Downgraded Medium → Low (quality/CI coverage defect, aligning with `BM-011` Low). |
| `QUAL-004` | QUAL | Medium | **Low** | `DOWNGRADED` | `QUAL-004` | Verified 60+ modified/untracked files across lanes in working tree. Downgraded Medium → Low (git workflow/hygiene defect, not a security vulnerability). |
| `QUAL-005` | QUAL | Medium | **Low** | `DOWNGRADED` | `QUAL-005` | Verified shellcheck warnings (`SC2221`/`SC3067`/`SC2155`/`SC2034`) in `de_apps_deploy.sh:108`, `rabbiz-osk-ctl:13`, `rabbiz-labwc-session:50`, `audio-up.sh:22`. Downgraded Medium → Low (quality/portability defects, aligning with `BM-010` & `SVC-011` Low). |
| `SC-007` | SC | Medium | **Medium** | `CONFIRMED` | `SC-007` | Verified `cme3_prim.h:1-8` ("Confidential information of Samsung... Refer to LICENSE.txt"), missing `LICENSE.txt`, 345/365 driver files without SPDX, and GPL-2.0 tools under root Apache-2.0 `LICENSE`. |
| `SEC-001` | SEC | High | **High** | `CONFIRMED` | `SEC-001` | Verified `lock_ui.rs:266-274,716-723`: tapping bottom-right Quick Action calls `self.lock.unlock_now()` unconditionally (zero guard inside `unlock_now()`), setting `self.locked = false` and `Screen::Home` with no PIN/Passphrase check. |
| `SEC-002` | SEC | High | **High** | `CONFIRMED` | `SEC-002` | Verified end-to-end chain: `a21s_et5xx.c:87-119,188-204,307-318` (no biometric matching; any DRDY edge or write to `/dev/esfp0` or `trigger_unlock` records unlock event) + `99-a21s-modem-fp.rules:1,3` & `a21s-modem-ctl:13-14` (`0666`) + `power.rs:337-342,472-475` (unconditional `self.lock.unlock_now()`). |
| `KERN-003` | KERN | High | — | `MERGED_DUP (into SEC-002)` | `SEC-002` | Duplicate of `SEC-002` (kernel driver & udev side of the `a21s-et5xx` fingerprint unlock bypass chain). |
| `SEC-003` | SEC | High | **High** | `CONFIRMED` | `SEC-003` | Verified `auth_helper.rs:29-45,59-75,95-134` & `de-system/src/auth.rs:483-495` & `lock_ui.rs:82-98`: `/bin/su -c true <user>` as root bypasses PAM via `pam_rootok`, `LockMethod::Swipe => true` resets lockout/BFU, missing `SO_PEERCRED`, missing `--` before `user`, and `root:root 0660` socket forces silent fallback to user-writable `~/.config/rabbiz-shell/keyguard.json`. |
| `SVC-002` | SVC | High | — | `MERGED_DUP (into SEC-003)` | `SEC-003` | Exact duplicate of `SEC-003` (`rabbiz-auth-helper` root `/bin/su` bypass, `Swipe` lockout reset, missing `SO_PEERCRED`, and `su` arg injection). |
| `SEC-004` | SEC | High | **High** | `CONFIRMED` | `SEC-004` | Verified `99-a21s-modem-fp.rules:1-3`, `a21s-modem-ctl:13-16`, `99-a21s-sensors-vib.rules:2`, `90-a21s-backlight.rules:2`, `rabbiz-session.service:34`: `rabbiz` (group `video`) gets direct rw/ioctl access to `/dev/umts_boot0` (no `CAP_SYS_ADMIN` in `cpif_boot_ioctl`) & `/dev/umts_ipc*`, plus `0666` on `/dev/esfp0` & vibrator. |
| `SVC-004` | SVC | High | — | `MERGED_DUP (into SEC-004)` | `SEC-004` | Duplicate of `SEC-004` (`a21s-modem-ctl:12-17` `chmod 666` fingerprint and `chgrp video` `/dev/umts_*`). |
| `SEC-005` | SEC | High | **High** | `CONFIRMED` | `SEC-005` | Verified `51-rabbiz-shell.rules:9-15` (unconditional `Result.YES` for all `org.freedesktop.NetworkManager.*` incl. `GetSecrets` & global DNS, no `subject.active`/`local` check) & `50-rabbiz-session.rules:4-15` (unconditional `steamos-session.service` start conflicts with locked `rabbiz-session.service`). |
| `SEC-006` | SEC | Medium | **Medium** | `CONFIRMED` | `SEC-006` | Verified zero systemd sandboxing across all 9 root units, `/root/{audio,charger}-up.sh` invocations, and `rabbiz-osk-switch:12,45` writing `/tmp/rabbiz-osk.log` as root. |
| `SVC-006` | SVC | Medium | — | `MERGED_DUP (into SEC-006)` | `SEC-006` | Exact duplicate of `SEC-006` (missing systemd sandboxing across root daemons, `/root/*.sh`, and `/tmp/rabbiz-osk.log`). |
| `SEC-008` | SEC | Medium | **Medium** | `CONFIRMED` | `SEC-008` | Verified `session_lock.rs:81-96`: `on_external_client_died()` and `on_recovery_authenticated()` are `#[allow(dead_code)]` with zero non-test callers; dead external locker permanently strands session. |
| `SVC-001` | SVC | High | **High** | `CONFIRMED` | `SVC-001` | Verified `volume-keys.service:6` (root), `volume-keys.py:30,42`, `a21s-volume:107-122`, `a21s-power-key:35-42`, `rabbiz-screenshot:41-43,60,71-76`: root follows symlinks and runs `chown`/`chmod 666` inside non-sticky `rabbiz`-owned `/run/user/<uid>` and `~/Pictures/Screenshots` (`fs.protected_symlinks` does NOT protect non-sticky user-owned dirs) -> direct `rabbiz -> root` LPE. |
| `SVC-003` | SVC | High | **Medium** | `DOWNGRADED` | `SVC-003` | Verified `rabbiz-wifi-connect:41` (`ask "connection modify \"$PROFILE\" wifi-sec.psk"` -> `script -qec` shell injection). Downgraded High → Medium because it runs as same-user `rabbiz` (no LPE), is only invoked by the legacy QML shell (`Network.qml`), and requires the user to join a malicious SSID and later reconnect with a PSK. |
| `SVC-005` | SVC | Medium | **Medium** | `CONFIRMED` | `SVC-005` | Verified `auth_helper.rs:95-113,126-136`: single-threaded blocking accept loop with no timeouts (permanent Keyguard DoS), unbounded `read_line` (OOM), and `bind`→`chmod` race. |
| `SVC-007` | SVC | Medium | **Medium** | `CONFIRMED` | `SVC-007` | Verified `ctl_ipc.rs:67-70,161-166,322-340`: 64 idle `CtlClient` slots with no timeout (permanent IPC DoS) and unbounded `EmuFrameServer` client list. |
| `SVC-008` | SVC | Medium | **Medium** | `CONFIRMED` | `SVC-008` | Verified `rabbiz-spotify-ctl:277-289,366-376`: `angelfish` launched with `QTWEBENGINE_CHROMIUM_FLAGS="--no-sandbox --disable-gpu"` and `mpv` invoked with `"${url}"` without `--`. |

---

## 3. Detailed Challenge Notes (Discrepancies, Downgrades & Nuanced Verdicts)

1. **`SEC-007` vs `KERN-012` — Partial False Positive Caught (`CONFIG_SECCOMP`):**
   - `SEC-007` ran `rg` on `devices/samsung/a21s/kernel/a21s_linux_defconfig` (a minimal `savedefconfig`) and reported `CONFIG_SECCOMP` / `CONFIG_SECCOMP_FILTER` as missing.
   - Direct verification of `build/linux/.config:848-849` confirms `CONFIG_SECCOMP=y` and `CONFIG_SECCOMP_FILTER=y` **are enabled** in the resolved Linux 7.3.0 arm64 kernel.
   - `SEC-007`'s valid core issue (zero active MAC/hardening LSMs: no SELinux, AppArmor, Yama, Landlock, Lockdown, or LoadPin, plus no per-app sandboxing) is merged into canonical **`KERN-002`**.

2. **`APP-005` + `PRIV-004` + `APP-001` Chain — Does NOT Reach Critical RCE on Current Binary:**
   - `APP-005` claimed QEMU `hostfwd=tcp::5911-:5911` on `0.0.0.0` chains with `APP-001` (`de_apps::launch`) for unauthenticated LAN RCE inside the VM.
   - Direct verification of `tools/rabbiz-de/crates/de-config/src/flags.rs:171-181` and `tools/rabbiz-de/crates/de-core/src/runtime/ctl_ipc.rs:134-137` proves that current `rabbiz-de` **strictly rejects non-loopback addresses** (`0.0.0.0`) for `--ctl-listen` and `--emu-listen`. Thus, `0.0.0.0:5911` cannot currently be bound by `rabbiz-de` inside the VM/Docker container.
   - However, `APP-005` still exposes guest SSH (`hostfwd=tcp::2222-:22`) on `0.0.0.0` of the developer host, and both `rabbiz-cli` (`hostfwd`) and `rabbiz-emu-viewer` (`docker run -p 5910:5910 -p 5911:5911`) remain unsafe if an older/unhardened binary or proxy runs inside. Therefore `APP-005` is **downgraded High → Medium** and merged into **`PRIV-004`**.

3. **`BOOT-001` & `OTA-005` — Kept High (Not Upgraded to Critical):**
   - Under the rubric, Critical requires a remote system compromise, live boot/OTA bypass, or primary signing key leak.
   - On Samsung Galaxy A21s (`SM-A217F`), stock BL1/BL2/S-Boot are e-fuse locked to Samsung's keys (`docs/PLAN.md:13,44`), so `OEM LOCK: OFF` and unsigned AVB (`Algorithm: NONE`) are an inherent hardware platform constraint for an aftermarket mainline Linux port, not a remote exploit. Both `BOOT-001` (zero downstream integrity: no `CONFIG_MODULE_SIG`, no `CONFIG_DM_VERITY`) and `OTA-005` (no signed OTA/A-B architecture) are **confirmed at High**.

4. **Severity Normalizations (5 High → Medium, 3 Medium → Low):**
   - `BM-003` (`bootstrap_build_env.sh` `uniLoader` `|| true`): requires upstream repo compromise or >20-commit drift on fresh bootstrap → **Medium** (aligning with `BOOT-004` & `SC-005`).
   - `SC-002` (merged into `BM-005`, `install_chrome_arm.sh`): uses HTTPS with TLS certificate verification intact (unlike `BM-001`'s `curl -k`) → **Medium** (aligning with `BM-005` & `OTA-007`).
   - `APP-002` (`rabbiz-store` `state.json` shell injection + `paru --noconfirm`): `state.json` is owned by the same user `rabbiz`; requires prior write/unset `HOME` or compromised AUR package → **Medium** (aligning with `OTA-006`).
   - `APP-003` (`RabbizMirror` SSH option injection & TOFU): requires user typing/pasting `-oProxyCommand=...@...` into their own desktop GUI or active Wi-Fi MITM on first SSH connect → **Medium**.
   - `SVC-003` (`rabbiz-wifi-connect` profile shell injection): same-user `rabbiz` execution, legacy QML shell only, requires user joining crafted SSID and later reconnecting with PSK → **Medium**.
   - `QUAL-001` (gitignored `vendor/smithay` breaks clean clone & CI): operational/CI build breakage, not a security privilege escalation → **Medium** (aligning with `SC-004`).
   - `QUAL-002` (CI test/lint blind spots), `QUAL-004` (working tree sprawl), `QUAL-005` (shellcheck lint defects): quality/process/portability defects → **Low** (aligning with `BM-010`, `BM-011`, and `SVC-011`).

5. **Full Confirmation of All 4 Critical-Impact Local Security Chains:**
   - `SEC-001` (`lock_ui.rs:716-723` + `266-274`): Verified `LockHit::Quick` calls `self.lock.unlock_now()` with zero auth check inside either caller or callee.
   - `SEC-002` (`a21s_et5xx.c` + `99-a21s-modem-fp.rules` + `power.rs:337-342,472-475`): Verified complete hardware & software biometric lockscreen bypass chain.
   - `SEC-003` (`auth_helper.rs` + `auth.rs` + `lock_ui.rs:82-98`): Verified root `/bin/su` PAM bypass, `Swipe` lockout reset, missing `SO_PEERCRED`, and `root:root 0660` socket forcing silent fallback to user-writable `keyguard.json`.
   - `SVC-001` (`volume-keys.service` + `volume-keys.py` + `a21s-volume` + `a21s-power-key` + `rabbiz-screenshot`): Verified direct `rabbiz -> root` LPE via symlink following + `chown`/`chmod 666` inside non-sticky `rabbiz`-owned directories.

---

## 4. 10%+ Spot-Check Log (25 Randomly Sampled `reviewed` Files)

All 25 sampled files were verified to exist and directly read in full.

| # | File | Round 1 Claimed Coverage | Spot-Check Verification | Missed Issues / Notes |
|---:|---|---|---|---|
| 1 | `devices/samsung/a21s/docs/a21s-access-decision.md` | `PRIV` (`direct-read`) | Verified (`PRIV-005`) | Confirmed real Samsung serial on lines 4–5 (redacted). Accurate. |
| 2 | `devices/samsung/a21s/docs/device-a21s-profile.txt` | `PRIV` (`direct-read`) | Verified (no issues) | Confirmed model/OS/package whitelist dump only; no serial/IMEI/MAC PII. Accurate. |
| 3 | `devices/samsung/a21s/kernel/patches/0002-asoc-topology-visible.patch` | `KERN` (`direct-read`) | Verified (no issues) | 1-line Kconfig prompt visibility change for `SND_SOC_TOPOLOGY`. Benign. |
| 4 | `devices/samsung/a21s/kernel/patches/0006-s2mu106-fuelgauge-compatible.patch` | `KERN` (`direct-read`) | Verified (no issues) | Clean upstream backport adding `s2mu106` variant descriptor & optional IRQ check (`client->irq > 0`). Benign. |
| 5 | `devices/samsung/a21s/kernel/patches/0009-a21s-tmu-thermal-zones.patch` | `KERN` (`direct-read`) | Verified (no issues) | Clean DTS thermal zones (`little0`, `little1`, `gpu`, 80°C passive / 95°C critical). Benign. |
| 6 | `devices/samsung/a21s/kernel/patches/0012-a21s-et520-spi-dts.patch` | `KERN` (`direct-read`) | Verified (`KERN-009`) | Confirmed `!!! DO NOT FLASH !!!` TZPC hang warning while `bootstrap_build_env.sh` applies `*.patch`. Accurate. |
| 7 | `rootfs-overlay/etc/modules-load.d/a21s-hardware.conf` | `KERN` (`direct-read`) | Verified (`KERN-003`/`KERN-008`) | Autoloads `a21s-cpif`, `a21s-et5xx`, `a21s-vibrator`, `a21s-chub` at boot. Accurate. |
| 8 | `rootfs-overlay/etc/systemd/logind.conf.d/10-a21s-powerkey.conf` | `BOOT`, `SEC` (`direct-read`) | Verified (no issues) | 3 lines (`HandlePowerKey=ignore`, `HandlePowerKeyLongPress=poweroff`). Accurate. |
| 9 | `rootfs-overlay/etc/systemd/system/a21s-drm-keeper.service` | `BOOT`, `SEC`, `SVC` (`direct-read`) | Verified (`SEC-006`) | Runs `/usr/local/bin/a21s-drm-keeper` as root with no systemd sandboxing. Accurate. |
| 10 | `rootfs-overlay/usr/local/bin/a21s_holdmenu.py` | `SVC` (`direct-read`) | Verified (no issues) | 48-line pure state machine (`HoldMenu`), no I/O, no subprocesses. Clean. |
| 11 | `rootfs-overlay/usr/local/bin/rabbiz-osk-ctl` | `QUAL`, `SVC` (`direct-read`) | Verified (`QUAL-005`/`SVC-011`) | Confirmed `#!/bin/sh` with non-POSIX `[ -O "$RD" ]` on line 13; `gdbus`/`gsettings` args are all static. Accurate. |
| 12 | `scripts/de_apps_deploy.sh` | `BM`, `OTA`, `QUAL` (`direct-read`) | Verified (`BM-002`/`BM-010`) | Confirmed predictable `/tmp/rabbiz-apps-bin.$$` root staging LPE and dead case pattern on line 108. Accurate. |
| 13 | `scripts/mirror_setup.sh` | `BM`, `OTA`, `PRIV`, `SC` (`direct-read`) | Verified (`BM-001`, `PRIV-001`) | **Additional defect found:** Line 65 runs `cat > /home/rabbiz/.config/systemd/user/a21s-wayvnc.service` as `root` inside `rabbiz`-owned directory (symlink-follow root overwrite + leaves unit root-owned; noted under `BM-002`). |
| 14 | `scripts/setup_dumb_destroy_fix.sh` | `OTA` (`direct-read`) | Verified (`OTA-004` -> `BM-002`) | Confirmed predictable `/var/tmp/dumb-destroy.c` compiled as root into `/usr/local/lib/a21s-qt-scanout/dumb-destroy.so`. Accurate. |
| 15 | `shell/Rabbiz/Services/Bluetooth.qml` | `APP` (`targeted-read`) | Verified (minor note) | Properly uses `_shq()` single-quote escaping on `mac` (`lines 43-45, 62-80`). Note: `scanProc`/`listProc` lines 377/394 use unquoted `for m in $(...)` over `bluetoothctl` MAC column (bounded by BlueZ MAC format). Accurate. |
| 16 | `socs/exynos850/drivers/fingerprint/a21s_et5xx.c` | `KERN`, `SEC` (`direct-read`) | Verified (`SEC-002`/`KERN-003`) | Confirmed zero biometric verification and unprivileged `esfp_write` / `trigger_unlock_store` event injection. Accurate. |
| 17 | `socs/exynos850/drivers/tmu/exynos850-tmu.c` | `KERN` (`direct-read`) | Verified (positive) | Clean defensive driver: checks `resource_size`, validates `code_85 > code_25`, read-only `0444` hwmon. Accurate. |
| 18 | `tools/cbd/a21s-cbd.c` | `SVC`, `SC` (`direct-read`) | Verified (`SVC-010`) | Confirmed `ret != -EOPNOTSUPP` bug (lines 358, 377), silent timeout exit 0 (lines 380–395), unchecked `lseek`/`--chunk`/TOC CRC. Accurate. |
| 19 | `tools/rabbiz-apps/crates/rabbiz-app-kit/src/mcp.rs` | `APP` (`direct-read`) | Verified (minor note) | Clean stdio JSON-RPC 2.0 loop. Minor missed note: line 50 uses unbounded `stdin.lock().lines()` with no `MAX_MESSAGE_BYTES` cap (unlike `de-config/src/ipc.rs`). |
| 20 | `tools/rabbiz-de/crates/de-config/src/ipc.rs` | `QUAL`, `SVC` (`direct-read`) | Verified (positive) | Clean 16 KiB `MAX_MESSAGE_BYTES` cap on `take()` & decode; refuses `/tmp` fallback when `XDG_RUNTIME_DIR` unset. Accurate. |
| 21 | `tools/rabbiz-de/crates/de-core/src/runtime/composite.rs` | `PRIV` (`targeted-read`) | **MISSED 2 HIGH BUGS!** | **NEW `CHAL-001` (High):** `i32` integer overflow in `read_shm()` (`lines 470-491`) → `out.get_unchecked_mut(d)` heap buffer overflow from malicious `wl_shm` client.<br>**NEW `CHAL-002` (High):** `capture_output()` (`lines 348-360`) checks `de.nav.current == Screen::Lock`, which is **never set** by `power_lock()`/`lock.open()`, making lock-screen capture blanking completely dead! (Directly refutes `PRIV` Positive Observation #3.) |
| 22 | `tools/rabbiz-osk/crates/osk-wayland/src/feedback.rs` | `QUAL` (`rg-scan`) | Verified (minor note) | Confirmed hardcoded `/sys/kernel/a21s_vib/duration_ms` (`QUAL-009`). Minor missed note: lines 118 & 142/189–190 fall back to predictable `/tmp/rabbiz-de-prefs.json` and `/tmp/rabbiz-osk-click*.wav` when XDG env vars are unset. |
| 23 | `tools/rabbiz-sdk/Cargo.toml` | `BM`, `SC` (`direct-read`) | Verified (`BM-007`) | Confirmed `unsafe_code = "warn"`, zero git/wildcard deps, and missing `[profile.release]`. Accurate. |
| 24 | `tools/rabbiz-sdk/crates/rabbiz-flutter-runner/src/lib.rs` | `APP` (`direct-read`) | Verified (minor note) | Clean bundle validator + `dlopen` probe (`RTLD_LAZY \| RTLD_LOCAL`). Minor note: lines 185–186 `Box::leak` `app_id`/`title` per `FlutterHostApp::new`. |
| 25 | `tools/rabbiz-sdk/crates/rabbiz-sdk-c/src/lib.rs` | `APP` (`skipped`); `QUAL` (`targeted-read` L1–120) | **Audited in full (`CHAL-003`)** | Explained ledger discrepancy: `APP` marked it `skipped`, while `QUAL` marked it `reviewed` after reading only lines 1–120 (`QUAL-010`). Full review of all 725 lines / 31 `unsafe` items found **NEW `CHAL-003` (Low)**: unchecked `width * height * 4` & unasserted `px.data.len() >= needed` before `ptr::copy_nonoverlapping` in `rabbiz_app_render_headless`, missing `catch_unwind` across `extern "C"`, and `Box::leak` in `rabbiz_app_run`. |

---

## 5. Newly Discovered Findings (Phase 4 Spot-Check)

### CHAL-001
```
ID: CHAL-001
Title: Integer Overflow in Compositor wl_shm Capture (read_shm) Leads to Unchecked Heap Buffer Overflow
Severity: High
Confidence: Confirmed
Location: tools/rabbiz-de/crates/de-core/src/runtime/composite.rs:458-494
Evidence:
  470:     let (w, h) = (data.width, data.height);
  471:     if w <= 0 || h <= 0 || data.offset < 0 || data.stride < w * 4 {
  472:         return None;
  473:     }
  474:     let need = data.offset as usize + (h as usize - 1) * data.stride as usize + w as usize * 4;
  475:     if need > len {
  476:         return None;
  477:     }
  478:     let mut out = vec![0u8; (w * h * 4) as usize];
  479:     for y in 0..h {
  480:         let row = (data.offset + y * data.stride) as usize;
  481:         for x in 0..w {
  482:             let s = row + x as usize * 4;
  483:             let d = (y * w + x) as usize * 4;
  484:             unsafe {
  485:                 *out.get_unchecked_mut(d) = *ptr.add(s + r);
  486:                 *out.get_unchecked_mut(d + 1) = *ptr.add(s + g);
  487:                 *out.get_unchecked_mut(d + 2) = *ptr.add(s + b);
  488:                 *out.get_unchecked_mut(d + 3) = if a == NO_ALPHA { 255 } else { *ptr.add(s + a) };
  489:             }
  490:         }
  491:     }
Impact:
  In release builds (`overflow-checks = false`, verified in BM-007), `w * 4` (line 471), `w * h * 4` (line 478), `data.offset + y * data.stride` (line 480), and `y * w + x` (line 483) are all evaluated in signed 32-bit `i32` arithmetic. A malicious Wayland client can attach a sparse `memfd`-backed `wl_shm` buffer (requiring zero physical RAM via `ftruncate` on 64-bit Linux) with `w = 32768, h = 32768` (or `w = 0x4000_0000` so `w * 4` wraps to `0`), causing `(w * h * 4) as usize` to wrap to `0` (allocating a 0-byte `Vec`) while `unsafe { *out.get_unchecked_mut(d) = ... }` writes attacker-controlled pixel data out of bounds on the compositor heap (memory corruption / DoS / potential RCE in `rabbiz-de`).
Exploit/Trigger:
  Any unprivileged Wayland client commits a crafted `wl_shm` buffer and triggers a CPU composite (`wlr-screencopy`, `wayvnc`, `ctl_screenshot`, or morph snapshot).
Recommendation:
  1. Perform all dimension, stride, and byte-offset calculations in `usize` using `checked_mul` and `checked_add`, and enforce an explicit upper bound on `w` and `h` (e.g., `<= 16384`).
  2. Replace `out.get_unchecked_mut(...)` with safe slice indexing (e.g., `out[d..d + 4]` or `chunks_exact_mut(4)`), and copy from a bounds-checked `std::slice::from_raw_parts(ptr, len)`.
References: CWE-190 (Integer Overflow or Wraparound), CWE-787 (Out-of-bounds Write)
```

### CHAL-002
```
ID: CHAL-002
Title: Lock-Screen Capture Blanking (capture_output) Is Dead Code Because de.nav.current Is Never Set to Screen::Lock
Severity: High
Confidence: Confirmed
Location: tools/rabbiz-de/crates/de-core/src/runtime/composite.rs:348-360; tools/rabbiz-de/crates/de-core/src/runtime/power.rs:495-520
Evidence:
  // tools/rabbiz-de/crates/de-core/src/runtime/composite.rs:348-360
  pub fn capture_is_blank(screen: de_ui::nav::Screen) -> bool {
      screen == de_ui::nav::Screen::Lock
  }

  pub fn capture_output(de: &De) -> Option<Pixels> {
      let full = composite_output(de)?;
      if capture_is_blank(de.nav.current) {
          return Some(Canvas::filled(full.size, CLEAR).into_pixels());
      }
      Some(full)
  }

  // tools/rabbiz-de/crates/de-core/src/runtime/power.rs:508-511
  pub(crate) fn power_lock(&mut self) {
      self.lock.open();
      self.needs_present = true;
  }
  // (Verified via rg across tools/rabbiz-de/crates/: `nav.lock()` has ZERO non-test callers;
  //  power_lock(), power_fade(), power_wake(), state.rs:813, and session_lock.rs:126 all call
  //  `self.lock.open()` without setting `self.nav.current = Screen::Lock`.)
Impact:
  `capture_is_blank(de.nav.current)` always evaluates to `false` during actual device lock states. External capture clients (`wlr-screencopy` / `wayvnc` — see PRIV-001) receive live composited frames of the lock screen, lock-screen notifications, PIN bouncer entry, and (during swipe-up drag when `slide_px() > f32::EPSILON`) the underlying unlocked desktop/app behind the translucent lock overlay, completely defeating the documented lock-screen capture security boundary. (This directly refutes PRIV Positive Observation #3.)
Exploit/Trigger:
  Lock the device normally (Power button, idle timeout, `ctl lock`, or `ext-session-lock-v1`) and capture frames via `wlr-screencopy` (e.g., `wayvnc` or `grim`).
Recommendation:
  Change `capture_output(de)` to check `de.locked() || de.is_session_locked() || capture_is_blank(de.nav.current)` BEFORE calling `composite_output(de)` (avoiding unnecessary compositing and guaranteeing a solid blank frame whenever Keyguard or `ext-session-lock-v1` is active). Add an integration test exercising `power_lock()` -> `capture_output()`.
References: CWE-200 (Exposure of Sensitive Information), CWE-693 (Protection Mechanism Failure)
```

### CHAL-003
```
ID: CHAL-003
Title: C FFI ABI Safety Gaps in rabbiz-sdk-c (Unchecked Headless Buffer Arithmetic, Missing catch_unwind, Box::leak)
Severity: Low
Confidence: Confirmed
Location: tools/rabbiz-sdk/crates/rabbiz-sdk-c/src/lib.rs:512-513, 530-559
Evidence:
  // lib.rs:512-513
  let leaked_id: &'static str = Box::leak(app_id_str.to_string().into_boxed_str());
  let leaked_title: &'static str = Box::leak(title_str.to_string().into_boxed_str());

  // lib.rs:537-558
  if cb.is_null() || width <= 0 || height <= 0 {
      return 0;
  }
  let needed = (width as usize) * (height as usize) * 4;
  ...
  let px = c_canvas.inner.into_pixels();
  if !out_rgba.is_null() && cap >= needed {
      unsafe {
          ptr::copy_nonoverlapping(px.data.as_ptr(), out_rgba, needed);
      }
  }
Impact:
  1. `rabbiz_app_render_headless()` computes `needed = (width as usize) * (height as usize) * 4` without checked arithmetic or an upper dimension bound (allowing multi-GiB OOM or overflow on 32-bit targets), and passes `needed` directly to `ptr::copy_nonoverlapping` without asserting `px.data.len() >= needed`.
  2. None of the `pub unsafe extern "C" fn` entry points wrap Rust calls in `std::panic::catch_unwind`; any panic inside `rabbiz_app_kit` / `de_paint` aborts the host C/C++/Dart process.
  3. `rabbiz_app_run()` leaks heap strings via `Box::leak` on each call.
Exploit/Trigger:
  C/Dart SDK consumer passing oversized dimensions to `rabbiz_app_render_headless()` or triggering a panic inside a canvas/theme call.
Recommendation:
  Cap `width`/`height` (e.g., `<= 8192`) using `checked_mul`, assert `px.data.len() >= needed` (or copy `px.data.len().min(needed)`), and wrap `extern "C"` bodies in `std::panic::catch_unwind`.
References: CWE-190 (Integer Overflow or Wraparound), CWE-248 (Uncaught Exception)
```
