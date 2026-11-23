# Domain Audit: Supply Chain (`SC`)

**Auditor:** Principal Security & Quality Domain Auditor
**Repository:** `/home/dit/Sites/Labs/android-arm-linux` (RabbizOS)
**Scope:** Rust workspaces & lockfiles, SwiftPM & Dart manifests, tracked pre-compiled binaries, vendored C driver provenance & licensing, external downloads in scripts/CI/Dockerfiles, and SBOM/dependency-scanning posture.

---

## Scope & Files Reviewed

### Inspected Files
- `audit/tools/supply-chain-scan.txt` (`direct-read`) — Phase 3 triage input; every claim re-verified directly against source files.
- `audit/tools/binary-blobs.txt` (`direct-read`) — Phase 3 triage input; verified directly with `zstd` and `git ls-files`.
- `LICENSE` (`targeted-read`, lines 1–60)
- `.gitignore` (`direct-read`)
- `.github/workflows/host-checks.yml` (`direct-read`)
- `tools/rabbiz-de/Cargo.toml` (`direct-read`), `tools/rabbiz-de/Cargo.lock` (`python3-parse`), `tools/rabbiz-de/Dockerfile` (`direct-read`), `tools/rabbiz-de/rust-toolchain.toml` (`direct-read`)
- `tools/rabbiz-de/vendor/smithay/Cargo.toml` (`targeted-read`, lines 1–60)
- `tools/rabbiz-apps/Cargo.toml` (`direct-read`), `tools/rabbiz-apps/Cargo.lock` (`python3-parse`), `tools/rabbiz-apps/crates/rabbiz-search/Cargo.toml` (`direct-read`)
- `tools/rabbiz-osk/Cargo.toml` (`direct-read`), `tools/rabbiz-osk/Cargo.lock` (`python3-parse`), `tools/rabbiz-osk/spikes/s2-render/Cargo.toml` (`direct-read`), `tools/rabbiz-osk/dict-build/build_emoji.py` (`targeted-read`, lines 1–80)
- `tools/rabbiz-sdk/Cargo.toml` (`direct-read`), `tools/rabbiz-sdk/Cargo.lock` (`python3-parse`), `tools/rabbiz-sdk/Dockerfile.emu` (`direct-read`), `tools/rabbiz-sdk/packages/rabbiz_sdk_dart/pubspec.yaml` (`direct-read`)
- `tools/rabbiz-camera/Cargo.toml` (`direct-read`), `tools/rabbiz-camera/Cargo.lock` (`python3-parse`)
- `tools/rabbiz-mirror/Package.swift` (`direct-read`), `tools/rabbiz-mirror/linux/Cargo.toml` (`direct-read`), `tools/rabbiz-mirror/linux/Cargo.lock` (`python3-parse`)
- All 47 Rust `Cargo.toml` files under `tools/` (`python3-parse` via `tomllib` for `license`, `git`, and wildcard dependencies)
- `socs/exynos850/drivers/scsc/scsc_bt.ko.zst`, `scsc_mx.ko.zst`, `scsc_platform_mif.ko.zst`, `scsc_wlan.ko.zst` (`binary-inspect` via `zstd -dc`, `git log`, `git ls-files`)
- `socs/exynos850/drivers/LICENSING.md` (`direct-read`)
- `socs/exynos850/drivers/scsc/include/scsc/api/cme3_prim.h` (`targeted-read`, lines 1–40)
- `socs/exynos850/drivers/scsc/bt/scsc_bluez.c` (`targeted-read`, lines 1–30)
- All 365 `.c`/`.h` files under `socs/exynos850/drivers/` (`python3-parse` header scan for SPDX, GPL, `All rights reserved`, `Confidential`/`Proprietary`, plus `rg` for `MODULE_LICENSE`)
- `tools/cbd/a21s-cbd.c` (`targeted-read`, lines 1–80), `tools/cbd/Makefile` (`direct-read`)
- `tools/a21s-et520-id-probe/et520-id-probe.c` (`targeted-read`, lines 1–30)
- `tools/cma-leak/leaktest.c` (`targeted-read`, lines 1–30)
- `scripts/mirror_setup.sh` (`direct-read`)
- `scripts/install_chrome_arm.sh` (`direct-read`)
- `scripts/setup_panvk_test.sh` (`direct-read`)
- `scripts/install_fex_rootfs.sh` (`direct-read`)
- `scripts/bootstrap_build_env.sh` (`direct-read`)
- `scripts/deploy_shell.sh` (`targeted-read`, lines 215–245)
- `scripts/`, `tools/`, `devices/`, `rootfs-overlay/` (`rg-scan` for `curl`, `wget`, `git clone`, `pacman`, `pip`, `npm`, `cargo install`, `rustup`, `urllib`, `requests.get`)

