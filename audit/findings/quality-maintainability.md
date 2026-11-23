# Domain Audit: Quality & Maintainability (`QUAL`)

## Scope & Files Reviewed

### Inspected Files
- **CI & Host Checks (direct-read):**
  - `.github/workflows/host-checks.yml`
  - `scripts/check.sh`
- **Repo Hygiene & Workspace Configs (direct-read):**
  - `.gitignore`
  - `tools/rabbiz-de/Cargo.toml`
  - `tools/rabbiz-apps/Cargo.toml`
- **Triage Tool Outputs & Verified Targets (direct-read / targeted-read):**
  - `audit/tools/shellcheck.txt`
  - `audit/tools/code-patterns.txt`
  - `scripts/de_apps_deploy.sh` (lines 95–130)
  - `rootfs-overlay/usr/local/bin/rabbiz-osk-ctl`
  - `rootfs-overlay/usr/local/bin/rabbiz-labwc-session` (lines 1–70)
  - `scripts/install_fex_rootfs.sh`
  - `scripts/mirror_setup.sh` (lines 50–100)
  - `scripts/audio-up.sh` (lines 1–35)
  - `tools/rabbiz-sdk/crates/rabbiz-sdk-c/src/lib.rs` (lines 1–120)
  - `tools/rabbiz-apps/crates/rabbiz-dialer/src/model.rs` (targeted `rg` on `unsafe`)
  - `tools/rabbiz-apps/crates/rabbiz-files/src/model.rs` (targeted `rg` on `unsafe`)
  - `shell/Rabbiz/Windowing/LaunchFlow.js` (targeted `rg` on `TODO`)
- **Documentation & Code Cross-Checks (direct-read / targeted-read):**
  - `README.md` (lines 1–120)
  - `ROADMAP.md`
  - `CONTRIBUTING.md`
  - `AGENTS.md` (loaded via project rules)
  - `docs/DE-IPC.md`
  - `docs/DE-KEYGUARD-AUTH.md`
  - `docs/DE-PRIVILEGED-PROTOCOLS.md`
  - `tools/rabbiz-de/crates/de-config/src/privileged.rs` (lines 1–170)
  - `tools/rabbiz-de/crates/de-config/src/ipc.rs` (lines 1–120)
  - `tools/rabbiz-de/crates/de-core/src/runtime/ctl_ipc.rs` (lines 1–90)
- **Repository-Wide Scans (read-only `git`, `rg`, `diff`, `ls`):**
  - `git status --short`, `git ls-files` (binary artifacts, `tools/rabbiz-de/vendor/`)
  - Duplicate check between `scripts/` and `rootfs-overlay/` (`volume-keys.py`, `volume-keys.service`)
  - `#[test]` count across all 6 Rust workspaces (`rabbiz-de`, `rabbiz-apps`, `rabbiz-osk`, `rabbiz-sdk`, `rabbiz-camera`, `rabbiz-mirror/linux`)
  - `rg` scans for `gamer`, `a21s`, legacy `shell/`/`quickshell` references, diagnostic tools, and `TODO`/`FIXME`/`HACK`/`XXX` markers.

### Not Inspected (and Reason)
- Individual QML files under `shell/` (79 files) and individual Rust app source files under `tools/rabbiz-apps/crates/` (beyond targeted `unsafe`/`a21s` scans) — out of scope for line-by-line review in Domain 10; functional/security logic is covered by dedicated DE/App domains.
- Kernel C driver implementations under `socs/exynos850/drivers/` — only scanned for tracked `.ko.zst` artifacts and `TODO`/`FIXME`/`HACK` counts; C security/correctness is covered by the Kernel/Drivers domain.
- `tools/rabbiz-mirror/` Swift macOS sources — not inspected line-by-line; only checked workspace structure and test coverage presence.
- Runtime execution of `cargo test`, `cargo clippy`, `scripts/check.sh`, or `shellcheck` — forbidden by read-only audit rules (no builds/test execution).

---

## Findings

### QUAL-001
- **ID:** QUAL-001
- **Title:** Clean checkout and GitHub Actions CI for `tools/rabbiz-de` are broken by gitignored `vendor/smithay` patch
- **Severity:** High
- **Confidence:** Confirmed
- **Location:** `tools/rabbiz-de/Cargo.toml:88-89`, `.gitignore:6`, `.github/workflows/host-checks.yml:24-58`
- **Evidence:**
  ```toml
  # tools/rabbiz-de/Cargo.toml:88-89
  [patch.crates-io]
  smithay = { path = "vendor/smithay" }
  ```
  ```gitignore
  # .gitignore:6
  tools/rabbiz-de/vendor/
  ```
  Command verification:
  ```text
  $ git ls-files tools/rabbiz-de/vendor/ | wc -l
  0
  $ ls -ld tools/rabbiz-de/vendor/smithay
  drwxr-xr-x 1 dit dit 546 พ.ย. 21 12:36 tools/rabbiz-de/vendor/smithay
  ```
  `.github/workflows/host-checks.yml:24-58` runs `actions/checkout@v4` followed directly by `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` in `tools/rabbiz-de` with no step to populate `vendor/smithay`.
