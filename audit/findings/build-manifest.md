# Domain Audit: Build & Manifest (`BM`)

**Auditor Scope:** CI workflows, bootstrap/build/deploy scripts, Cargo workspace manifests & profiles, C Makefiles & kernel driver Kbuild files, shellcheck triage for build scripts, and reproducible build / toolchain pinning posture.  
**Repository:** `/home/dit/Sites/Labs/android-arm-linux`  
**Audit Mode:** Strict read-only inspection.

---

## Scope & Files Reviewed

### Inspected Files
- **CI / Workflows (direct-read):**
  - `.github/workflows/host-checks.yml`
- **Bootstrap, Build, Deploy & Release Scripts (direct-read):**
  - `scripts/bootstrap_build_env.sh`
  - `scripts/check.sh`
  - `scripts/de_build.sh`
  - `scripts/de_test_host.sh`
  - `scripts/de_deploy.sh`
  - `scripts/de_apps_deploy.sh`
  - `scripts/device_session.sh`
  - `scripts/verify_mirror.sh`
  - `scripts/run_mirror_linux.sh`
  - `scripts/mirror_setup.sh`
  - `scripts/install_fex_rootfs.sh`
  - `scripts/install_chrome_arm.sh`
  - `scripts/install_linux_emu_desktop.sh`
  - `scripts/setup_panvk_test.sh`
  - `scripts/verify_apps.sh`
  - `scripts/build_emu_image.sh`
  - `scripts/build_mac_app.sh`
  - `scripts/build_mac_emu_app.sh`
  - `scripts/release/public_history.sh`
- **Rust / Swift / Container Build Manifests (direct-read):**
  - `tools/rabbiz-de/Cargo.toml`
  - `tools/rabbiz-de/rust-toolchain.toml`
  - `tools/rabbiz-de/Dockerfile`
  - `tools/rabbiz-de/crates/de-bin/Cargo.toml`
  - `tools/rabbiz-apps/Cargo.toml`
  - `tools/rabbiz-osk/Cargo.toml`
  - `tools/rabbiz-sdk/Cargo.toml`
  - `tools/rabbiz-sdk/Dockerfile.emu`
  - `tools/rabbiz-camera/Cargo.toml`
  - `tools/rabbiz-mirror/linux/Cargo.toml`
  - `tools/rabbiz-mirror/Package.swift`
- **C / Kernel Driver Build Files (direct-read):**
  - `tools/cbd/Makefile`
  - `socs/exynos850/drivers/abox/Kbuild`
  - `socs/exynos850/drivers/acpm/Makefile`
  - `socs/exynos850/drivers/cpif/Kbuild`
  - `socs/exynos850/drivers/fingerprint/Kbuild`
  - `socs/exynos850/drivers/s2mu106/Kbuild`
  - `socs/exynos850/drivers/scsc/Kbuild`
  - `socs/exynos850/drivers/sensorhub/Kbuild`
  - `socs/exynos850/drivers/tmu/Kbuild`
  - `socs/exynos850/drivers/vibrator/Kbuild`
- **Documentation & Phase 3 Tool Outputs (targeted-read):**
  - `docs/BUILD.md` (lines 1–120)
  - `audit/tools/shellcheck.txt`
  - `audit/tools/supply-chain-scan.txt`

### Files NOT Inspected (and Reason)
- Individual member crate `Cargo.toml` files (40+ sub-crates under `tools/rabbiz-*/crates/`) — except `tools/rabbiz-de/crates/de-bin/Cargo.toml`; profile definitions in Cargo are workspace-root only (`rg` confirmed no `[profile.*]` blocks exist outside `tools/rabbiz-de/Cargo.toml`). Dependency audit is handled in the Supply Chain domain.
- `Cargo.lock` files — lockfile presence verified; full crate CVE/advisory analysis belongs to Supply Chain domain.
- `scripts/deploy_shell.sh`, `scripts/de_m1_check.sh`, `scripts/shell_perf_compare.sh`, diagnostic/hardware scripts (`a21s-power-diag.sh`, `abox_*.sh`, `audio-up.sh`, `charger-up.sh`, `frame_time.sh`, etc.), and `rootfs-overlay/` scripts — outside primary Build & Manifest scope (shellcheck warnings in non-build scripts are deferred to their respective domains).
- `src/` and `build/` — gitignored local build trees, not part of tracked repository sources.

---

## Findings