### Not Inspected (and Reason)
- `src/` and `build/` — gitignored local upstream kernel/uniLoader checkouts and build outputs; out of tracked repository scope.
- Full CVE database matching (`cargo audit`) against the 1,462 locked Rust crate entries — network access is prohibited by audit rules, and `cargo-audit` / offline RustSec advisory DB was not run; therefore absence of known CVEs in transitive Rust crates is **Unverified**.
- Individual `.c` source bodies of all 365 files in `socs/exynos850/drivers/` beyond the first 3 KB license header and `MODULE_LICENSE` declarations — outside supply-chain scope (kernel driver code quality/memory safety is covered by the kernel domain).
- `tools/rabbiz-apps/crates/rabbiz-store/src/backend.rs` runtime package-installation logic — noted as a cross-domain lead for Domain 6/7 (Userland/Apps).

---

## Findings

### SC-001
```
ID: SC-001
Title: TLS certificate verification disabled (`curl -k`) when downloading root-installed Arch packages
Severity: High
Confidence: Confirmed
Location: scripts/mirror_setup.sh:47-56
Evidence:
41:     for package in \
42:         aml-1.0.0-1-aarch64.pkg.tar.xz \
43:         neatvnc-1.0.3-1-aarch64.pkg.tar.xz \
44:         wayvnc-0.10.2-1-aarch64.pkg.tar.xz
45:     do
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
Impact: Any active network attacker (MITM on Wi-Fi, proxy, or DNS hijack) between the developer host and `mirror.archlinuxarm.org` can substitute any of the three `.pkg.tar.xz` archives with a malicious Arch package. Because neither SHA-256 digests nor `.sig` PGP signatures are verified before `pacman -U --noconfirm` (and Arch Linux ARM's default `LocalFileSigLevel = Optional` accepts unsigned local packages), a trojanized `.INSTALL` hook executes arbitrary commands as `root` on the phone.
Exploit/Trigger: Developer runs `./scripts/mirror_setup.sh` on a fresh device (or when `tools/rabbiz-mirror/.cache/pkgs/` does not contain the cached packages) over an untrusted or intercepted network.
Recommendation: Remove `-k` from `curl` (use `curl -fsSL --proto '=https' --tlsv1.2`), hardcode verified SHA-256 checksums for all three pinned package filenames, and verify them with `sha256sum -c` before `scp` and `pacman -U`. Also download and verify the matching `.sig` files or enforce `LocalFileSigLevel = Required`.
References: CWE-295 (Improper Certificate Validation), CWE-494 (Download of Code Without Integrity Check)
```

### SC-002
```
ID: SC-002
Title: Unpinned, unverified Google Chrome `.deb` downloaded and extracted directly into `/` as root
Severity: High
Confidence: Confirmed
Location: scripts/install_chrome_arm.sh:17-26
Evidence:
17: DEB_URL="https://dl.google.com/linux/direct/google-chrome-stable_current_arm64.deb"
...
20: ssh "${TARGET}" 'set -e
21:   command -v bsdtar >/dev/null || pacman -S --needed --noconfirm libarchive wget
22:   cd /tmp
23:   wget -q --show-progress -O chrome-arm64.deb "'"${DEB_URL}"'"
24:   mkdir -p /tmp/chrome-extract && cd /tmp/chrome-extract
25:   bsdtar -xf /tmp/chrome-arm64.deb data.tar.xz
26:   tar -xJf data.tar.xz --no-same-owner --no-same-permissions -C /
Impact: The script fetches a floating (`current`) URL with no GPG signature check (no APT `Release.gpg` / `Packages` verification) and no SHA-256 checksum verification, then extracts `data.tar.xz` directly onto the device root filesystem `/` as `root`. Compromised upstream/CDN artifacts or MITM tampering achieve immediate persistent root code execution on the device. Additionally, installations are non-reproducible because the URL is unpinned, and extraction uses predictable `/tmp/chrome-extract` paths on the device.
Exploit/Trigger: Running `./scripts/install_chrome_arm.sh` when the upstream package is compromised, tampered in transit, or unexpectedly altered.
Recommendation: Pin a specific versioned `.deb` URL with a hardcoded SHA-256 digest (or verify Google's APT repository `InRelease` GPG signature against a pinned Google Linux signing key), verify the digest before extraction, and use `mktemp -d` instead of fixed `/tmp/chrome-extract`.
References: CWE-494 (Download of Code Without Integrity Check), CWE-377 (Insecure Temporary File)
```