- **Impact:** Any fresh clone of the repository and every GitHub Actions CI run of the `rabbiz-de` job will fail immediately when Cargo attempts to read `tools/rabbiz-de/vendor/smithay/Cargo.toml`. Local builds only succeed because an untracked/ignored copy exists on the developer's machine.
- **Exploit/Trigger:** Clone the repository cleanly (or trigger `.github/workflows/host-checks.yml` on any PR/push) and run `cargo test --workspace` in `tools/rabbiz-de`.
- **Recommendation:** Either (a) track `tools/rabbiz-de/vendor/smithay` (or the minimal patched crate / a git submodule / a pinned upstream git rev in `[patch.crates-io]`) in git and remove `tools/rabbiz-de/vendor/` from `.gitignore`, or (b) point `[patch.crates-io]` to a pinned git commit containing the upstream fix (`smithay = { git = "...", rev = "..." }`) so Cargo fetches it reproducibly in CI and fresh clones.
- **References:** CWE-1357 (Reliance on Insufficiently Trustworthy Component / Reproducible Build Failure)

---

### QUAL-002
- **ID:** QUAL-002
- **Title:** Major CI test and lint blind spots: 480+ Rust unit tests, `shellcheck`, and `rabbiz-camera` are excluded from CI
- **Severity:** Medium
- **Confidence:** Confirmed
- **Location:** `.github/workflows/host-checks.yml:13-58`, `scripts/check.sh:2,18,29-37`
- **Evidence:**
  Count of existing `#[test]` functions per Rust workspace:
  ```text
  tools/rabbiz-de:           442 #[test] fns  (TESTED in CI)
  tools/rabbiz-apps:         390 #[test] fns  (NOT in CI; local check.sh only)
  tools/rabbiz-osk:           61 #[test] fns  (NOT in CI; local check.sh only)
  tools/rabbiz-sdk:           24 #[test] fns  (NOT in CI; local check.sh only)
  tools/rabbiz-camera:         2 #[test] fns  (NOT in CI; NOT in check.sh)
  tools/rabbiz-mirror/linux:   3 #[test] fns  (untracked; NOT in CI; NOT in check.sh)
  ```
  `.github/workflows/host-checks.yml:19` also runs `tests/findings.test.mjs` twice (explicitly and via `tests/*.test.mjs`):
  ```yaml
  for t in tests/findings.test.mjs tests/*.test.mjs shell/tests/*.test.mjs; do node "$t"; done
  ```
  And omits `tests/test_rabbiz_splash.py` (which is present in `scripts/check.sh:18`), `shellcheck` across 54 shell scripts (`scripts/` and `rootfs-overlay/`), `cargo fmt`/`cargo clippy` for all non-DE Rust workspaces, and C static checks/compilation for `socs/` drivers and `tools/cbd/a21s-cbd.c`.
  Meanwhile, `scripts/check.sh:2` contains a stale comment:
  ```bash
  # One command for every host-side check. No CI exists in this repo yet; run before committing.
  ```
- **Impact:** More than half of the repository's Rust unit tests (480 of 922 `#[test]` functions) and all shell scripts are unverified by CI. Regressions, formatting drift, and shell bugs in `rabbiz-apps`, `rabbiz-osk`, `rabbiz-sdk`, `rabbiz-camera`, and `rootfs-overlay/` can merge to `main` undetected.
- **Exploit/Trigger:** Any PR modifying `tools/rabbiz-apps/`, `tools/rabbiz-osk/`, `tools/rabbiz-sdk/`, `tools/rabbiz-camera/`, or `rootfs-overlay/` scripts passes GitHub Actions without running their tests or linters.
- **Recommendation:**
  1. Add CI jobs in `.github/workflows/host-checks.yml` for `rabbiz-apps`, `rabbiz-osk` (`--exclude osk-wayland` or with `libxkbcommon-dev` installed), `rabbiz-sdk`, and `rabbiz-camera` (`cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test --workspace`).
  2. Add `rabbiz-camera` to `scripts/check.sh` and update the stale line 2 comment.
  3. Add a `shellcheck` job in CI for `scripts/` and `rootfs-overlay/usr/local/bin/*`.
  4. Deduplicate `tests/findings.test.mjs` in `.github/workflows/host-checks.yml:19` and add `tests/test_rabbiz_splash.py` once committed.
- **References:** N/A

---

### QUAL-003
- **ID:** QUAL-003
- **Title:** Compiled binary kernel modules (`*.ko.zst`) tracked in git and `.gitignore` gaps
- **Severity:** Medium
- **Confidence:** Confirmed
- **Location:** `.gitignore:20-26,38-45`, `socs/exynos850/drivers/scsc/*.ko.zst`
- **Evidence:**
  ```gitignore
  # .gitignore:38-40
  *.o
  *.ko
  *.mod
  ```
  `*.ko.zst` is NOT ignored, and 4 compiled binary kernel modules (361 KiB total) are tracked in git:
  ```text
  $ git ls-files | grep '\.ko\.zst$'
  socs/exynos850/drivers/scsc/scsc_bt.ko.zst        (30K)
  socs/exynos850/drivers/scsc/scsc_mx.ko.zst       (64K)
  socs/exynos850/drivers/scsc/scsc_platform_mif.ko.zst (16K)
  socs/exynos850/drivers/scsc/scsc_wlan.ko.zst     (251K)
  ```
  Additionally, `.gitignore:11-26,50` explicitly lists `target/` ignores for `rabbiz-de`, `rabbiz-osk`, `rabbiz-apps`, `rabbiz-sdk`, and `rabbiz-mirror/linux`, but omits `tools/rabbiz-camera/target/`.