```
ID: BM-001
Title: TLS certificate verification disabled (curl -k) and unverified Arch package install as root in mirror_setup.sh
Severity: High
Confidence: Confirmed
Location: scripts/mirror_setup.sh:47-56
Evidence:
  46:         if [ ! -f "$CACHE/$package" ]; then
  47:             curl -skL --fail --max-time 120 \
  48:                 -o "$CACHE/$package" \
  49:                 "https://mirror.archlinuxarm.org/aarch64/extra/$package"
  50:         fi
  51:     done
  52:     say "Installing packages on the device"
  53:     scp -q "$CACHE"/aml-1.0.0-1-aarch64.pkg.tar.xz \
  54:            "$CACHE"/neatvnc-1.0.3-1-aarch64.pkg.tar.xz \
  55:            "$CACHE"/wayvnc-0.10.2-1-aarch64.pkg.tar.xz "root@$DEVICE:/tmp/"
  56:     ssh "root@$DEVICE" 'pacman -U --noconfirm /tmp/aml-1.0.0-1-aarch64.pkg.tar.xz /tmp/neatvnc-1.0.3-1-aarch64.pkg.tar.xz /tmp/wayvnc-0.10.2-1-aarch64.pkg.tar.xz'
Impact: An on-path network attacker (MITM) can intercept the HTTPS connection to mirror.archlinuxarm.org and substitute trojanized `.pkg.tar.xz` archives containing malicious binaries or `.INSTALL` scriptlets, achieving full root code execution on the A21s device when `pacman -U --noconfirm` installs them.
Exploit/Trigger: Running `./scripts/mirror_setup.sh` on an untrusted network (or with a poisoned local `$CACHE`) when `wayvnc`/`neatvnc`/`aml` are not yet installed on the device.
Recommendation: Remove `-k` from `curl` (use `curl -fsSL --proto '=https' --tlsv1.2`), pin and verify SHA-256 digests for all three packages before copying to the device (matching the pattern used in `scripts/install_fex_rootfs.sh:28`), or download and verify `.sig` files with `pacman-key`/GPG. Also stage packages in a root-only temporary directory rather than `/tmp/`.
References: CWE-295 (Improper Certificate Validation), CWE-494 (Download of Code Without Integrity Check)
```

```
ID: BM-002
Title: Predictable /tmp staging paths in root deployment scripts allow local TOCTOU privilege escalation
Severity: High
Confidence: Confirmed
Location: scripts/de_deploy.sh:34-70; scripts/de_apps_deploy.sh:116-132
Evidence:
  scripts/de_deploy.sh:
  34: scp -q "$BIN" "$HOST:/tmp/rabbiz-de.new"
  ...
  41: scp -q "$OVL/etc/systemd/system/rabbiz-session.service" \
  42:     "$HOST:/tmp/rabbiz-session.service.new"
  ...
  55: install -m 0755 -o root -g root /tmp/rabbiz-de.new /usr/local/bin/rabbiz-de

  scripts/de_apps_deploy.sh:
  116: tmp_remote=/tmp/rabbiz-apps-bin.$$
  117: ssh "$HOST" "mkdir -p $tmp_remote"
  118: for f in "${bins[@]}"; do
  119:   scp -q "$f" "$HOST:$tmp_remote/$(basename "$f").new"
  ...
  127: for new in $tmp_remote/*.new; do
  128:   name=$(basename "$new" .new)
  129:   install -m 0755 -o root -g root "$new" "/usr/local/bin/$name"
  130: done
Impact: In `de_apps_deploy.sh`, an unprivileged local user (`rabbiz`) on the device can pre-create `/tmp/rabbiz-apps-bin.<pid>` (predictable 16-bit PID range) with mode `0777`; `mkdir -p` succeeds silently, and because `rabbiz` owns the directory, `rabbiz` can rename/replace any `.new` file inside it between `scp` and `install` (even with `fs.protected_regular=2` enabled), causing `root` to install an attacker-controlled binary into `/usr/local/bin/`. In `de_deploy.sh`, predictable `/tmp/rabbiz-*.new` paths are similarly vulnerable to pre-creation DoS (with `fs.protected_regular>=1`) or file-replacement LPE (if `fs.protected_regular=0`), including systemd unit files.
Exploit/Trigger: Unprivileged local process running as `rabbiz` on the target device during a developer deployment via `de_deploy.sh` or `de_apps_deploy.sh`.
Recommendation: Create a private root-owned staging directory on the device via `tmp_remote=$(ssh "$HOST" 'mktemp -d /root/.rabbiz-deploy.XXXXXX')` (or `mktemp -d` under `/run/` with `chmod 0700`), transfer files there, install atomically, and remove the directory on `EXIT`.
References: CWE-377 (Insecure Temporary File), CWE-367 (TOCTOU Race Condition)
```