### SC-003
```
ID: SC-003
Title: Four pre-compiled, unsigned aarch64 kernel modules (`.ko.zst`) tracked in git
Severity: High
Confidence: Confirmed
Location: socs/exynos850/drivers/scsc/scsc_bt.ko.zst:1 (and scsc_mx.ko.zst, scsc_platform_mif.ko.zst, scsc_wlan.ko.zst)
Evidence:
$ git ls-files | rg '\.(ko|ko\.zst|ko\.xz|so|a|bin|elf|fw|img|dtb|dtbo|tar|gz|zst|zip|jar|apk|wasm|dylib|exe)$'
socs/exynos850/drivers/scsc/scsc_bt.ko.zst
socs/exynos850/drivers/scsc/scsc_mx.ko.zst
socs/exynos850/drivers/scsc/scsc_platform_mif.ko.zst
socs/exynos850/drivers/scsc/scsc_wlan.ko.zst

# Decompressed sizes & kernel module signature check (zstd -dc):
socs/exynos850/drivers/scsc/scsc_bt.ko.zst: decompressed=103072 bytes, module_sig=False
socs/exynos850/drivers/scsc/scsc_mx.ko.zst: decompressed=253040 bytes, module_sig=False
socs/exynos850/drivers/scsc/scsc_platform_mif.ko.zst: decompressed=60968 bytes, module_sig=False
socs/exynos850/drivers/scsc/scsc_wlan.ko.zst: decompressed=908304 bytes, module_sig=False

# vermagic in all 4 blobs (audit/tools/binary-blobs.txt:10,19,28,38):
vermagic=7.3.0-g14a425b3a7dc SMP preempt mod_unload aarch64

# Policy contradiction in socs/exynos850/drivers/LICENSING.md:12-13:
12: - Do not add vendor firmware blobs or binaries. Firmware is loaded from the device's own
13:   partitions at runtime and is not redistributed by this project.

# Direct deployment instruction in devices/samsung/a21s/docs/a21s-wifi-bluetooth.md:116:
116: scp socs/exynos850/drivers/scsc/*.ko.zst root@172.16.42.1:/lib/modules/7.3.0-g14a425b3a7dc/updates/
Impact: Opaque, unsigned binary kernel modules (~1.33 MB decompressed) are tracked in git and documented for direct deployment into `/lib/modules/.../updates/` where they execute in kernel space (EL1). Reviewers and downstream builders cannot verify that these binaries match the tracked C source in `socs/exynos850/drivers/scsc/`, nor are they protected by kernel module signatures or reproducible build attestations. Their presence also directly contradicts `socs/exynos850/drivers/LICENSING.md:12` and slipped past `.gitignore:45` (which ignores `*.ko` but not `*.ko.zst`).
Exploit/Trigger: Any contributor or compromised commit updating a `.ko.zst` blob can stealthily introduce a kernel-level backdoor that is invisible to source-level code review.
Recommendation: Remove all four `.ko.zst` binaries from git (`git rm`), add `*.ko.zst` and `*.ko.xz` to `.gitignore`, and build the modules from `socs/exynos850/drivers/scsc/` via `Kbuild` (or distribute release artifacts with provenance/checksums via GitHub Releases rather than git).
References: CWE-494 (Download of Code Without Integrity Check), CWE-1357 (Reliance on Insufficiently Trustworthy Component), SLSA Build L2+
```

### SC-004
```
ID: SC-004
Title: `tools/rabbiz-de` patches `smithay` to a gitignored `vendor/smithay` directory, breaking reproducible builds and CI
Severity: Medium
Confidence: Confirmed
Location: tools/rabbiz-de/Cargo.toml:88-89 and .gitignore:6
Evidence:
# tools/rabbiz-de/Cargo.toml:88-89:
88: [patch.crates-io]
89: smithay = { path = "vendor/smithay" }

# .gitignore:6:
6: tools/rabbiz-de/vendor/

# Verification that vendor/smithay is completely untracked in git:
$ git ls-files tools/rabbiz-de/vendor/ | wc -l
count=0
Impact: `tools/rabbiz-de` cannot be built from a fresh git checkout or in GitHub Actions CI (`.github/workflows/host-checks.yml:24-58`) because `tools/rabbiz-de/vendor/smithay` does not exist in git and no bootstrap script generates or patches it. Furthermore, the security/stability backport described in `tools/rabbiz-de/Cargo.toml:80-87` is unversioned and unaudited in git history, creating a hidden local-state dependency.
Exploit/Trigger: Any clean clone, contributor checkout, or CI run executing `cargo test --workspace` in `tools/rabbiz-de`.
Recommendation: Either track the minimal patched `smithay` crate in git (un-ignoring `tools/rabbiz-de/vendor/smithay/` or using a git fork pinned to a full 40-char commit SHA with a clear patch commit), or update `smithay` to an upstream release containing the fix and remove `[patch.crates-io]`.
References: CWE-1357 (Reliance on Insufficiently Trustworthy Component)
```