- **Impact:** Pre-built opaque binary kernel modules in git bloat repository history, cannot be code-reviewed in diffs, risk shipping stale/mismatched kernel ABI modules relative to tracked C sources, and lack provenance verification. Missing `tools/rabbiz-camera/target/` risks accidental commit of Rust build artifacts.
- **Exploit/Trigger:** Running `git add .` after building `rabbiz-camera` or compressing out-of-tree `.ko` modules with `zstd` will stage binary build outputs.
- **Recommendation:** Add `*.ko.zst` and `tools/rabbiz-camera/target/` (or a workspace-wide `tools/**/target/`) to `.gitignore`, and `git rm --cached` the four tracked `socs/exynos850/drivers/scsc/*.ko.zst` binaries (publish prebuilt modules via GitHub Releases if needed).
- **References:** CWE-494 (Download/Use of Code Without Integrity Check / Unverifiable Binary Artifacts)

---

### QUAL-004
- **ID:** QUAL-004
- **Title:** Large uncommitted/untracked working tree sprawl mixing multiple lanes
- **Severity:** Medium
- **Confidence:** Confirmed
- **Location:** Repository working tree (`git status --short`), `CONTRIBUTING.md:60`
- **Evidence:**
  `git status --short` shows **60 modified tracked files**, **1 deleted tracked file** (`tools/rabbiz-apps/crates/rabbiz-photos/src/decode.rs`), and **6 untracked paths** (excluding `audit/`):
  - Modified across multiple lanes: `.gitignore`, `rootfs-overlay/etc/systemd/system/rabbiz-splash.service`, `rootfs-overlay/usr/local/bin/rabbiz-splash`, `scripts/check.sh`, `scripts/verify_mirror.sh`, `tools/rabbiz-de/crates/de-apps/data/launch.toml`, and 54 files across `tools/rabbiz-apps/`.
  - Untracked: `scripts/run_mirror_linux.sh`, `tests/test_rabbiz_splash.py`, `tools/rabbiz-apps/crates/rabbiz-app-kit/src/preview.rs`, `tools/rabbiz-apps/crates/rabbiz-app-kit/src/raster_image.rs`, `tools/rabbiz-apps/crates/rabbiz-search/`, `tools/rabbiz-mirror/linux/`.
  - Note: `scripts/check.sh:18` already invokes the untracked `tests/test_rabbiz_splash.py`, and `tools/rabbiz-apps/Cargo.toml:18` already references the untracked `crates/rabbiz-search` member.
- **Impact:** Violates `CONTRIBUTING.md:60` ("One lane per PR. One concern per PR."), increases risk of partial commits breaking the workspace (e.g., committing `Cargo.toml` or `check.sh` without the untracked files they reference), and makes security/regression bisection difficult.
- **Exploit/Trigger:** Staging tracked changes (`git commit -a`) without untracked files would break `tools/rabbiz-apps` (missing `crates/rabbiz-search`) and `scripts/check.sh` (missing `tests/test_rabbiz_splash.py`).
- **Recommendation:** Split and commit the working tree cleanly by lane: (1) `userland` (`rabbiz-splash` + `tests/test_rabbiz_splash.py` + `scripts/check.sh`), (2) `de`/apps (`rabbiz-apps` refactor + `rabbiz-search` + `launch.toml`), (3) `mirror` (`tools/rabbiz-mirror/linux` + `scripts/run_mirror_linux.sh` + `scripts/verify_mirror.sh`).
- **References:** `CONTRIBUTING.md:60`

---

### QUAL-005
- **ID:** QUAL-005
- **Title:** Verified shell script defects and POSIX portability bugs (`shellcheck` triage)
- **Severity:** Medium
- **Confidence:** Confirmed
- **Location:** `scripts/de_apps_deploy.sh:108`, `rootfs-overlay/usr/local/bin/rabbiz-osk-ctl:1,13`, `rootfs-overlay/usr/local/bin/rabbiz-labwc-session:50`, `scripts/audio-up.sh:22`
- **Evidence:**
  Direct verification of findings from `audit/tools/shellcheck.txt` (54 scripts scanned):
  1. **Unreachable case branch (`SC2221`/`SC2222`)** — `scripts/de_apps_deploy.sh:108`:
     ```bash
     case "$desc" in
       *ELF*aarch64*|*ELF*ARM\ aarch64*) ;;
     ```
     `*ELF*aarch64*` already matches `*ELF*ARM aarch64*`, making the second pattern dead code.
  2. **Non-POSIX test operator in `#!/bin/sh` (`SC3067`)** — `rootfs-overlay/usr/local/bin/rabbiz-osk-ctl:1,13`:
     ```sh
     #!/bin/sh
     ...
     [ -O "$RD" ] || RD="/run/user/$(id -u)"
     ```
     `test -O` is undefined in POSIX `sh` (fails under `dash`/`ash`/`busybox sh`; only works when `/bin/sh` is `bash`).
  3. **Masked exit status under `set -euo pipefail` (`SC2155`)** — `rootfs-overlay/usr/local/bin/rabbiz-labwc-session:10,50`:
     ```bash
     set -euo pipefail
     ...
     export WLR_DRM_DEVICES="$(readlink -f /dev/dri/by-path/platform-exynos-drm-card)"
     ```
     `export VAR="$(...)"` masks the return code of `readlink -f`, bypassing `set -e` if resolution fails.
  4. **Unused loop variable (`SC2034`)** — `scripts/audio-up.sh:22`:
     ```sh
     for i in $(seq 1 20); do
     ```
  *(Triage note: `SC2087`/`SC2029` warnings in `scripts/install_fex_rootfs.sh:18` and `scripts/mirror_setup.sh:65` were inspected directly and confirmed intentional client-side expansions with inner variables properly escaped.)*
