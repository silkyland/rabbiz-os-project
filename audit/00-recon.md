# Phase 0: Reconnaissance & Architecture Baseline

- **Project**: RabbizOS (`silkyland/android-arm-linux` / `rabbiz-os-project`)
- **Audit Date**: 2026-11-23
- **Git HEAD**: `c620c1d` (`main`, fast-forwarded from `9a908d7`)
- **Working Tree State**: Dirty (local uncommitted edits in `tools/rabbiz-apps/`, `rootfs-overlay/usr/local/bin/rabbiz-splash`, `scripts/check.sh`, `scripts/verify_mirror.sh`, `tools/rabbiz-de/crates/de-apps/data/launch.toml`, plus untracked `tools/rabbiz-apps/crates/rabbiz-search/`, `tools/rabbiz-mirror/linux/`, `scripts/run_mirror_linux.sh`, `tests/test_rabbiz_splash.py`). Audit covers tracked source files and notes dirty/untracked files explicitly where inspected.
- **Mode**: Strict READ-ONLY on source repository; outputs written exclusively under `audit/`.

---

## 1. Repository Inventory

### 1.1 Tracked Files vs. Disk Footprint

| Top-level Path | Tracked Files | Disk Files (incl. ignored) | Disk Size | Notes |
|---|---:|---:|---:|---|
| `tools/` | 515 | 44,937 | 66G | 5 Rust workspaces + SwiftPM + C utilities; `target/` & `.build/` gitignored |
| `socs/` | 381 | 381 | 7.9M | Exynos 850 out-of-tree kernel drivers (`abox`, `acpm`, `cpif`, `fingerprint`, `s2mu106`, `scsc`, `sensorhub`, `tmu`, `vibrator`, `vendor-include`) |
| `devices/` | 97 | 97 | 2.9M | Device configs, kernel defconfig/patches, findings (`samsung/a21s`, `xiaomi/muyu`, `_template`) |
| `rootfs-overlay/` | 89 | 68* | 988K | Systemd units, polkit/udev rules, `/usr/local/bin/*`, `.desktop` files, icons, XDG configs (*tracked count includes symlinks/deep paths) |
| `shell/` | 79 | 79 | 2.6M | Legacy/hybrid QML shell (`Rabbiz/`), Node.js unit tests |
| `docs/` | 58 | 59 | 3.4M | Architecture, plans, IPC/keyguard/protocol specs |
| `assets/` | 55 | 55 | 17M | Branding artwork, icons |
| `scripts/` | 51 | 54 | 296K | Bootstrap, build, deploy, device queue, mirror, release scripts |
| `findings/` | 7 | 7 | 32K | OS-wide engineering findings |
| `.github/` | 6 | 6 | <64K | CI workflows (`host-checks.yml`, templates) |
| `(root files)` | 6 | 6 | ~56K | `AGENTS.md`, `CONTRIBUTING.md`, `LICENSE`, `README.md`, `ROADMAP.md`, `.gitignore` |
| `tests/` | 3 | 4 | 20K | Python tests (`test_rabbiz_splash.py` untracked) |
| `patches/` | 3 | 3 | 12K | Top-level patches |
| `src/` *(gitignored)* | 0 | 72,753 | 2.8G | Cloned upstream `samsungexynos850/linux` & `uniLoader` |
| `build/` *(gitignored)* | 0 | 30,937 | 3.9G | Kernel/bootloader build output |
| `backups/` *(gitignored)* | 0 | 7 | 7.7G | Device stock/partition backups (excluded from audit) |
| `drivers/` *(gitignored)* | 0 | 385 | 21M | Local ignored directory (superseded by `socs/exynos850/drivers/`) |
| **Total Tracked** | **1,350** | — | — | **No `.gitmodules` (0 submodules)** |

### 1.2 Languages (Tracked Files)
- **Rust**: 318 `.rs`, 57 `Cargo.toml`, 5 `Cargo.lock` (`rabbiz-de`, `rabbiz-apps`, `rabbiz-osk`, `rabbiz-sdk`, `rabbiz-camera`)
- **C / Headers**: 163 `.c`, 207 `.h` (`socs/exynos850/drivers/*`, `tools/cbd`, `tools/a21s-et520-id-probe`, `tools/cma-leak`, `tools/rabbiz-sdk/crates/rabbiz-sdk-c`)
- **QML / JS / MJS**: 53 `.qml`, 6 `.js`, 11 `.mjs` (`shell/`, `scripts/`)
- **Shell / Python**: 42 `.sh`, 18 `.py`, plus extensionless scripts in `rootfs-overlay/usr/local/bin/`
- **Swift**: 13 `.swift`, 1 `Package.swift` (`tools/rabbiz-mirror`)
- **Config / Policy / Docs**: 135 `.md`, 17 `.desktop`, 15 `.patch`, 13 `.service`, 8 `.conf`, 6 `.rules` (polkit/udev), 5 `.json`, 4 `.yml`, 3 `.yaml`, 1 `.wit`