```
ID: BM-003
Title: Silent fallback to unpinned upstream HEAD for uniLoader bootloader and short Git commit prefixes
Severity: High
Confidence: Confirmed
Location: scripts/bootstrap_build_env.sh:6,8,44-46
Evidence:
  5: LINUX_REPO="https://github.com/samsungexynos850/linux.git"
  6: LINUX_COMMIT="14a425b3a"
  7: UNILOADER_REPO="https://github.com/samsungexynos850/uniLoader.git"
  8: UNILOADER_COMMIT="06a1bba"
  ...
  42: if [[ ! -d "$ROOT_DIR/src/uniLoader/.git" ]]; then
  43:   echo "[*] Cloning samsungexynos850/uniLoader ($UNILOADER_COMMIT)..."
  44:   git clone --depth 1 "$UNILOADER_REPO" "$ROOT_DIR/src/uniLoader"
  45:   git -C "$ROOT_DIR/src/uniLoader" fetch --depth 20 origin "$UNILOADER_COMMIT" || true
  46:   git -C "$ROOT_DIR/src/uniLoader" checkout "$UNILOADER_COMMIT" || true
Impact: Because `|| true` suppresses errors on both `git fetch` and `git checkout` for `uniLoader` (and GitHub often rejects fetching abbreviated commit SHAs), a fresh bootstrap can silently leave `src/uniLoader` on an unpinned, arbitrary upstream `HEAD` commit. Since `uniLoader` is the bare-metal bootloader flashed to the device, silently building an unintended upstream revision introduces a direct boot-chain supply-chain risk and breaks build reproducibility. Additionally, 7- and 9-character short SHAs (`06a1bba`, `14a425b3a`) are weaker than full 40-character hashes and existing `src/linux`/`src/uniLoader` checkouts are never verified against the pinned commit.
Exploit/Trigger: Running `./scripts/bootstrap_build_env.sh` on a fresh machine when `06a1bba` is not within `--depth 20` or when the remote rejects abbreviated-SHA fetch, or if the upstream `uniLoader` default branch is compromised.
Recommendation: Pin full 40-character commit SHAs for both `LINUX_COMMIT` and `UNILOADER_COMMIT`, remove `|| true` on lines 45–46 so checkout failure aborts immediately, and verify `git rev-parse HEAD` equals the exact 40-char pin (including when `src/linux` or `src/uniLoader` already exists).
References: CWE-494 (Download of Code Without Integrity Check), CWE-1357 (Reliance on Insufficiently Trustworthy Component)
```

```
ID: BM-004
Title: GitHub Actions workflow uses mutable action tags/branches and lacks least-privilege permissions block
Severity: Medium
Confidence: Confirmed
Location: .github/workflows/host-checks.yml:1-42
Evidence:
  1: name: host-checks
  2: on:
  3:   pull_request:
  4:   push:
  5:     branches: [main]
  6: jobs:
  7:   fast:
  8:     runs-on: ubuntu-latest
  9:     steps:
  10:       - uses: actions/checkout@v4
  11:       - uses: actions/setup-node@v4
  ...
  30:       - uses: actions/checkout@v4
  ...
  39:       - uses: dtolnay/rust-toolchain@stable
  ...
  42:       - uses: actions/cache@v4
Impact: No `permissions:` block is declared at workflow or job level, so `GITHUB_TOKEN` inherits repository defaults instead of least-privilege `contents: read`. All four actions — including third-party `dtolnay/rust-toolchain@stable` — are referenced by mutable tags/branches rather than immutable 40-character commit SHAs. If any upstream tag/branch is hijacked, malicious code runs in CI on every PR and push to `main`.
Exploit/Trigger: Compromise of an upstream action repository/tag or maintainer account; amplified if repository default `GITHUB_TOKEN` permissions are read-write.
Recommendation: Add top-level `permissions: { contents: read }` and pin every `uses:` reference to a full 40-character commit SHA with a version comment (e.g., `actions/checkout@<sha> # v4`).
References: CWE-829 (Inclusion of Functionality from Untrusted Control Sphere), GitHub Security Hardening for Actions
```

```
ID: BM-005
Title: Unverified remote binaries/packages downloaded and extracted as root (Chrome .deb, PanVK .so)
Severity: Medium
Confidence: Confirmed
Location: scripts/install_chrome_arm.sh:17-26; scripts/setup_panvk_test.sh:30-40
Evidence:
  scripts/install_chrome_arm.sh:
  17: DEB_URL="https://dl.google.com/linux/direct/google-chrome-stable_current_arm64.deb"
  ...
  22:   cd /tmp
  23:   wget -q --show-progress -O chrome-arm64.deb "'"${DEB_URL}"'"
  24:   mkdir -p /tmp/chrome-extract && cd /tmp/chrome-extract
  25:   bsdtar -xf /tmp/chrome-arm64.deb data.tar.xz
  26:   tar -xJf data.tar.xz --no-same-owner --no-same-permissions -C /

  scripts/setup_panvk_test.sh:
  30: BASE_URL="https://github.com/zenithblue-oss/panvk-kbase-android/releases/download/panvk-kbase-v26.3.1"
  34:   curl -sL -o "$STAGING/libvulkan_panfrost-android-aarch64.so" "${BASE_URL}/libvulkan_panfrost-android-aarch64.so"
  39:   curl -sL -o "$STAGING/panfrost_icd.aarch64.json" "${BASE_URL}/panfrost_icd.aarch64.json"