- **Impact:** `rabbiz-osk-ctl` breaks on POSIX `/bin/sh` shells; `rabbiz-labwc-session` silently exports an empty `WLR_DRM_DEVICES` if `readlink` fails; dead pattern in `de_apps_deploy.sh` obscures architecture-check intent.
- **Exploit/Trigger:** Running `rabbiz-osk-ctl` on a system where `/bin/sh` is `dash`, or a broken symlink at `/dev/dri/by-path/platform-exynos-drm-card` during session startup.
- **Recommendation:**
  - `scripts/de_apps_deploy.sh:108`: simplify to `*ELF*aarch64*) ;;`.
  - `rootfs-overlay/usr/local/bin/rabbiz-osk-ctl:1`: switch shebang to `#!/usr/bin/env bash` (or check ownership via `stat -c %u`).
  - `rootfs-overlay/usr/local/bin/rabbiz-labwc-session:50`: split into `drm_dev="$(readlink -f ...)"` and `export WLR_DRM_DEVICES="$drm_dev"`.
  - `scripts/audio-up.sh:22`: use `_` or a counter `while` loop.
- **References:** ShellCheck SC2221, SC2222, SC3067, SC2155, SC2034

---

### QUAL-006
- **ID:** QUAL-006
- **Title:** Legacy QML `shell/` tree (79 files) and one-off diagnostic tools remain in active tree post `rabbiz-de` cut-over
- **Severity:** Low
- **Confidence:** Confirmed
- **Location:** `shell/` (79 files), `rootfs-overlay/usr/local/bin/rabbiz-labwc-session`, `rootfs-overlay/usr/local/bin/rabbiz-shell`, `tools/{a21s-et520-id-probe,browser-bench,cma-leak,shade-preview}`
- **Evidence:**
  - `find shell -type f | wc -l` = **79 files** (QML `quickshell` implementation), still deployed by `scripts/deploy_shell.sh:122-125` and tested by `shell/tests/*.test.mjs` in CI, despite the cut-over to the native Rust compositor `rabbiz-de` (`AGENTS.md:3`).
  - `scripts/shell_reload.sh`, `scripts/verify_shell_blur.sh`, `scripts/shell_perf_compare.sh`, and `scripts/frame_time.sh:36` still target `quickshell -p /usr/share/rabbiz-shell/shell.qml`.
  - `tools/` mixes core production workspaces (`rabbiz-de`, `rabbiz-apps`, `rabbiz-osk`, `rabbiz-sdk`, `rabbiz-camera`, `rabbiz-mirror`, `cbd`) with unmaintained one-off diagnostic probes (`tools/a21s-et520-id-probe/et520-id-probe.c`, `tools/browser-bench/bench.html`, `tools/cma-leak/leaktest.c`, `tools/shade-preview/preview.py` for the legacy QML shade).
  - *(Note: `tools/qt-scanout/` is NOT dead — `dumb-destroy.c`, `kms-node.c`, and `patch_egl.py` are actively deployed by `scripts/setup_dumb_destroy_fix.sh` / `scripts/setup_qt_scanout.sh` and tested in CI.)*
- **Impact:** Increases cognitive load for contributors and AI agents, creates confusion about which shell is authoritative (e.g., `AGENTS.md:57-70` still gives detailed QML directory rules for `shell/Rabbiz/`), and consumes maintenance effort on deprecated surfaces.
- **Exploit/Trigger:** Contributors editing `shell/` expecting changes to appear in the `rabbiz-de` session.
- **Recommendation:** Either archive/remove `shell/` and QML-only scripts (`shade-preview`, `shell_reload.sh`, `verify_shell_blur.sh`) once the `labwc` fallback is retired, or explicitly move one-off probes under `tools/diagnostics/` (or `devices/samsung/a21s/probes/` for A21s-only probes like `a21s-et520-id-probe` and `cma-leak`) and mark `shell/` as `[LEGACY FALLBACK]` in `AGENTS.md`.
- **References:** `AGENTS.md:3,57-70`

---