### SC-005
```
ID: SC-005
Title: Bootloader (`uniLoader`) git checkout silently ignores failure (`|| true`) and uses short commit hashes
Severity: Medium
Confidence: Confirmed
Location: scripts/bootstrap_build_env.sh:6,8,44-46
Evidence:
5: LINUX_REPO="https://github.com/samsungexynos850/linux.git"
6: LINUX_COMMIT="14a425b3a"
7: UNILOADER_REPO="https://github.com/samsungexynos850/uniLoader.git"
8: UNILOADER_COMMIT="06a1bba"
...
43:   echo "[*] Cloning samsungexynos850/uniLoader ($UNILOADER_COMMIT)..."
44:   git clone --depth 1 "$UNILOADER_REPO" "$ROOT_DIR/src/uniLoader"
45:   git -C "$ROOT_DIR/src/uniLoader" fetch --depth 20 origin "$UNILOADER_COMMIT" || true
46:   git -C "$ROOT_DIR/src/uniLoader" checkout "$UNILOADER_COMMIT" || true
Impact: Because both `git fetch` and `git checkout` for `uniLoader` are suffixed with `|| true`, any failure to fetch or resolve `06a1bba` (for example, if `06a1bba` is older than `--depth 20`, the server rejects short-SHA fetch, or a transient error occurs) causes the script to silently proceed with whatever unpinned `HEAD` exists on `samsungexynos850/uniLoader`. This compromises supply-chain pinning for the device bootloader. Using 7–9 character short hashes instead of full 40-character SHA-1 commit IDs also weakens commit identity verification.
Exploit/Trigger: Upstream `uniLoader` advances more than 20 commits past `06a1bba`, or the remote rejects short-hash object fetches, causing `bootstrap_build_env.sh` to silently build an unpinned bootloader revision.
Recommendation: Pin full 40-character commit SHAs for both `LINUX_COMMIT` and `UNILOADER_COMMIT`, remove `|| true` on lines 45–46, and verify `git -C "$dir" rev-parse HEAD` equals the expected 40-character SHA before continuing.
References: CWE-829 (Inclusion of Functionality from Untrusted Control Sphere)
```

### SC-006
```
ID: SC-006
Title: GitHub Actions workflow uses mutable action tags, lacks `permissions` block, drifts from `rust-toolchain.toml`, and omits 5 Rust workspaces
Severity: Medium
Confidence: Confirmed
Location: .github/workflows/host-checks.yml:1-58
Evidence:
10:       - uses: actions/checkout@v4
11:       - uses: actions/setup-node@v4
...
30:       - uses: actions/checkout@v4
...
39:       - uses: dtolnay/rust-toolchain@stable
...
42:       - uses: actions/cache@v4
Impact:
1. Mutable major tags (`@v4`) and branch refs (`@stable`) can be retargeted upstream; a compromised action release executes arbitrary code in CI runners.
2. No top-level or job-level `permissions:` block is declared, leaving `GITHUB_TOKEN` at the repository default rather than explicit least-privilege (`contents: read`).
3. CI installs `dtolnay/rust-toolchain@stable` while `tools/rabbiz-de/rust-toolchain.toml:2` pins `channel = "1.99.1"`, causing toolchain drift between local/Docker builds and CI.
4. Only `tools/rabbiz-de` is built/tested in CI; the other five Rust workspaces (`rabbiz-apps`, `rabbiz-osk`, `rabbiz-sdk`, `rabbiz-camera`, `rabbiz-mirror/linux`) have no CI checks and no automated `cargo-audit` / `cargo-deny` scanning.
Exploit/Trigger: Compromise of any referenced Action tag (as in recent GitHub Actions supply-chain incidents) or silent regression/vulnerability introduction in the 5 unchecked Rust workspaces.
Recommendation: Pin every `uses:` Action to a full 40-character commit SHA (with version comment), add `permissions: { contents: read }` at the workflow level, align CI Rust toolchain with `rust-toolchain.toml`, and add CI jobs for the remaining Rust workspaces plus `cargo audit` / `cargo deny`.
References: CWE-829 (Inclusion of Functionality from Untrusted Control Sphere), CWE-1357
```