Impact: `install_chrome_arm.sh` downloads a floating unpinned `.deb` as root and extracts `data.tar.xz` directly into `/` without verifying the package signature or SHA-256 digest (and uses predictable `/tmp/chrome-extract`). `setup_panvk_test.sh` downloads a third-party shared library using `curl -sL` (without `--fail`, so HTTP 404/500 error pages are silently saved as `.so` files) and without SHA-256 verification.
Exploit/Trigger: Upstream release tag/artifact tampering, CDN compromise, or local `/tmp/chrome-extract` race on the device.
Recommendation: Verify Google's APT repository GPG signature (or pin a specific versioned `.deb` + SHA-256) and extract in `mktemp -d /root/.chrome.XXXXXX` in `install_chrome_arm.sh`. In `setup_panvk_test.sh`, add `--fail` to `curl` and verify pinned SHA-256 digests for both downloaded artifacts.
References: CWE-494 (Download of Code Without Integrity Check), CWE-377 (Insecure Temporary File)
```

```
ID: BM-006
Title: Missing compiler and linker hardening flags for root-privileged C modem boot daemon (a21s-cbd)
Severity: Medium
Confidence: Confirmed
Location: tools/cbd/Makefile:7-12
Evidence:
  7: CC ?= cc
  8: CFLAGS ?= -O2 -Wall -Wextra
  9: TARGET = a21s-cbd
  10:
  11: $(TARGET): a21s-cbd.c
  12: 	$(CC) $(CFLAGS) -o $@ $<
Impact: `a21s-cbd` is a native C daemon that parses modem firmware images and interacts with kernel modem/NV interfaces as root. The Makefile specifies no stack protection (`-fstack-protector-strong`), no fortified libc checks (`-D_FORTIFY_SOURCE=2`), no PIE (`-fPIE -pie`), no full RELRO (`-Wl,-z,relro,-z,now`), no format-string warnings (`-Wformat -Wformat-security`), and ignores `LDFLAGS` altogether. Cross-compiling with a generic `aarch64-linux-gnu-gcc` toolchain (as instructed on line 4) may therefore emit an unhardened binary.
Exploit/Trigger: Memory corruption bug in `a21s-cbd.c` when processing malformed modem firmware/IPC data is significantly easier to exploit without stack canaries, PIE, FORTIFY_SOURCE, and full RELRO.
Recommendation: Add explicit hardening defaults in `tools/cbd/Makefile` using `override`/appendable flags: `-O2 -Wall -Wextra -Wformat -Wformat-security -fstack-protector-strong -D_FORTIFY_SOURCE=2 -fPIE` and `LDFLAGS ?= -pie -Wl,-z,relro,-z,now -Wl,-z,noexecstack`, passing `$(LDFLAGS)` in the link recipe.
References: CWE-693 (Protection Mechanism Failure), OpenSSF Compiler Options Hardening Guide for C and C++
```

```
ID: BM-007
Title: Missing release profile hardening across 5 of 6 Rust workspaces and no overflow-checks / path remapping
Severity: Medium
Confidence: Confirmed
Location: tools/rabbiz-apps/Cargo.toml:1-43; tools/rabbiz-osk/Cargo.toml:1-19; tools/rabbiz-sdk/Cargo.toml:1-44; tools/rabbiz-camera/Cargo.toml:1-15; tools/rabbiz-mirror/linux/Cargo.toml:1-12; tools/rabbiz-de/Cargo.toml:91-93
Evidence:
  tools/rabbiz-de/Cargo.toml:
  91: [profile.release]
  92: lto = "thin"
  93: strip = "symbols"

  (rg scan of all other Cargo.toml files: 0 `[profile.release]` blocks in rabbiz-apps, rabbiz-osk, rabbiz-sdk, rabbiz-camera, rabbiz-mirror/linux; no `.cargo/config.toml` anywhere in the repo.)
Impact:
  1. Five Rust workspaces/packages (`rabbiz-apps`, `rabbiz-osk`, `rabbiz-sdk`, `rabbiz-camera`, `rabbiz-mirror/linux`) have no `[profile.release]` section, shipping unstripped symbol tables and missing LTO.
  2. All six workspaces leave `overflow-checks = false` (Rust release default) and `panic = "unwind"`, including security-sensitive binaries such as the compositor (`rabbiz-de`), privileged auth helper (`rabbiz-auth-helper`), and Wayland/network/image parsers.
  3. No `.cargo/config.toml` sets `--remap-path-prefix`, so release binaries embed local developer filesystem paths in panic strings.
  4. `tools/rabbiz-camera/Cargo.toml` and `tools/rabbiz-mirror/linux/Cargo.toml` omit `[lints.rust] unsafe_code = "warn"` (present in the other four workspaces).