### QUAL-007
- **ID:** QUAL-007
- **Title:** Byte-for-byte duplicated hardware key daemon and systemd unit in `scripts/` and `rootfs-overlay/`
- **Severity:** Low
- **Confidence:** Confirmed
- **Location:** `scripts/volume-keys.py`, `rootfs-overlay/usr/local/bin/volume-keys.py`, `scripts/volume-keys.service`, `rootfs-overlay/etc/systemd/system/volume-keys.service`
- **Evidence:**
  ```text
  $ diff -q scripts/volume-keys.py rootfs-overlay/usr/local/bin/volume-keys.py
  (exit 0 — IDENTICAL)
  $ diff -q scripts/volume-keys.service rootfs-overlay/etc/systemd/system/volume-keys.service
  (exit 0 — IDENTICAL)
  ```
  `tests/test_volume_keys.py:65` tests `rootfs-overlay/usr/local/bin/volume-keys.py`, and `scripts/deploy_shell.sh:178-179` deploys from `rootfs-overlay/`. Meanwhile, `devices/samsung/a21s/docs/a21s-audio-abox-plan.md:80` still points to `scripts/`.
- **Impact:** Two identical copies of the same runtime script and systemd unit invite silent divergence if one copy is patched without the other.
- **Exploit/Trigger:** Editing `scripts/volume-keys.py` has zero effect on deployed devices or `tests/test_volume_keys.py`.
- **Recommendation:** Remove the duplicate `scripts/volume-keys.py` and `scripts/volume-keys.service`, keeping `rootfs-overlay/` as the single source of truth, and update the reference in `devices/samsung/a21s/docs/a21s-audio-abox-plan.md:80`.
- **References:** N/A

---

### QUAL-008
- **ID:** QUAL-008
- **Title:** Documentation drift and internal contradictions across `README.md`, `ROADMAP.md`, `AGENTS.md`, `docs/DE-IPC.md`, and `docs/DE-KEYGUARD-AUTH.md`
- **Severity:** Low
- **Confidence:** Confirmed
- **Location:** `README.md:16,71,94,117`, `ROADMAP.md:3,18,51,62-66,77-92,105,111`, `AGENTS.md:5`, `docs/DE-IPC.md:4,24,35-43`, `docs/DE-KEYGUARD-AUTH.md:3,17,29,44`
- **Evidence:**
  1. **`README.md` app count contradiction:** Lines 16, 94, and 117 state **"12 native Rust mobile applications (`rabbiz-apps`)"**; line 71 states **"All 13 built-in apps"** (listing 13 apps in lines 75–87); `tools/rabbiz-apps/Cargo.toml:4-18` lists 14 app crates (plus `rabbiz-app-kit`, including untracked `rabbiz-search`), and `tools/rabbiz-camera/` is a separate 15th app.
  2. **`ROADMAP.md` stale architecture, internal contradiction, and device ID leak:**
     - Line 3 says `Last updated: 2026-11-15` despite 2026-11-18 updates on lines 62–66.
     - Lines 18 and 77–92 still describe M7 exclusively as the legacy `labwc` + `quickshell` QML shell, omitting `rabbiz-de`, `rabbiz-apps`, `rabbiz-sdk`, and `rabbiz-emu`.
     - Risks table (line 105) claims `No thermal throttling (/sys/class/thermal/ empty) under load`, contradicting line 66 (`three zones live with passive 80 °C CPU trips bound to cpufreq cooling devices + 95 °C criticals`).
     - Line 51 includes an unredacted hardware serial/ADB identifier (`muyu` (`[REDACTED]`)), contrary to `CONTRIBUTING.md:51` ("Never print, log, or commit secrets, Wi-Fi passwords, tokens, serial numbers, IMEIs, or MAC addresses of real hardware").
  3. **`AGENTS.md:5` stale SteamOS identity:** Describes the project as `Native Mainline Linux (7.3.0) + Arch Linux ARM64 + SteamOS (steam -gamepadui) port`, contradicting `ROADMAP.md:111` ("Steam / SteamOS support — fully uninstalled 2026-11-16... Deferred").
  4. **`docs/DE-KEYGUARD-AUTH.md` stale RFC status & IPC contradiction:**
     - Line 3 says `Status: Draft RFC (Concept — ready for owner review before implementation)` and line 17 says `there is no real credential gate yet`, yet `tools/rabbiz-de/crates/de-system/src/auth.rs` (33 KB) and `tools/rabbiz-de/crates/de-ui/src/screens/lock_bouncer.rs` (12.8 KB) are already implemented, and lines 42–49 discuss the live implementation (`Currently keyguard.json and keyguard.state live in ~/.config/rabbiz-shell/`).
     - Line 29 states `only GetState and Lock are allowed` while locked, contradicting `docs/DE-IPC.md:24` (`only get_state, lock, reload_theme, reload_apps, show_osd, and set_panel (open: false) are permitted`).
  5. **`docs/DE-IPC.md` incomplete command list:** Line 4 says `all 7 commands wired` and Section 3 (lines 35–43) lists 7 commands, whereas `tools/rabbiz-de/crates/de-config/src/ipc.rs:45-103` implements **17 commands** (10 undocumented in Section 3: `reload_apps`, `inject_touch`, `inject_key`, `inject_hw_button`, `set_mock`, `set_rotation`, `set_app_orientation`, `set_desktop_mode`, `set_immersive`, `set_virtual_output`).