### SC-007
```
ID: SC-007
Title: Vendored Samsung driver headers include a "Confidential" file with missing `LICENSE.txt`, 345/365 files lack SPDX, and GPL-2.0 tools sit under Apache-2.0 root `LICENSE`
Severity: Medium
Confidence: Confirmed
Location: socs/exynos850/drivers/scsc/include/scsc/api/cme3_prim.h:1-8 (and socs/exynos850/drivers/*, tools/cbd/a21s-cbd.c:1, tools/a21s-et520-id-probe/et520-id-probe.c:1)
Evidence:
# socs/exynos850/drivers/scsc/include/scsc/api/cme3_prim.h:1-8:
1: /*****************************************************************************
2:  *
3:  *          Copyright (c) 2015 Samsung Electronics Co., Ltd
4:  *          Confidential information of Samsung Electronics Co., Ltd
5:  *
6:  *          Refer to LICENSE.txt included with this source for details
7:  *          on the license terms.
8:  *

# Header scan across all 365 .c/.h files in socs/exynos850/drivers/:
Total .c/.h files in socs/exynos850/drivers: 365
SPDX counts: {'GPL-2.0+': 1, 'GPL-2.0-or-later': 1, 'GPL-2.0': 18}
Files without SPDX: 345 (of which 81 mention GPL in header)
Files with 'All rights reserved': 239
Files with NO license text in first 3KB: 247

# 6 files declare MODULE_LICENSE("GPL and additional rights") (e.g. wlan/dev.c:718, misc/mx_mmap.c:541),
# yet `git ls-files socs/exynos850/drivers/scsc/LICENSE* socs/exynos850/drivers/COPYING*` returns empty.

# GPL-2.0 C tools outside socs/exynos850/drivers/LICENSING.md scope (root LICENSE is Apache-2.0):
tools/cbd/a21s-cbd.c:1: /* SPDX-License-Identifier: GPL-2.0 */
tools/cbd/Makefile:1: # SPDX-License-Identifier: GPL-2.0
tools/a21s-et520-id-probe/et520-id-probe.c:1: /* SPDX-License-Identifier: GPL-2.0 */
tools/cma-leak/leaktest.c:1-17: (no license header)
Impact: Redistributing `cme3_prim.h` (marked "Confidential information of Samsung Electronics Co., Ltd") and 6 `"GPL and additional rights"` modules without the upstream `LICENSE.txt`/`COPYING` creates copyright and license-compliance exposure. Additionally, the repository root `LICENSE` is `Apache-2.0`, while `socs/exynos850/drivers/LICENSING.md` only documents exceptions for `drivers/` — leaving GPL-2.0 tools under `tools/` (`tools/cbd/`, `tools/a21s-et520-id-probe/`) undocumented at the root level.
Exploit/Trigger: Public redistribution or downstream commercial/OS packaging performing automated license compliance review (e.g., REUSE / FOSSA / ScanCode).
Recommendation: Verify the upstream Samsung OSS tarball provenance for `socs/exynos850/drivers/scsc/`, include the exact upstream `COPYING`/`LICENSE.txt` alongside `socs/exynos850/drivers/scsc/` (or remove `cme3_prim.h` if not covered by Samsung's GPL release), add a root-level `LICENSING.md` or REUSE `DEP5` mapping that explicitly covers `socs/exynos850/drivers/`, `tools/cbd/`, and `tools/a21s-et520-id-probe/`, and add an SPDX header to `tools/cma-leak/leaktest.c`.
References: REUSE Specification 3.2, SPDX License List
```

### SC-008
```
ID: SC-008
Title: Unverified third-party `.so` binary downloaded from mutable GitHub release tag without `-f` or SHA-256 check
Severity: Medium
Confidence: Confirmed
Location: scripts/setup_panvk_test.sh:30-40
Evidence:
30: BASE_URL="https://github.com/zenithblue-oss/panvk-kbase-android/releases/download/panvk-kbase-v26.3.1"
31:
32: if [[ ! -f "$STAGING/libvulkan_panfrost-android-aarch64.so" ]]; then
33:   echo "Downloading libvulkan_panfrost-android-aarch64.so..."
34:   curl -sL -o "$STAGING/libvulkan_panfrost-android-aarch64.so" "${BASE_URL}/libvulkan_panfrost-android-aarch64.so"
35: fi
36:
37: if [[ ! -f "$STAGING/panfrost_icd.aarch64.json" ]]; then
38:   echo "Downloading panfrost_icd.aarch64.json..."
39:   curl -sL -o "$STAGING/panfrost_icd.aarch64.json" "${BASE_URL}/panfrost_icd.aarch64.json"
40: fi
Impact: Downloads a pre-compiled native shared library (`libvulkan_panfrost-android-aarch64.so`) from a third-party GitHub release tag (which can be overwritten by the upstream maintainer or a compromised account) with no SHA-256 verification and pushes it to the device via `adb`. Omitting `-f` (`--fail`) also means HTTP 404/5xx error pages are silently written as `.so`/`.json` files and cached.
Exploit/Trigger: Running `./scripts/setup_panvk_test.sh` if the upstream release asset is replaced or unavailable.
Recommendation: Use `curl -fsSL`, pin expected SHA-256 checksums for both downloaded files, and verify them with `sha256sum -c` before `adb push`.
References: CWE-494 (Download of Code Without Integrity Check)
```