Exploit/Trigger: Arithmetic overflow in release builds wraps silently instead of trapping; unstripped binaries and embedded host paths leak build environment metadata and simplify reverse engineering.
Recommendation: Add a consistent `[profile.release]` (`lto = "thin"`, `strip = "symbols"`, `panic = "abort"` where unwinding is not required, and consider `overflow-checks = true` for `rabbiz-de`/`rabbiz-auth-helper`) across all workspaces, add `[lints.rust] unsafe_code = "warn"` to `rabbiz-camera` and `rabbiz-mirror/linux`, and add `.cargo/config.toml` with `--remap-path-prefix` for reproducible path-free builds.
References: CWE-190 (Integer Overflow or Wraparound), CWE-215 (Insertion of Sensitive Information Into Debugging Code)
```

```
ID: BM-008
Title: Reproducible build and toolchain pinning gaps (floating Docker/CI Rust versions, missing --locked)
Severity: Medium
Confidence: Confirmed
Location: .github/workflows/host-checks.yml:39,56,58; tools/rabbiz-de/Dockerfile:5; tools/rabbiz-de/rust-toolchain.toml:2; scripts/de_apps_deploy.sh:38,69; scripts/de_build.sh:25
Evidence:
  tools/rabbiz-de/rust-toolchain.toml:
  2: channel = "1.99.1"

  tools/rabbiz-de/Dockerfile:
  5: FROM docker.io/library/rust:1.99-bookworm

  .github/workflows/host-checks.yml:
  39:       - uses: dtolnay/rust-toolchain@stable
  56:         run: cargo clippy --workspace --all-targets -- -D warnings
  58:         run: cargo test --workspace

  scripts/de_apps_deploy.sh:
  38: FALLBACK_IMAGE="rust:1.99-bookworm"
  69:       "${pre}cargo build --release --workspace && \
Impact:
  1. Toolchain drift: `tools/rabbiz-de/rust-toolchain.toml` pins `1.99.1`, `Dockerfile` and `de_apps_deploy.sh` float on `rust:1.99-bookworm` (no patch pin or `@sha256:` digest), and CI uses `dtolnay/rust-toolchain@stable` (floating). Five other Rust workspaces have no `rust-toolchain.toml`.
  2. Lockfile enforcement: Although `Cargo.lock` is committed in all 6 workspaces, CI (`host-checks.yml`) and most build scripts (`de_build.sh`, `de_apps_deploy.sh`, `de_test_host.sh`, `check.sh`, `verify_mirror.sh`, `run_mirror_linux.sh`) omit `--locked`, allowing silent dependency re-resolution instead of failing on lockfile drift.
  3. Unpinned `apt-get install` packages in CI and both Dockerfiles (`tools/rabbiz-de/Dockerfile`, `tools/rabbiz-sdk/Dockerfile.emu`).
  4. `docs/BUILD.md:67` references a local kernel build container `rabbizos-kbuild:usb-investigation` whose Dockerfile is not tracked in the repository.
Exploit/Trigger: Upstream container tag mutation, Rust toolchain release changes, or accidental `Cargo.lock` updates during CI/release builds.
Recommendation: Pin Docker base images by `@sha256:` digest, align CI to `tools/rabbiz-de/rust-toolchain.toml` (or add a repo-root `rust-toolchain.toml`), pass `--locked` to all `cargo build`/`test`/`clippy` commands in CI and release/deploy scripts (as already done in `scripts/build_mac_emu_app.sh:14` and `scripts/install_linux_emu_desktop.sh:13`), and track the kernel kbuild Dockerfile in-repo.
References: SLSA Build Level 2 / Reproducible Builds
```

```
ID: BM-009
Title: Command/code injection via unescaped argument and env var interpolation in host/device scripts
Severity: Low
Confidence: Confirmed
Location: scripts/verify_mirror.sh:25; scripts/device_session.sh:7,12; scripts/de_deploy.sh:113
Evidence:
  scripts/verify_mirror.sh:
  8: OUT=${1:-/tmp/rabbiz-mirror-verify.png}
  25:     read -r WIDTH HEIGHT < <(python3 -c "from PIL import Image; im=Image.open('$OUT'); print(im.width, im.height)")

  scripts/device_session.sh:
  7: wait_s="${DEVICE_LOCK_WAIT:-45}"
  9: exec ssh "$host" "
  12: if ! flock --timeout $wait_s 9; then

  scripts/de_deploy.sh:
  113: sed -i.bak "s/__SKIP_PROBE__/${DE_DEPLOY_SKIP_PROBE:-0}/g" "$REMOTE_SCRIPT"