- **Impact:** Misleads new contributors and automated agents about system status, security posture (Keyguard and IPC lock guards), and supported interfaces.
- **Exploit/Trigger:** Reading top-level docs or RFCs to plan work or audit behavior.
- **Recommendation:** Update `README.md` app counts, refresh `ROADMAP.md` (add `rabbiz-de`/`rabbiz-apps`/`rabbiz-sdk` milestones, fix Risks table, redact the `muyu` serial on line 51), update `AGENTS.md:5` to reflect Steam deferral, mark `docs/DE-KEYGUARD-AUTH.md` as Implemented (with remaining follow-ups) and align its locked-IPC list with `docs/DE-IPC.md`, and document all 17 `IpcCommand` variants in `docs/DE-IPC.md`.
- **References:** `CONTRIBUTING.md:51`

---

### QUAL-009
- **ID:** QUAL-009
- **Title:** `a21s-*` naming and hardware-specific paths hardcoded in shared cross-device code
- **Severity:** Low
- **Confidence:** Confirmed
- **Location:** `scripts/mirror_setup.sh:67,89`, `tools/rabbiz-osk/crates/osk-wayland/src/feedback.rs:20`, `tools/rabbiz-de/crates/de-system/src/fingerprint.rs:15`, `tools/rabbiz-de/crates/de-ui/src/interaction/gestures.rs:141,590`, `tools/rabbiz-apps/crates/rabbiz-store/src/main.rs:161,748`
- **Evidence:**
  `AGENTS.md:3` states: *"code that is shared across devices uses `rabbiz-*`; code tied to this phone's hardware ... keeps `a21s-*`."* `CONTRIBUTING.md:55` states: *"Device-specific rules live with the device. Do not copy them into `core` code as if they were universal."*
  Violations verified in shared code:
  - `scripts/mirror_setup.sh:67,89`:
    ```sh
    Description=A21s wayvnc screen mirror
    UNIT=a21s-wayvnc.service
    ```
  - `tools/rabbiz-osk/crates/osk-wayland/src/feedback.rs:20`:
    ```rust
    const VIB_DURATION_PATH: &str = "/sys/kernel/a21s_vib/duration_ms";
    ```
  - `tools/rabbiz-de/crates/de-system/src/fingerprint.rs:15`:
    ```rust
    const SYSFS: &str = "/sys/kernel/a21s_fp";
    ```
  - `tools/rabbiz-de/crates/de-ui/src/interaction/gestures.rs:141`:
    ```rust
    pub fn a21s() -> Self {
    ```
  - `tools/rabbiz-apps/crates/rabbiz-store/src/main.rs:161,748`:
    ```rust
    id: "linux-mainline-7.3.0-a21s-r2".into(),
    ...
    self.kernel_release = "7.3.0-3-rabbiz-a21s".into();
    ```
- **Impact:** Couples shared cross-device crates (`de-system`, `de-ui`, `osk-wayland`, `rabbiz-store`, `mirror_setup.sh`) to the Galaxy A21s, adding porting friction for Phase 2 (`muyu`) and future devices.
- **Exploit/Trigger:** Deploying `rabbiz-de`, `rabbiz-osk`, or `rabbiz-store` on a non-A21s device causes hardcoded `/sys/kernel/a21s_*` lookups or A21s-branded fallback update IDs.
- **Recommendation:** Parameterize sysfs paths (`a21s_vib`, `a21s_fp`) via device config or runtime probing (e.g., standard `ff-memless`/`leds` vibrator and `fprintd` D-Bus), rename `GestureConfig::a21s()` to `GestureConfig::handset_default()`, and read device/kernel metadata dynamically in `rabbiz-store`.
- **References:** `AGENTS.md:3`, `CONTRIBUTING.md:55`

---

### QUAL-010
- **ID:** QUAL-010
- **Title:** Blanket `#![allow(clippy::missing_safety_doc)]` in C FFI crate (`rabbiz-sdk-c`) and unchecked `libc::localtime_r` calls in `rabbiz-dialer`
- **Severity:** Low
- **Confidence:** Confirmed
- **Location:** `tools/rabbiz-sdk/crates/rabbiz-sdk-c/src/lib.rs:4`, `tools/rabbiz-apps/crates/rabbiz-dialer/src/model.rs:433-455`
- **Evidence:**
  1. `tools/rabbiz-sdk/crates/rabbiz-sdk-c/src/lib.rs:4` (which holds 31 `unsafe` items — the highest `unsafe` density in the repository per `audit/tools/code-patterns.txt:2`):
     ```rust
     #![allow(clippy::missing_safety_doc)]
     ```
     Suppresses safety documentation requirements across the entire public C99 FFI boundary (`pub unsafe extern "C" fn ...`).
  2. `tools/rabbiz-apps/crates/rabbiz-dialer/src/model.rs:433-455` (6 `unsafe` blocks):
     ```rust
     let mut tm: libc::tm = unsafe { std::mem::zeroed() };
     unsafe { libc::localtime_r(&t, &mut tm) };
     ```
     Called 3 times without checking if `localtime_r` returned NULL (whereas `tools/rabbiz-apps/crates/rabbiz-files/src/model.rs:241` checks `.is_null()`). The same `localtime_r` pattern is duplicated across apps.