### SC-009
```
ID: SC-009
Title: Dockerfiles use unpinned base image tags and unpinned `apt-get` package versions
Severity: Medium
Confidence: Confirmed
Location: tools/rabbiz-de/Dockerfile:5-12 and tools/rabbiz-sdk/Dockerfile.emu:6-13
Evidence:
# tools/rabbiz-de/Dockerfile:5-12:
5: FROM docker.io/library/rust:1.99-bookworm
6: RUN dpkg --add-architecture arm64 && apt-get update && apt-get install -y --no-install-recommends \
7:       gcc-aarch64-linux-gnu pkg-config \
...
# tools/rabbiz-sdk/Dockerfile.emu:6-13:
6: FROM rabbiz-de-build:arm64
8: RUN apt-get update && apt-get install -y --no-install-recommends \
9:     dbus \
10:     fonts-noto-core \
Impact: `docker.io/library/rust:1.99-bookworm` is a mutable tag rather than an immutable `@sha256:` image digest, and Debian packages are installed without version pins. Rebuilding the ARM64 cross-compile container or Tier-2 emulator image at a later time pulls different base layers and system libraries, breaking build reproducibility and increasing exposure to upstream tag compromise.
Exploit/Trigger: Building `tools/rabbiz-de/Dockerfile` or `tools/rabbiz-sdk/Dockerfile.emu` on different dates or after an upstream image tag update.
Recommendation: Pin `FROM docker.io/library/rust:1.99-bookworm@sha256:<digest>` in `tools/rabbiz-de/Dockerfile` and consider snapshot/pinned Debian package versions for release builds.
References: CWE-1357 (Reliance on Insufficiently Trustworthy Component), CIS Docker Benchmark 4.7
```

### SC-010
```
ID: SC-010
Title: No SBOM generation or automated vulnerability/license policy scanning (`cargo-audit`, `cargo-deny`)
Severity: Low
Confidence: Confirmed
Location: .github/workflows/host-checks.yml:1-58 (repo-wide absence)
Evidence:
$ find . -maxdepth 4 \( -iname '*sbom*' -o -iname '*cyclonedx*' -o -iname '*spdx*' -o -iname 'deny.toml' -o -iname 'audit.toml' -o -iname '.cargo-audit*' \) -not -path './.git/*' -not -path './audit/*'
./src/linux/scripts/sbom
./src/linux/scripts/spdxcheck-test.sh
./src/linux/scripts/spdxcheck.py
./src/linux/scripts/spdxexclude
# (All matches are inside gitignored upstream `src/linux`; zero SBOM or dependency-audit configs exist in RabbizOS.)

# Total locked Rust package entries across 6 workspaces:
tools/rabbiz-de/Cargo.lock: 362
tools/rabbiz-apps/Cargo.lock: 346
tools/rabbiz-sdk/Cargo.lock: 307
tools/rabbiz-camera/Cargo.lock: 188
tools/rabbiz-osk/Cargo.lock: 184
tools/rabbiz-mirror/linux/Cargo.lock: 75
Impact: Across 6 Rust workspaces totaling 1,462 locked package entries (plus system Arch Linux ARM packages and vendored C drivers), there is no CycloneDX/SPDX SBOM and no automated RustSec advisory or license-compatibility check. Known vulnerabilities in transitive Rust crates (e.g., `nix 0.24.3`, `png`, `zbus`, `flate2`) cannot be tracked or alerted automatically.
Exploit/Trigger: A CVE is disclosed in a transitive crate or system package and goes unnoticed across releases.
Recommendation: Add a workspace-shared `deny.toml` and run `cargo deny check advisories licenses bans sources` (or `cargo audit`) in CI across all Rust workspaces, and generate CycloneDX SBOMs (`cargo cyclonedx` / `syft`) on release tags.
References: Executive Order 14028 (SBOM), NTIA Minimum Elements for an SBOM, CWE-1357
```