Impact:
  - In `verify_mirror.sh:25`, passing an output filename containing a single quote `'` breaks out of the Python string and executes arbitrary Python code locally.
  - In `device_session.sh:12`, an unvalidated `DEVICE_LOCK_WAIT` containing shell metacharacters is injected directly into the remote SSH command executed as `root`.
  - In `de_deploy.sh:113`, an unvalidated `DE_DEPLOY_SKIP_PROBE` containing `/` or shell metacharacters breaks `sed` or injects shell syntax into the remote root script.
Exploit/Trigger: Requires attacker-controlled CLI argument (`$1`) or environment variable (`DEVICE_LOCK_WAIT`, `DE_DEPLOY_SKIP_PROBE`) when invoking these developer scripts.
Recommendation: Pass `$OUT` to Python via `sys.argv[1]` (`python3 -c "import sys; from PIL import Image; im=Image.open(sys.argv[1]); print(im.width, im.height)" "$OUT"`); validate `wait_s` matches `^[0-9]+$` in `device_session.sh`; validate `DE_DEPLOY_SKIP_PROBE` is strictly `0` or `1` in `de_deploy.sh`.
References: CWE-78 (OS Command Injection), CWE-94 (Code Injection)
```

```
ID: BM-010
Title: Shellcheck-confirmed dead pattern in de_apps_deploy.sh and silent codesign failure masking in build_mac_app.sh
Severity: Low
Confidence: Confirmed
Location: scripts/de_apps_deploy.sh:108; scripts/build_mac_app.sh:61,63
Evidence:
  scripts/de_apps_deploy.sh:
  107:   case "$desc" in
  108:     *ELF*aarch64*|*ELF*ARM\ aarch64*) ;;

  scripts/build_mac_app.sh:
  61:     codesign --force --sign "$IDENTITY" "$APP" >/dev/null 2>&1 || true
  62: else
  63:     codesign --force --sign - "$APP" >/dev/null 2>&1 || true
Impact:
  - In `de_apps_deploy.sh:108` (flagged by shellcheck `SC2221`/`SC2222`), `*ELF*aarch64*` always shadows `*ELF*ARM\ aarch64*`, leaving dead code.
  - In `build_mac_app.sh:61,63`, `codesign` errors are silently swallowed (`>/dev/null 2>&1 || true`), so a broken/unsigned `.app` bundle can be produced without warning, and `--options runtime` (macOS Hardened Runtime) is not enabled.
Exploit/Trigger: Normal execution of `de_apps_deploy.sh` (benign dead branch) or `build_mac_app.sh` when keychain/codesign fails.
Recommendation: Simplify line 108 of `de_apps_deploy.sh` to `*ELF*aarch64*) ;;`; in `build_mac_app.sh`, drop `|| true` (or fail loudly on non-ad-hoc signing failure) and add `--options runtime` when signing with a real Apple Development/Developer ID identity.
References: ShellCheck SC2221/SC2222, Apple Code Signing Hardened Runtime
```

```
ID: BM-011
Title: CI workflow omits five Rust workspaces and host unit tests covered by scripts/check.sh
Severity: Low
Confidence: Confirmed
Location: .github/workflows/host-checks.yml:13-22,24-58; scripts/check.sh:2,18,29-37
Evidence:
  scripts/check.sh:
  2: # One command for every host-side check. No CI exists in this repo yet; run before committing.
  18: python3 -B tests/test_rabbiz_splash.py
  30:   cargo test --quiet --manifest-path tools/rabbiz-osk/Cargo.toml --workspace --exclude osk-wayland
  32:     cargo test --quiet --manifest-path tools/rabbiz-apps/Cargo.toml --workspace
  35:     cargo test --quiet --manifest-path tools/rabbiz-sdk/Cargo.toml --workspace

  .github/workflows/host-checks.yml:
  (Only runs `rabbiz-de` Cargo checks; omits `rabbiz-apps`, `rabbiz-osk`, `rabbiz-sdk`, `rabbiz-camera`, `rabbiz-mirror/linux`, and `tests/test_rabbiz_splash.py`.)