- **Impact:** Missing `# Safety` contracts on public FFI functions increase misuse risk for C/C++/Dart SDK consumers; unchecked `localtime_r` return values silently format zeroed `tm` structs (1900-01-00) on out-of-range timestamps.
- **Exploit/Trigger:** Passing an out-of-range `time_t` value to `rabbiz-dialer` timestamp formatting, or calling `rabbiz-sdk-c` FFI functions with invalid/aliased pointers without documented invariants.
- **Recommendation:** Remove `#![allow(clippy::missing_safety_doc)]` from `rabbiz-sdk-c/src/lib.rs` and document pointer-validity/aliasing/lifetime requirements on each `pub unsafe extern "C" fn`. Extract a single safe local-time formatting helper (checking `.is_null()`) into `rabbiz-app-kit` and remove the duplicated `unsafe` `libc::localtime_r` blocks in `rabbiz-dialer` and `rabbiz-files`.
- **References:** CWE-252 (Unchecked Return Value)

---

### QUAL-011
- **ID:** QUAL-011
- **Title:** `TODO`/`FIXME`/`HACK` marker distribution is clean in first-party Rust code and concentrated in vendored SCSC kernel drivers
- **Severity:** Info
- **Confidence:** Confirmed
- **Location:** `shell/Rabbiz/Windowing/LaunchFlow.js:5`, `socs/exynos850/drivers/scsc/`
- **Evidence:**
  Repo-wide scan (`audit/tools/code-patterns.txt:102-109` and verified with `rg -i '\b(TODO|FIXME|HACK|XXX)\b'`):
  - First-party Rust crates (`tools/rabbiz-de/`, `tools/rabbiz-apps/`, `tools/rabbiz-osk/`, `tools/rabbiz-sdk/`): **0** `TODO`/`FIXME`/`HACK`/`XXX` markers.
  - Legacy QML shell: **1** marker (`shell/Rabbiz/Windowing/LaunchFlow.js:5`: `// TODO(M7.1): drive from ToplevelManager.toplevels (QML side)...`).
  - Vendored Samsung SCSC/Exynos kernel drivers (`socs/exynos850/drivers/scsc/`): **25+** inherited vendor markers (e.g., `misc/mxconf.h:33`: `/* TODO: HACK - don't actually pack! */`, `wlan/tx.c:305`, `wlan/mgt.c:3792`, `wlan/scsc_wifilogger_ring_pktfate.c:237,273`).
  - Kernel patches: `devices/samsung/a21s/kernel/patches/0005-a21s-power-modem-dts.patch` (4) and `0009-a21s-tmu-thermal-zones.patch` (1).
- **Impact:** Informational — first-party userspace code has minimal tracked debt markers; inherited vendor driver TODOs are expected in out-of-tree Android BSP ports.
- **Exploit/Trigger:** N/A
- **Recommendation:** No action needed on first-party Rust crates; track security-relevant SCSC driver TODOs (such as missing mutex locking noted in `mgt.c:3792` and `scsc_wifilogger_core.c:106`) in the Kernel/Drivers domain audit.
- **References:** N/A

---

## Positive Observations

1. **Architectural seam and link enforcement in CI:** `.github/workflows/host-checks.yml:16-18,52` runs automated architectural dependency-direction checks (`scripts/check_shell_seams.mjs`, `tools/rabbiz-de/scripts/check_seams.py`), markdown link verification (`scripts/check_links.mjs`), and finding front-matter validation (`scripts/check_findings.mjs`).
2. **Strict Rust lint discipline in `rabbiz-de` and `rabbiz-apps`:** `tools/rabbiz-de/Cargo.toml:95-99` enables `unsafe_code = "warn"` and `clippy::pedantic = "warn"` (enforced with `-D warnings` in `.github/workflows/host-checks.yml:56`), and `tools/rabbiz-apps/Cargo.toml:28-29` also enables `unsafe_code = "warn"`.
3. **Strong host-side unit test culture:** Across the 6 Rust workspaces, the project already maintains **922 `#[test]` functions** (`rabbiz-de`: 442, `rabbiz-apps`: 390, `rabbiz-osk`: 61, `rabbiz-sdk`: 24, `rabbiz-mirror/linux`: 3, `rabbiz-camera`: 2), plus deterministic host tests for hardware key daemons (`tests/test_holdmenu.py`, `tests/test_volume_keys.py`, `tests/test_rabbiz_splash.py`) and Mesa version gating (`tools/qt-scanout/tests`).
4. **Accurate security-policy documentation in `docs/DE-PRIVILEGED-PROTOCOLS.md` §8:** Section 8 (`docs/DE-PRIVILEGED-PROTOCOLS.md:114-127`) accurately reflects the least-privilege defaults implemented in `tools/rabbiz-de/crates/de-config/src/privileged.rs:108-133` (empty public-socket allow-lists for `screencopy`, `data_control`, `virtual_pointer`, and `session_lock`).