### 1.3 Build Systems Observed
> **Fact**: This is **NOT** an AOSP/Soong/Bazel/Yocto/repo-manifest build.
- **OS Base**: Bare-metal **Arch Linux ARM64** + Mainline Linux (`7.3.0`, `samsungexynos850/linux`) + `uniLoader` bootloader (`scripts/bootstrap_build_env.sh`).
- **Kernel & Out-of-tree Modules**: Linux Kbuild / `make ARCH=arm64 LLVM=1` (`devices/samsung/a21s/kernel/a21s_linux_defconfig`, `devices/samsung/a21s/kernel/patches/*.patch`, `socs/exynos850/drivers/*/Kbuild` & `Makefile`).
- **Desktop Environment & Apps**: Cargo workspaces (`tools/rabbiz-de`, `tools/rabbiz-apps`, `tools/rabbiz-osk`, `tools/rabbiz-sdk`, `tools/rabbiz-camera`).
- **Companion App**: SwiftPM (`tools/rabbiz-mirror/Package.swift`) + untracked Linux Rust client (`tools/rabbiz-mirror/linux/`).
- **CI**: GitHub Actions (`.github/workflows/host-checks.yml`).

### 1.4 Submodules, Vendored Code & Tracked Binary Blobs
- **Git Submodules**: None (`.gitmodules` does not exist).
- **Vendored / Ported Code**:
  - `socs/exynos850/drivers/` — Samsung/Exynos vendor-derived Linux drivers ported to mainline 7.3.0 (`scsc`, `abox`, `cpif`, `s2mu106`, `acpm`, `sensorhub`, `fingerprint`, `tmu`, `vibrator`, `vendor-include`).
  - `tools/cbd/` — Modem boot daemon (C).
- **Tracked Non-Image Binary Blobs**:
  - **4 pre-compiled compressed kernel modules (`.ko.zst`)**:
    - `socs/exynos850/drivers/scsc/scsc_bt.ko.zst`
    - `socs/exynos850/drivers/scsc/scsc_mx.ko.zst`
    - `socs/exynos850/drivers/scsc/scsc_platform_mif.ko.zst`
    - `socs/exynos850/drivers/scsc/scsc_wlan.ko.zst`
  - **Fonts**: 7 `.ttf`/`.otf` files (`MaterialIcons*`, `NotoSans*`).
  - **Icons/Images**: 2 `.icns`, 130 `.png`, 31 `.svg`.

---

## 2. Real Architecture Summary

```
Hardware (Samsung Galaxy A21s SM-A217F / Exynos 850; Phase 2: Xiaomi Pad 7 Pro / muyu)
  └─ Bootloader: Samsung stock PBL/SBL -> uniLoader (replaces Android boot.img)
       └─ Mainline Linux 7.3.0 (a21s_linux_defconfig + 13 DTS/driver patches)
            ├─ Out-of-tree Exynos 850 kernel modules (socs/exynos850/drivers/*)
            └─ Arch Linux ARM64 Rootfs (rootfs-overlay/)
                 ├─ Systemd services (rabbiz-session, rabbiz-auth-helper, rabbiz-splash,
                 │   a21s-drm-keeper, a21s-modem-init, abox-audio, s2mu106-charger,
                 │   volume-keys, fex-guestos, getty@tty1)
                 ├─ Security boundaries: Polkit rules, udev rules, Wayland privileged
                 │   protocol allow-list, Keyguard (Argon2id + rabbiz-auth-helper)
                 ├─ Wayland Compositor: rabbiz-de (Rust/Smithay-style, DRM/KMS,
                 │   Desktop Mode, XWayland, CtlIpc, lock_ui, StatusNotifierWatcher)
                 ├─ Userland Apps: rabbiz-apps (14 Rust Wayland apps + rabbiz-app-kit),
                 │   rabbiz-osk (Rust OSK), rabbiz-camera, legacy QML shell/
                 └─ Developer/Host Tools: rabbiz-sdk (CLI, emu-proto, emu-viewer,
                     emu-vnc, pack, wasm-runner, flutter-runner), rabbiz-mirror (Swift)
```

---

## 3. Adapted 10-Domain Audit Checklist

All 10 requested domains are retained and tailored to RabbizOS's real architecture:

| # | Domain | Adapted Scope for RabbizOS |
|---|---|---|
| 1 | `build-manifest` | `scripts/bootstrap_build_env.sh`, `scripts/check.sh`, `scripts/de_deploy.sh`, `.github/workflows/host-checks.yml`, Cargo workspace profiles/flags, Kbuild/Makefile hardening flags, revision pinning (`samsungexynos850/linux`, `uniLoader`, toolchains). |
| 2 | `boot-chain` | `uniLoader` bootstrap/flashing scripts, `a21s` boot image creation, `getty@tty1.service`, `rabbiz-splash.service`, kexec/reboot paths, presence/absence of Verified Boot / dm-verity / image signing / key management. |
| 3 | `partition-ota` | Partition layout/hiding (`99-hide-system-partitions.rules`), `backup_muyu_stock.sh`, `de_deploy.sh`, `rabbiz-store` update/install paths, presence/absence of A/B partitioning, rollback protection, signed OTA & TLS verification. |
| 4 | `kernel` | `devices/samsung/a21s/kernel/a21s_linux_defconfig` hardening (KASLR, STACKPROTECTOR, FORTIFY_SOURCE, STRICT_KERNEL_RWX, LOCKDOWN, BPF, etc.), 13 kernel patches, `patches/`, and out-of-tree drivers in `socs/exynos850/drivers/` (`ioctl`, `copy_from_user`, sysfs/debugfs permissions, memory safety, `.ko.zst` blobs). |
| 5 | `security-policy` | `rootfs-overlay/etc/polkit-1/rules.d/*`, `rootfs-overlay/etc/udev/rules.d/*`, `rabbiz-de` privileged Wayland protocol gating & keyguard (`de-config/privileged.rs`, `de-system/auth.rs`, `de-core/protocols/privileged.rs`), `rabbiz-auth-helper`, systemd service sandboxing, SELinux/AppArmor/seccomp status. |
| 6 | `system-services` | All 13 systemd services, `rootfs-overlay/usr/local/bin/*` daemons/helpers, `tools/cbd/`, `rabbiz-de` `CtlIpc` & `auth_helper` Unix/TCP sockets, file/socket permissions, input validation, root-running scripts (`volume-keys.py`, `a21s-modem-ctl`, `rabbiz-wifi-connect`, `rabbiz-splash`). |
| 7 | `framework-apps` | `rabbiz-store` (package installation/privilege model), `rabbiz-settings` (system actions), `rabbiz-dialer`/`rabbiz-sms`/`rabbiz-files`/`rabbiz-terminal`/`rabbiz-app-kit`, `rabbiz-sdk` (`rabbiz-pack`, `rabbiz-wasm-runner`, `rabbiz-emu-vnc`), `rabbiz-mirror`, `shell/` QML/JS. |
| 8 | `supply-chain` | Cargo.toml / Cargo.lock pinning & git/path deps, `Package.swift`, tracked `.ko.zst` binary blobs, vendored Samsung driver provenance & license (`LICENSE` vs GPLv2/proprietary), SBOM presence, unpinned curl/pip/pacman/git installs in scripts/CI. |
| 9 | `secrets-privacy` | Hardcoded credentials/tokens/keys/IPs/MACs/serials/IMEIs across repo, `~/.credentail` handling, `a21s-telemetry`, `rabbiz-mirror` & `rabbiz-emu-vnc` unauthenticated streams, `scripts/release/public_history.sh`, debug logs/findings leaking device data. |
| 10 | `quality-maintainability` | CI coverage & test gaps, dead/legacy code (`shell/` vs `rabbiz-de`, `drivers/` vs `socs/`), `TODO`/`FIXME`/`HACK`/`unsafe` density, shell script quality (`shellcheck`), naming/structure consistency. |

---

## 4. Phase 3 Tool Availability

| Tool | Status | Phase 3 Strategy |
|---|---|---|
| `shellcheck` | **AVAILABLE** (`/usr/bin/shellcheck`) | Run on `scripts/*.sh` and shell scripts in `rootfs-overlay/usr/local/bin/` |
| `clang-tidy` | **AVAILABLE** (`/usr/bin/clang-tidy`) | Available for targeted C checks (note: no compile_commands.json without building) |
| `rg`, `python3`, `node`, `jq`, `cargo` | **AVAILABLE** | Custom read-only static scanners for secrets, kernel defconfig hardening checks, Cargo.lock/dependency audit, `unsafe`/TODO inventory |
| `gitleaks`, `trufflehog` | `tool unavailable` | Compensate with regex/entropy secret scan via `rg` + Python script + manual review |
| `semgrep` | `tool unavailable` | Compensate with targeted `rg` pattern rules (Rust `unsafe`/Command/IPC, Python `subprocess`/`eval`, C `strcpy`/`copy_from_user`) + manual review |
| `cppcheck` | `tool unavailable` | Compensate with targeted C audit on `tools/cbd/` and `socs/exynos850/drivers/` |
| `syft`, `trivy`, `grype`, `cargo-audit` | `tool unavailable` | Compensate with `Cargo.lock` dependency inventory + version/source inspection (no network) |
| `kernel-hardening-checker` | `tool unavailable` | Compensate with Python Kconfig hardening checklist against `a21s_linux_defconfig` |
| `checksec` | `tool unavailable` | Compensate with `readelf`/`file`/`sha256sum` on tracked `.ko.zst` blobs (read-only) |
| `licensee`, `scancode` | `tool unavailable` | Compensate with SPDX / license header scan across `LICENSE`, `Cargo.toml`, and `socs/` headers |
| `bandit` | `tool unavailable` | Compensate with manual + `rg` audit of all `.py` files |