Impact: Regressions, formatting violations, clippy warnings, or test failures in `rabbiz-apps`, `rabbiz-osk`, `rabbiz-sdk`, `rabbiz-camera`, `rabbiz-mirror/linux`, and `tests/test_rabbiz_splash.py` are not gated by pull-request CI. `scripts/check.sh:2` also carries a stale comment claiming no CI exists.
Exploit/Trigger: PRs modifying those workspaces merge without automated test/lint verification.
Recommendation: Expand `.github/workflows/host-checks.yml` to run `tests/test_rabbiz_splash.py` and Cargo `fmt`/`clippy`/`test --locked` across `rabbiz-apps`, `rabbiz-osk` (`--exclude osk-wayland` or full on Linux), `rabbiz-sdk`, `rabbiz-camera`, and `rabbiz-mirror/linux`; update the stale comment in `scripts/check.sh:2`.
References: N/A (CI/Quality Assurance)
```

---

## Shellcheck Triage Summary (`audit/tools/shellcheck.txt`)

All build/deploy-script entries in `audit/tools/shellcheck.txt` were verified directly in source:
- `scripts/de_apps_deploy.sh:108` (`SC2221`/`SC2222`): Confirmed dead pattern — reported in `BM-010`.
- `scripts/de_apps_deploy.sh:117` (`SC2029`): Verified intentional client-side expansion of `$tmp_remote`; underlying security risk (predictable `/tmp` staging) reported in `BM-002`.
- `scripts/install_fex_rootfs.sh:18` (`SC2087`): Verified intentional client-side expansion of `${SQSH_URL}` and `${SQSH_SHA256}` (inner `SHIM` heredoc properly escapes `\$`). Benign.
- `scripts/mirror_setup.sh:65,85` (`SC2029`/`SC2087`): Verified intentional client-side expansion of `$UNIT_PATH`, `${RABBIZ_UID}`, `$LISTEN`, `$HELPER_PATH`. Benign (real vulnerability in this file is `curl -skL`, reported in `BM-001`).
- `scripts/verify_apps.sh:9,11` (`SC2029`): Verified intentional client-side expansion of hardcoded `$p` array items. Benign.
- Remaining entries (`devices/xiaomi/muyu/scripts/backup_muyu_stock.sh`, `rootfs-overlay/usr/local/bin/*`, `scripts/a21s-power-diag.sh`, `scripts/audio-up.sh`, `scripts/charger-up.sh`) are non-build runtime/diagnostic scripts outside Domain 1 scope.

---

## Positive Observations

1. **No hardening-disabling compiler flags in kernel driver Kbuild/Makefile files:** All 9 tracked Exynos 850 driver build files (`socs/exynos850/drivers/{abox,cpif,fingerprint,s2mu106,scsc,sensorhub,tmu,vibrator}/Kbuild` and `acpm/Makefile`) were inspected; none disable stack protection (`-fno-stack-protector`), PIE, or security warnings. Only `abox/Kbuild:1` suppresses benign C99/unused-function warnings (`-Wno-declaration-after-statement -Wno-unused-function`).
2. **Verified SHA-256 integrity check for FEX guest rootfs:** `scripts/install_fex_rootfs.sh:16,26,28` downloads `ArchLinux.sqsh` over HTTPS with `curl -fSL` and enforces an explicit SHA-256 verification (`sha256sum -c -`) before enabling the systemd service.
3. **All 6 Rust workspaces commit `Cargo.lock` with zero git/wildcard dependencies:** Confirmed across `tools/rabbiz-{de,apps,osk,sdk,camera}` and `tools/rabbiz-mirror/linux` (`audit/tools/supply-chain-scan.txt:51-57`), and `scripts/build_mac_emu_app.sh:14` and `scripts/install_linux_emu_desktop.sh:13` properly pass `--locked`.
4. **Strict lints and `unsafe_code = "warn"` in core Rust workspaces:** `tools/rabbiz-de/Cargo.toml:95-99` enables `unsafe_code = "warn"` and `clippy::pedantic = "warn"`, enforced with `-D warnings` in `.github/workflows/host-checks.yml:56`. `rabbiz-apps`, `rabbiz-osk`, and `rabbiz-sdk` also set `unsafe_code = "warn"`.
5. **Defensive `/lib` symlink guardrail in VM image builder:** `scripts/build_emu_image.sh:113-116` explicitly fails if `rootfs-overlay/lib` exists, preventing rootfs corruption on Arch Linux ARM (`/lib -> usr/lib`).
6. **Public release sanitization script with `gitleaks` gate:** `scripts/release/public_history.sh` implements non-destructive single-branch history rewriting, mailmap/path/IP redaction, commit/file-list integrity verification, and optional `gitleaks` secret scanning without auto-pushing.

---

## Cross-Domain Leads

- **Network / DE Domain:** `scripts/mirror_setup.sh:75,161-162` allows binding unauthenticated `wayvnc` to `0.0.0.0:5900` (`--lan`), and `tools/rabbiz-sdk/Dockerfile.emu:20-24` runs `rabbiz-de` as `root` with no `USER` directive and binds emulator/control ports on `0.0.0.0:5910` and `0.0.0.0:5911`.
- **Rootfs / Shell Scripts Domain:** `audit/tools/shellcheck.txt` flags `SC2155` in `rootfs-overlay/usr/local/bin/rabbiz-labwc-session:50` and `SC3067` (non-POSIX `test -O`) in `rootfs-overlay/usr/local/bin/rabbiz-osk-ctl:13`.

---

## Reviewed Files Machine List

```files
.github/workflows/host-checks.yml|reviewed|direct-read|CI workflow audit
scripts/bootstrap_build_env.sh|reviewed|direct-read|Bootstrap & upstream pinning audit
scripts/check.sh|reviewed|direct-read|Host check script audit
scripts/de_build.sh|reviewed|direct-read|DE build script audit
scripts/de_test_host.sh|reviewed|direct-read|DE host test script audit
scripts/de_deploy.sh|reviewed|direct-read|DE deploy script audit
scripts/de_apps_deploy.sh|reviewed|direct-read|Apps build/deploy script audit
scripts/device_session.sh|reviewed|direct-read|Device lock/SSH wrapper audit
scripts/verify_mirror.sh|reviewed|direct-read|Mirror verification script audit
scripts/run_mirror_linux.sh|reviewed|direct-read|Linux mirror build/run script audit
scripts/mirror_setup.sh|reviewed|direct-read|Wayvnc package download/install audit
scripts/install_fex_rootfs.sh|reviewed|direct-read|FEX rootfs download/integrity audit
scripts/install_chrome_arm.sh|reviewed|direct-read|Chrome ARM64 download/install audit
scripts/install_linux_emu_desktop.sh|reviewed|direct-read|Linux emulator launcher build/install audit
scripts/setup_panvk_test.sh|reviewed|direct-read|PanVK binary download script audit
scripts/verify_apps.sh|reviewed|direct-read|Shellcheck triage
scripts/build_emu_image.sh|reviewed|direct-read|Tier 3 VM image build script audit
scripts/build_mac_app.sh|reviewed|direct-read|macOS Mirror app build/sign audit
scripts/build_mac_emu_app.sh|reviewed|direct-read|macOS Emulator app build/sign audit
scripts/release/public_history.sh|reviewed|direct-read|Release history sanitization script audit
tools/rabbiz-de/Cargo.toml|reviewed|direct-read|Workspace manifest & release profile audit
tools/rabbiz-de/rust-toolchain.toml|reviewed|direct-read|Rust toolchain pinning audit
tools/rabbiz-de/Dockerfile|reviewed|direct-read|ARM64 build container audit
tools/rabbiz-de/crates/de-bin/Cargo.toml|reviewed|direct-read|Binary targets & lint inheritance check
tools/rabbiz-apps/Cargo.toml|reviewed|direct-read|Workspace manifest & profile audit
tools/rabbiz-osk/Cargo.toml|reviewed|direct-read|Workspace manifest & profile audit
tools/rabbiz-sdk/Cargo.toml|reviewed|direct-read|Workspace manifest & profile audit
tools/rabbiz-sdk/Dockerfile.emu|reviewed|direct-read|Emulator container build audit
tools/rabbiz-camera/Cargo.toml|reviewed|direct-read|Package manifest & profile audit
tools/rabbiz-mirror/linux/Cargo.toml|reviewed|direct-read|Package manifest & profile audit
tools/rabbiz-mirror/Package.swift|reviewed|direct-read|Swift package manifest audit
tools/cbd/Makefile|reviewed|direct-read|C modem boot daemon compiler flags audit
socs/exynos850/drivers/abox/Kbuild|reviewed|direct-read|Kernel driver compiler flags audit
socs/exynos850/drivers/acpm/Makefile|reviewed|direct-read|Kernel driver compiler flags audit
socs/exynos850/drivers/cpif/Kbuild|reviewed|direct-read|Kernel driver compiler flags audit
socs/exynos850/drivers/fingerprint/Kbuild|reviewed|direct-read|Kernel driver compiler flags audit
socs/exynos850/drivers/s2mu106/Kbuild|reviewed|direct-read|Kernel driver compiler flags audit
socs/exynos850/drivers/scsc/Kbuild|reviewed|direct-read|Kernel driver compiler flags audit
socs/exynos850/drivers/sensorhub/Kbuild|reviewed|direct-read|Kernel driver compiler flags audit
socs/exynos850/drivers/tmu/Kbuild|reviewed|direct-read|Kernel driver compiler flags audit
socs/exynos850/drivers/vibrator/Kbuild|reviewed|direct-read|Kernel driver compiler flags audit
docs/BUILD.md|reviewed|targeted-read|Build procedure & kbuild container cross-check (lines 1-120)
audit/tools/shellcheck.txt|reviewed|direct-read|Phase 3 shellcheck triage
audit/tools/supply-chain-scan.txt|reviewed|direct-read|Phase 3 lockfile/manifest cross-check
```