### SC-011
```
ID: SC-011
Title: Minor manifest/lockfile hygiene gaps (`rabbiz-search` missing license, `s2-render` unlocked, `pacman -Sy` partial-upgrade instruction)
Severity: Low
Confidence: Confirmed
Location: tools/rabbiz-apps/crates/rabbiz-search/Cargo.toml:1-8, tools/rabbiz-osk/spikes/s2-render/Cargo.toml:1-6, .gitignore:21, scripts/deploy_shell.sh:87,231
Evidence:
# tools/rabbiz-apps/crates/rabbiz-search/Cargo.toml:1-8 (omits `license.workspace = true`):
1: [package]
2: name = "rabbiz-search"
3: description = "Fast native app + web search for RabbizOS"
4: version.workspace = true
5: edition.workspace = true
6: rust-version.workspace = true
7: publish.workspace = true

# tools/rabbiz-osk/spikes/s2-render/Cargo.toml:1-6 (excluded from workspace, no license, Cargo.lock gitignored at .gitignore:21):
1: [package]
2: name = "osk-spike-render"
3: version = "0.0.1"
4: edition = "2021"
5: publish = false

# scripts/deploy_shell.sh:87,231 (partial upgrade anti-pattern on Arch Linux):
87:   echo "  pacman -Sy --needed ${PACKAGES[*]}"
231:        pacman -Sy --needed labwc quickshell qt6-declarative qt6-svg \
Impact:
1. `rabbiz-search` omits `license.workspace = true` unlike all other 14 member crates in `tools/rabbiz-apps`, breaking automated license metadata extraction.
2. `tools/rabbiz-osk/spikes/s2-render` has no `license` field and no committed `Cargo.lock`.
3. `tools/rabbiz-mirror/linux/` (including its `Cargo.lock`) is currently untracked in git (`git ls-files tools/rabbiz-mirror/linux/` is empty — work in progress).
4. `pacman -Sy --needed` syncs the remote package database without upgrading installed packages, which is a known Arch Linux partial-upgrade hazard that can break ABI compatibility across shared libraries.
Exploit/Trigger: Running automated license checks (`cargo metadata` / `cargo-deny`), building the spike crate, or executing the documented `pacman -Sy` command on a device with stale system libraries.
Recommendation: Add `license.workspace = true` to `tools/rabbiz-apps/crates/rabbiz-search/Cargo.toml`, add `license = "Apache-2.0"` to `tools/rabbiz-osk/spikes/s2-render/Cargo.toml` (or retire the spike if obsolete), commit `tools/rabbiz-mirror/linux/Cargo.lock` when committing that crate, and change `pacman -Sy --needed` to `pacman -Syu --needed` in `scripts/deploy_shell.sh`.
References: Arch Linux Wiki — System maintenance (Partial upgrades are unsupported)
```

---

## Positive Observations

1. **Clean Rust dependency hygiene (0 git deps, 0 wildcard deps, 100% checksummed lockfiles):**
   Direct `tomllib` inspection of all 47 Rust `Cargo.toml` files and all 6 `Cargo.lock` files (`tools/rabbiz-{de,apps,osk,sdk,camera}/Cargo.lock` and `tools/rabbiz-mirror/linux/Cargo.lock`) confirmed zero `git =` dependencies, zero wildcard (`*`) version constraints, lockfile format v4 across all workspaces, 100% `crates.io` registry sources, 100% SHA-256 checksums on every external package, and zero workspace-member lockfile drift.
2. **SHA-256 verified FEX rootfs download:**
   `scripts/install_fex_rootfs.sh:15-16,28` pins a dated URL (`https://rootfs.fex-emu.gg/ArchLinux/2026-08-11/ArchLinux.sqsh`) and enforces `sha256sum -c -` against a hardcoded `SQSH_SHA256` before mounting.
3. **Zero external SwiftPM dependencies:**
   `tools/rabbiz-mirror/Package.swift:4-17` declares zero third-party packages (only the local `RabbizMirror` executable target), eliminating SwiftPM supply-chain surface (and correctly requiring no `Package.resolved`).
4. **Explicit kernel-driver licensing boundary documented:**
   `socs/exynos850/drivers/LICENSING.md:1-16` clearly separates upstream GPL-2.0 kernel drivers from the repository's `Apache-2.0` license and explicitly notes the missing-SPDX gap.
5. **Workspace-wide `unsafe_code = "warn"` lint:**
   `tools/rabbiz-de/Cargo.toml:96`, `tools/rabbiz-apps/Cargo.toml:29`, `tools/rabbiz-osk/Cargo.toml:14`, and `tools/rabbiz-sdk/Cargo.toml:22` all enable `unsafe_code = "warn"` at the workspace level.

---

## Cross-Domain Leads
- **Domain 6/7 (Userland / Apps):** `tools/rabbiz-apps/crates/rabbiz-store/src/backend.rs` wraps `paru`/`pacman`/`flatpak`/`rabbiz-updater` — verify privilege escalation, argument injection, and package verification in the Store app.
- **Domain 5 (Host Scripts / Device Ops):** `scripts/mirror_setup.sh:75,161-162` supports `--lan` (`wayvnc` on `0.0.0.0:5900` with no authentication) and `scripts/install_chrome_arm.sh:24` uses fixed `/tmp/chrome-extract` on the device.

---

## Reviewed Files Machine List