---

## Reviewed Files Machine List

```files
.github/workflows/host-checks.yml|reviewed|direct-read|CI workflow and test/lint coverage audit
scripts/check.sh|reviewed|direct-read|Local host check gate vs CI comparison
.gitignore|reviewed|direct-read|Repo hygiene and ignored vendor/target/ko.zst gaps
audit/tools/shellcheck.txt|reviewed|direct-read|Shellcheck triage input
audit/tools/code-patterns.txt|reviewed|direct-read|Unsafe and TODO/FIXME triage input
scripts/de_apps_deploy.sh|reviewed|targeted-read|Verified SC2221/SC2222 dead case pattern
rootfs-overlay/usr/local/bin/rabbiz-osk-ctl|reviewed|direct-read|Verified SC3067 non-POSIX test -O
rootfs-overlay/usr/local/bin/rabbiz-labwc-session|reviewed|direct-read|Verified SC2155 and legacy labwc session status
scripts/install_fex_rootfs.sh|reviewed|direct-read|Verified SC2087 is intentional client-side expansion
scripts/mirror_setup.sh|reviewed|targeted-read|Verified SC2087 intentional and a21s-wayvnc naming
scripts/audio-up.sh|reviewed|targeted-read|Verified SC2034 unused variable i
tools/rabbiz-de/Cargo.toml|reviewed|direct-read|Verified vendor/smithay patch and lints
tools/rabbiz-apps/Cargo.toml|reviewed|direct-read|Verified workspace members and lints
tools/rabbiz-sdk/crates/rabbiz-sdk-c/src/lib.rs|reviewed|targeted-read|Verified missing_safety_doc suppression and unsafe FFI
tools/rabbiz-apps/crates/rabbiz-dialer/src/model.rs|reviewed|targeted-read|Verified unsafe localtime_r without null check
tools/rabbiz-apps/crates/rabbiz-files/src/model.rs|reviewed|targeted-read|Verified unsafe localtime_r and statvfs usage
shell/Rabbiz/Windowing/LaunchFlow.js|reviewed|targeted-read|Verified TODO(M7.1) marker
README.md|reviewed|targeted-read|Verified app count contradictions
ROADMAP.md|reviewed|direct-read|Verified stale M7 description, thermal risk contradiction, and serial leak
CONTRIBUTING.md|reviewed|direct-read|Verified lane/PR rules and secret/serial policy
AGENTS.md|reviewed|direct-read|Verified naming rules and stale SteamOS claim
docs/DE-IPC.md|reviewed|direct-read|Verified 7 vs 17 IPC commands and lock guard docs
docs/DE-KEYGUARD-AUTH.md|reviewed|direct-read|Verified Draft RFC drift and lock guard contradiction
docs/DE-PRIVILEGED-PROTOCOLS.md|reviewed|direct-read|Verified Section 8 accuracy against privileged.rs
tools/rabbiz-de/crates/de-config/src/privileged.rs|reviewed|targeted-read|Cross-checked Policy::builtin() against docs
tools/rabbiz-de/crates/de-config/src/ipc.rs|reviewed|targeted-read|Cross-checked IpcCommand variants against DE-IPC.md
tools/rabbiz-de/crates/de-core/src/runtime/ctl_ipc.rs|reviewed|targeted-read|Cross-checked IPC listener docs
scripts/volume-keys.py|reviewed|diff-check|Confirmed byte-for-byte duplicate of rootfs-overlay copy
rootfs-overlay/usr/local/bin/volume-keys.py|reviewed|diff-check|Confirmed byte-for-byte duplicate of scripts copy
scripts/volume-keys.service|reviewed|diff-check|Confirmed byte-for-byte duplicate of rootfs-overlay copy
rootfs-overlay/etc/systemd/system/volume-keys.service|reviewed|diff-check|Confirmed byte-for-byte duplicate of scripts copy
socs/exynos850/drivers/scsc/scsc_bt.ko.zst|reviewed|git-ls-files|Confirmed tracked binary kernel module artifact
socs/exynos850/drivers/scsc/scsc_mx.ko.zst|reviewed|git-ls-files|Confirmed tracked binary kernel module artifact
socs/exynos850/drivers/scsc/scsc_platform_mif.ko.zst|reviewed|git-ls-files|Confirmed tracked binary kernel module artifact
socs/exynos850/drivers/scsc/scsc_wlan.ko.zst|reviewed|git-ls-files|Confirmed tracked binary kernel module artifact
tools/rabbiz-osk/crates/osk-wayland/src/feedback.rs|reviewed|rg-scan|Verified hardcoded /sys/kernel/a21s_vib/duration_ms
tools/rabbiz-de/crates/de-system/src/fingerprint.rs|reviewed|rg-scan|Verified hardcoded /sys/kernel/a21s_fp
tools/rabbiz-de/crates/de-ui/src/interaction/gestures.rs|reviewed|rg-scan|Verified GestureConfig::a21s() naming
tools/rabbiz-apps/crates/rabbiz-store/src/main.rs|reviewed|rg-scan|Verified hardcoded a21s fallback kernel strings
```