```files
audit/tools/supply-chain-scan.txt|reviewed|direct-read|Phase 3 supply-chain triage input
audit/tools/binary-blobs.txt|reviewed|direct-read|Phase 3 binary-blobs triage input
LICENSE|reviewed|targeted-read|Verified root Apache-2.0 license
.gitignore|reviewed|direct-read|Verified vendor/smithay and Cargo.lock ignore rules
.github/workflows/host-checks.yml|reviewed|direct-read|Audited CI action pinning, permissions, and workspace coverage
tools/rabbiz-de/Cargo.toml|reviewed|direct-read|Audited workspace deps, license, and vendor/smithay patch
tools/rabbiz-de/Cargo.lock|reviewed|python3-parse|Verified lockfile v4, crates.io sources, checksums, member sync
tools/rabbiz-de/Dockerfile|reviewed|direct-read|Audited base image and apt pinning
tools/rabbiz-de/rust-toolchain.toml|reviewed|direct-read|Checked toolchain pin vs CI
tools/rabbiz-de/vendor/smithay/Cargo.toml|reviewed|targeted-read|Verified MIT license and untracked git status
tools/rabbiz-apps/Cargo.toml|reviewed|direct-read|Audited workspace deps and license
tools/rabbiz-apps/Cargo.lock|reviewed|python3-parse|Verified lockfile v4, crates.io sources, checksums, member sync
tools/rabbiz-apps/crates/rabbiz-search/Cargo.toml|reviewed|direct-read|Confirmed missing license.workspace = true
tools/rabbiz-osk/Cargo.toml|reviewed|direct-read|Audited workspace deps and license
tools/rabbiz-osk/Cargo.lock|reviewed|python3-parse|Verified lockfile v4, crates.io sources, checksums, member sync
tools/rabbiz-osk/spikes/s2-render/Cargo.toml|reviewed|direct-read|Confirmed missing license and excluded/unlocked spike
tools/rabbiz-osk/dict-build/build_emoji.py|reviewed|targeted-read|Checked external download behavior (offline raw inputs)
tools/rabbiz-sdk/Cargo.toml|reviewed|direct-read|Audited workspace deps and license
tools/rabbiz-sdk/Cargo.lock|reviewed|python3-parse|Verified lockfile v4, crates.io sources, checksums, member sync
tools/rabbiz-sdk/Dockerfile.emu|reviewed|direct-read|Audited emulator Dockerfile base image and apt pinning
tools/rabbiz-sdk/packages/rabbiz_sdk_dart/pubspec.yaml|reviewed|direct-read|Checked Dart SDK dependencies
tools/rabbiz-camera/Cargo.toml|reviewed|direct-read|Audited crate deps and license
tools/rabbiz-camera/Cargo.lock|reviewed|python3-parse|Verified lockfile v4, crates.io sources, checksums, member sync
tools/rabbiz-mirror/Package.swift|reviewed|direct-read|Verified zero external SwiftPM dependencies
tools/rabbiz-mirror/linux/Cargo.toml|reviewed|direct-read|Audited crate deps and license
tools/rabbiz-mirror/linux/Cargo.lock|reviewed|python3-parse|Verified lockfile v4, checksums, and untracked git status
socs/exynos850/drivers/scsc/scsc_bt.ko.zst|reviewed|binary-inspect|Verified tracked, unsigned, vermagic 7.3.0-g14a425b3a7dc
socs/exynos850/drivers/scsc/scsc_mx.ko.zst|reviewed|binary-inspect|Verified tracked, unsigned, vermagic 7.3.0-g14a425b3a7dc
socs/exynos850/drivers/scsc/scsc_platform_mif.ko.zst|reviewed|binary-inspect|Verified tracked, unsigned, vermagic 7.3.0-g14a425b3a7dc
socs/exynos850/drivers/scsc/scsc_wlan.ko.zst|reviewed|binary-inspect|Verified tracked, unsigned, vermagic 7.3.0-g14a425b3a7dc
socs/exynos850/drivers/LICENSING.md|reviewed|direct-read|Verified driver licensing policy and no-binary rule
socs/exynos850/drivers/scsc/include/scsc/api/cme3_prim.h|reviewed|targeted-read|Confirmed Samsung Confidential header and missing LICENSE.txt
socs/exynos850/drivers/scsc/bt/scsc_bluez.c|reviewed|targeted-read|Verified All rights reserved header without SPDX
tools/cbd/a21s-cbd.c|reviewed|targeted-read|Verified GPL-2.0 SPDX header
tools/cbd/Makefile|reviewed|direct-read|Verified GPL-2.0 SPDX header
tools/a21s-et520-id-probe/et520-id-probe.c|reviewed|targeted-read|Verified GPL-2.0 SPDX header
tools/cma-leak/leaktest.c|reviewed|targeted-read|Confirmed missing license header
scripts/mirror_setup.sh|reviewed|direct-read|Confirmed curl -skL + unverified pacman -U as root
scripts/install_chrome_arm.sh|reviewed|direct-read|Confirmed unpinned unverified deb extracted to / as root
scripts/setup_panvk_test.sh|reviewed|direct-read|Confirmed unverified third-party .so download without -f or SHA-256
scripts/install_fex_rootfs.sh|reviewed|direct-read|Confirmed pinned URL and SHA-256 verification
scripts/bootstrap_build_env.sh|reviewed|direct-read|Confirmed short commit hashes and silent || true on uniLoader checkout
scripts/deploy_shell.sh|reviewed|targeted-read|Confirmed pacman -Sy partial-upgrade instruction
```
