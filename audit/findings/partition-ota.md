# Domain Audit: Partition & OTA (`OTA`)

## Scope & Files Reviewed

### Files Inspected
- `rootfs-overlay/etc/udev/rules.d/99-hide-system-partitions.rules` (`direct-read`)
- `devices/xiaomi/muyu/scripts/backup_muyu_stock.sh` (`direct-read`)
- `.gitignore` (`direct-read`)
- `tools/rabbiz-apps/crates/rabbiz-store/src/backend.rs` (`direct-read`)
- `tools/rabbiz-apps/crates/rabbiz-store/src/main.rs` (`targeted-read`: lines 125–170, 400–920, 2080–2220 + `rg-scan`)
- `tools/rabbiz-sdk/crates/rabbiz-pack/Cargo.toml` (`direct-read`)
- `tools/rabbiz-sdk/crates/rabbiz-pack/src/lib.rs` (`direct-read`)
- `tools/rabbiz-sdk/crates/rabbiz-pack/src/sha256.rs` (`direct-read`)
- `tools/rabbiz-sdk/crates/rabbiz-cli/src/lib.rs` (`targeted-read`: lines 755–820)
- `scripts/de_deploy.sh` (`direct-read`)
- `scripts/de_apps_deploy.sh` (`direct-read`)
- `scripts/deploy_shell.sh` (`direct-read`)
- `scripts/install_chrome_arm.sh` (`direct-read`)
- `scripts/install_fex_rootfs.sh` (`direct-read`)
- `scripts/mirror_setup.sh` (`direct-read`)
- `scripts/build_emu_image.sh` (`direct-read`)
- `scripts/bootstrap_build_env.sh` (`direct-read`)
- `scripts/setup_qt_scanout.sh` (`direct-read`)
- `scripts/setup_dumb_destroy_fix.sh` (`direct-read`)
- `scripts/device_session.sh` (`direct-read`)
- `devices/samsung/a21s/docs/a21s-flash-boot-checklist.md` (`direct-read`)
- `devices/samsung/a21s/docs/a21s-boot-recovery.md` (`direct-read`)
- `devices/samsung/a21s/docs/a21s-pit.txt` (`targeted-read`: lines 1–120 + `rg-scan` of all 42 partition names)

### Files Not Inspected (and Reason)
- `tools/rabbiz-apps/crates/rabbiz-store/src/main.rs` UI rendering / layout code (lines 1–124, 171–399, 921–2079, 2221–2366): skipped because they cover canvas painting, hit-testing, and tests; package/update execution paths were located via `rg-scan` and read directly.
- `scripts/` diagnostic/benchmark/input scripts (`a21s-power-diag.sh`, `abox_*.sh`, `frame_time.sh`, `volume-keys.py`, `xbox-ble-uinput.py`, etc.): out of scope for Partition & OTA domain.
- External `src/linux`, `src/uniLoader`, and device `/etc/pacman.conf` (not tracked in this repository — `.gitignore:3-4`).

---

## Findings

### OTA-001
```
ID: OTA-001
Title: Unauthenticated Arch Linux ARM package download with TLS verification disabled (curl -k) installed as root
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
Impact: Arbitrary root code execution on the target device via network MITM on the host (TLS certificate validation is disabled via `-k` and neither `.sig` signatures nor SHA-256 checksums are verified) or via local privilege escalation on the device (predictable `/tmp/*.pkg.tar.xz` filenames can be pre-created/swapped by the unprivileged `rabbiz` user before `pacman -U` runs as root). Note that Arch Linux ARM's default `pacman.conf` sets `LocalFileSigLevel = Optional`, so `pacman -U` installs unsigned local packages without complaint.
Exploit/Trigger: A MITM attacker between the developer host and `mirror.archlinuxarm.org` serves a trojanized `.pkg.tar.xz` (e.g., with a `.INSTALL` post_install root hook) when `scripts/mirror_setup.sh` runs; or a local `rabbiz` process races `/tmp/wayvnc-0.10.2-1-aarch64.pkg.tar.xz` between `scp` and `pacman -U`.
Recommendation: Remove `-k` from `curl` (use `curl -fsSL`), pin SHA-256 checksums (or download and verify `.pkg.tar.xz.sig` against the Arch Linux ARM keyring), stage packages in a root-only `mktemp -d /root/mirror-pkgs.XXXXXX` directory (not `/tmp`), and clean up the staging directory via `trap`.
References: CWE-295 (Improper Certificate Validation), CWE-494 (Download of Code Without Integrity Check), CWE-377 (Insecure Temporary File)
```

### OTA-002
```
ID: OTA-002
Title: No cryptographic signature or authenticity verification on .rbz application bundles (self-checksum only + TOCTOU)
Severity: High
Confidence: Confirmed
Location: tools/rabbiz-sdk/crates/rabbiz-pack/src/lib.rs:358-426, 459-482
Evidence:
  389:         let expected_hash_bytes = read_slice(raw, &mut pos, 32)?;
  390:         let data_len = read_u64_be(raw, &mut pos)? as usize;
  391:         let data = read_slice(raw, &mut pos, data_len)?.to_vec();
  392: 
  393:         let actual_hash_bytes = sha256_bytes(&data);
  394:         if actual_hash_bytes.as_slice() != expected_hash_bytes {
  395:             return Err(format!(
  396:                 "SHA-256 integrity mismatch for file {rel_path:?} in .rbz archive"
  397:             ));
  398:         }
  ...
  459: pub fn install_rbz_local(rbz_file: &Path, data_home: &Path) -> Result<PathBuf, String> {
  460:     let raw = std::fs::read(rbz_file)
  461:         .map_err(|e| format!("read {}: {e}", rbz_file.display()))?;
  462:     let parsed = parse_and_verify_rbz_bytes(&raw)?;
  463:     let app_id = &parsed.metadata.manifest.app.id;
  464: 
  465:     let install_dir = data_home.join("rabbiz").join("apps").join(app_id);
  466:     let manifest = unpack_rbz(rbz_file, &install_dir)?;
Impact: `verify_rbz` and `install_rbz_local` only check that each embedded file matches a SHA-256 hash stored inside the same untrusted `.rbz` archive, and the manifest TOML itself is not even hashed. Any attacker can modify binaries or `rabbiz.toml` inside an `.rbz` bundle, recompute the embedded SHA-256 hashes (or repack with `pack_rbz`), and pass verification with zero publisher authenticity, trust-root, or downgrade/anti-rollback checks. Additionally, `install_rbz_local` reads `rbz_file` twice (`lib.rs:460` and again in `unpack_rbz` at `lib.rs:430`), introducing a TOCTOU window if the file is modified between validation and extraction.
Exploit/Trigger: Distribute a tampered `.rbz` bundle (or replace an existing app's `.rbz` with a backdoored version of the same `app.id`) and have a user or tool install it via `rabbiz install` / `install_rbz_local`.
Recommendation: Add asymmetric detached or envelope signatures (e.g., Ed25519 over the manifest + file table + per-file hashes, including a monotonic version/epoch field for anti-rollback) verified against a pinned developer/store public keyring before unpacking. Refactor `install_rbz_local` to unpack directly from the already-parsed `ParsedRbz` in memory to eliminate the double-read TOCTOU.
References: CWE-345 (Insufficient Verification of Data Authenticity), CWE-347 (Improper Verification of Cryptographic Signature), CWE-367 (TOCTOU)
```

### OTA-003
```
ID: OTA-003
Title: Path traversal, arbitrary chmod 0755, and .desktop INI injection in rabbiz-pack manifest handling
Severity: High
Confidence: Confirmed
Location: tools/rabbiz-sdk/crates/rabbiz-pack/src/lib.rs:158-208, 465-480
Evidence:
  158:     pub fn validate(&self) -> Result<(), String> {
  159:         let id = self.app.id.trim();
  160:         if id.is_empty() {
  161:             return Err("app.id must not be empty".to_string());
  162:         }
  163:         if id.contains('/') || id.contains('\\') || id.chars().any(char::is_whitespace) {
  164:             return Err(format!("invalid app.id {id:?}: must not contain slashes or whitespace"));
  165:         }
  ...
  172:         if self.app.exec.trim().is_empty() {
  173:             return Err("app.exec must not be empty".to_string());
  174:         }
  175:         Ok(())
  176:     }
  ...
  198:         format!(
  199:             "[Desktop Entry]\nType=Application\nName={}\nExec={}\nIcon={}\nCategories={}\nTerminal=false\nStartupWMClass={}\nX-Rabbiz-Kind={}\nX-Rabbiz-MCP={}\n",
  200:             self.app.name,
  201:             install_bin_path.display(),
  202:             icon_val,
  203:             cats,
  ...
  468:     let bin_path = install_dir.join(&manifest.app.exec);
  469:     if bin_path.exists() {
  470:         set_executable_bit(&bin_path);
  471:     }
Impact:
1. `app.id` is not checked against `".."` or `"."`; setting `app.id = ".."` escapes `<data_home>/rabbiz/apps/<id>` to `<data_home>/rabbiz/` during unpack (`lib.rs:465`).
2. `app.exec` is not validated against absolute paths, `..` components, or whitespace/control characters. In Rust, `install_dir.join("/abs/path")` replaces `install_dir` entirely, causing `set_executable_bit(&bin_path)` (`lib.rs:470`) to `chmod | 0o755` an arbitrary user-writable file and emit `Exec=/abs/path` in the `.desktop` entry.
3. `app.name`, `app.exec`, `app.icon`, and `app.categories` are interpolated raw into the `.desktop` INI file without rejecting `\n`/`\r`, enabling arbitrary `.desktop` key injection (e.g., overriding `Exec=`, adding `Path=`, or injecting a second desktop action).
Exploit/Trigger: Install a crafted `.rbz` archive containing a malicious `rabbiz.toml` via `rabbiz install` or `install_rbz_local` (also exposed via the `emu_pack_install` MCP tool in `tools/rabbiz-sdk/crates/rabbiz-cli/src/lib.rs:755-781`).
Recommendation: Enforce strict allowlists in `AppManifest::validate`:
- `app.id`: reverse-DNS ASCII `[a-zA-Z0-9._-]+`, rejecting `.` and `..` and leading/trailing dots.
- `app.exec` and `app.icon`: relative paths with normal components only (`Path::components()` all `Component::Normal`), no `..`, no leading `/`, no control characters or whitespace.
- Reject `\r` and `\n` (and NUL) in all manifest string fields (`name`, `version`, `exec`, `icon`, `categories`).
References: CWE-22 (Path Traversal), CWE-74 (Improper Neutralization of Special Elements in Output / INI Injection)
```

### OTA-004
```
ID: OTA-004
Title: Predictable world-writable /tmp and /var/tmp staging paths in root deployment scripts (LPE rabbiz -> root)
Severity: High
Confidence: Confirmed
Location: scripts/de_deploy.sh:34-74, scripts/de_apps_deploy.sh:116-131, scripts/install_chrome_arm.sh:22-28, scripts/setup_dumb_destroy_fix.sh:6-9
Evidence:
  # scripts/de_deploy.sh:34-49, 55-64
  34: scp -q "$BIN" "$HOST:/tmp/rabbiz-de.new"
  ...
  41: scp -q "$OVL/etc/systemd/system/rabbiz-session.service" \
  42:     "$HOST:/tmp/rabbiz-session.service.new"
  ...
  55: install -m 0755 -o root -g root /tmp/rabbiz-de.new /usr/local/bin/rabbiz-de
  ...
  61: install -m 0644 -o root -g root /tmp/rabbiz-session.service.new \
  62:   /etc/systemd/system/rabbiz-session.service

  # scripts/de_apps_deploy.sh:116-130
  116: tmp_remote=/tmp/rabbiz-apps-bin.$$
  117: ssh "$HOST" "mkdir -p $tmp_remote"
  118: for f in "${bins[@]}"; do
  119:   scp -q "$f" "$HOST:$tmp_remote/$(basename "$f").new"
  ...
  129:   install -m 0755 -o root -g root "$new" "/usr/local/bin/$name"

  # scripts/install_chrome_arm.sh:22-26
  22:   cd /tmp
  23:   wget -q --show-progress -O chrome-arm64.deb "'"${DEB_URL}"'"
  24:   mkdir -p /tmp/chrome-extract && cd /tmp/chrome-extract
  25:   bsdtar -xf /tmp/chrome-arm64.deb data.tar.xz
  26:   tar -xJf data.tar.xz --no-same-owner --no-same-permissions -C /

  # scripts/setup_dumb_destroy_fix.sh:6-9
  6: scp -q tools/qt-scanout/dumb-destroy.c "$host:/var/tmp/dumb-destroy.c"
  7: ssh "$host" 'set -eu
  8: install -d /usr/local/lib/a21s-qt-scanout
  9: gcc -shared -fPIC -O2 -Wall -Wextra -Werror /var/tmp/dumb-destroy.c -o /usr/local/lib/a21s-qt-scanout/dumb-destroy.so -ldl
Impact: Local privilege escalation from the unprivileged `rabbiz` user to `root` whenever any of these deployment scripts runs. Because `/tmp` and `/var/tmp` are world-writable, `rabbiz` can pre-create `/tmp/rabbiz-session.service.new`, `/tmp/rabbiz-de.new`, `/tmp/chrome-arm64.deb`, `/tmp/chrome-extract`, or `/var/tmp/dumb-destroy.c` owned by `rabbiz` (or pre-create `/tmp/rabbiz-apps-bin.<pid>` which `mkdir -p` silently accepts). When `scp`/`wget` overwrites the file in place, `rabbiz` retains ownership and can swap its contents before `install`/`tar`/`gcc` executes as `root`.
Exploit/Trigger: A compromised app running as `rabbiz` pre-creates the predictable `/tmp` or `/var/tmp` file(s) and overwrites them right after `scp`/`wget` completes, before the subsequent `ssh` session installs/compiles them as `root`.
Recommendation: Create a root-owned private staging directory on the device via `stage=$(ssh "$HOST" 'mktemp -d /root/rabbiz-deploy.XXXXXX')` (mode `0700`, owned by `root:root`) or stream files over `tar | ssh` into a root-owned `mktemp -d` directory as `scripts/setup_qt_scanout.sh:8` already does, and remove it in an `EXIT` trap.
References: CWE-377 (Insecure Temporary File), CWE-362 (Race Condition)
```

### OTA-005
```
ID: OTA-005
Title: Architectural Gap: No signed OTA update system, no A/B partitioning, and no rollback protection
Severity: High
Confidence: Confirmed
Location: devices/samsung/a21s/docs/a21s-pit.txt:370, devices/samsung/a21s/docs/a21s-flash-boot-checklist.md:79-96, devices/samsung/a21s/docs/a21s-boot-recovery.md:16,37-38, tools/rabbiz-apps/crates/rabbiz-store/src/main.rs:101,691,736-755
Evidence:
  # devices/samsung/a21s/docs/a21s-pit.txt:370 (single BOOT partition, no _a/_b slots)
  370: Partition Name: BOOT
  385: Partition Name: RECOVERY

  # devices/samsung/a21s/docs/a21s-boot-recovery.md:16, 37-38
  16: | tail | AVB: `AVB0` vbmeta (512 B) + `AVBf` footer. Hash descriptor: partition `boot`, sha256, `Algorithm: NONE`, rollback index 12. `partition_size = 37748736` (= `/dev/mmcblk0p18` = 73728 sectors) |
  ...
  37: avbtool add_hash_footer --image /tmp/boot.img --partition_name boot \
  38:   --partition_size 37748736 --rollback_index 12

  # devices/samsung/a21s/docs/a21s-flash-boot-checklist.md:92-96
  92: scp /tmp/boot-avb-new.img root@172.16.42.1:/root/
  93: ./scripts/device_session.sh root@172.16.42.1 <<'EOF'
  94: dd if=/root/boot-avb-new.img of=/dev/mmcblk0p18 bs=4M && sync
  95: systemctl reboot
  96: EOF

  # tools/rabbiz-apps/crates/rabbiz-store/src/main.rs:736-755 (no real rabbiz-updater binary exists)
  748:             self.kernel_release = "7.3.0-3-rabbiz-a21s".into();
  749:             self.status_banner = format!("Applied {applied} Linux & driver patches cleanly");
  750:             self.last_cmd_log = "rabbiz-updater apply-all && paru -Syu --noconfirm".into();
Impact:
1. **No OTA framework exists** (no RAUC, OSTree, Mender, SWUpdate, or `rabbiz-updater` binary anywhere in the repository; `rabbiz-updater` appears only as UI log strings in `rabbiz-store`).
2. **No A/B partitioning or atomic rootfs updates** on Galaxy A21s: there is a single `BOOT` partition (`/dev/mmcblk0p18`) and single writable rootfs (`pmos_root_uuid`). Kernel updates are manual `dd` overwrites over root SSH; a failed boot drops the device to Download Mode or black screen, requiring physical USB recovery via Heimdall (`a21s-flash-boot-checklist.md:118`).
3. **No cryptographic boot/OTA signing or rollback protection**: `boot-avb.img` uses an unsigned AVB hash footer (`Algorithm: NONE`) with a hardcoded `--rollback_index 12` on an unlocked bootloader. Userland updates rely on live `pacman`/`paru -Syu` or manual `scp`/`tar | ssh` scripts without atomic snapshots or automatic fallback.
4. **Misleading Store UX**: When `paru`/`apt-get` are absent, `rabbiz-store` simulates applying kernel/DTB updates locally and bumps `kernel_release` in `state.json` without touching `/dev/mmcblk0p18` or `/lib/modules`.
Exploit/Trigger: Operational & security architectural gap: any interrupted kernel flash bricks boot until physical Heimdall recovery; any rootfs upgrade failure leaves the single writable rootfs in a half-upgraded state; no cryptographic chain prevents downgrade or unsigned OS modification.
Recommendation: For production/release readiness, adopt an atomic image-based update architecture (e.g., RAUC or OSTree with signed bundles, Ed25519/RSA-PSS verification, and monotonic anti-rollback metadata). On A21s (single `BOOT` partition), implement at least a verified staging + recovery initramfs fallback or btrfs/LVM rootfs snapshots with boot-success counters; on Phase 2 `muyu` (which has native `boot_a`/`boot_b` slots as seen in `backup_muyu_stock.sh:78`), design A/B slot switching from day one. Remove simulated kernel-update mutations from `rabbiz-store` until a real `rabbiz-updater` daemon exists.
References: CWE-345 (Insufficient Verification of Data Authenticity), CWE-693 (Protection Mechanism Failure)
```

### OTA-006
```
ID: OTA-006
Title: Shell command injection via persisted store state & unreviewed --noconfirm AUR execution in rabbiz-store
Severity: Medium
Confidence: Confirmed
Location: tools/rabbiz-apps/crates/rabbiz-store/src/main.rs:431-447, 708-711, 790-796, 836-854; tools/rabbiz-apps/crates/rabbiz-store/src/backend.rs:118-119, 227-230, 263-271
Evidence:
  # tools/rabbiz-apps/crates/rabbiz-store/src/main.rs:431-447
  431:         if let Ok(raw) = std::fs::read_to_string(&state_path) {
  432:             if let Ok(saved) = serde_json::from_str::<PersistedStoreState>(&raw) {
  ...
  447:                     items: saved.items,

  # tools/rabbiz-apps/crates/rabbiz-store/src/main.rs:790-796, 837-842
  790:                     format!(
  791:                         "flatpak remote-add --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo && flatpak install -y --noninteractive flathub {}",
  792:                         item.id
  793:                     )
  ...
  837:                 let pkg = backend::map_arch_pkg(&item.id);
  838:                 let script = if want_install {
  839:                     format!("paru -S --noconfirm {pkg}")
  840:                 } else {
  841:                     format!("paru -Rns --noconfirm {pkg}")
  842:                 };

  # tools/rabbiz-apps/crates/rabbiz-store/src/backend.rs:118-119, 227-229
  118:         let spawn_res = Command::new("sh")
  119:             .args(["-c", &script])
  ...
  227:     let script = format!(
  228:         "#!/bin/sh\nexec rabbiz-terminal --title \"{name}\"\n"
  229:     );
Impact:
1. **Command injection via state file**: `PersistedStoreState` (`$XDG_DATA_HOME/rabbiz-store/state.json`) persists full `CatalogItem` structs (`id`, `name`, `source`). On reload, `item.id` is interpolated unsanitized into `sh -c` commands (`flatpak ... {}`, `paru -S --noconfirm {pkg}` where `map_arch_pkg` returns unknown `id` verbatim) and `item.name` is interpolated into a double-quoted shell script (`backend.rs:228`). Any process that can modify `state.json` (or future remote catalog sync feeding `items`) gains arbitrary shell execution when the user taps Install/Update or triggers `store_install` via MCP (`main.rs:2197-2220`).
2. **Unreviewed AUR execution**: `paru -S --noconfirm` and `paru -Syu --noconfirm` (`main.rs:708-710, 839`) automatically build and install community-submitted AUR PKGBUILDs (e.g., `box64-git`, `paru-bin`) without PKGBUILD review, turning any compromised AUR maintainer account into immediate code execution on the device.
Exploit/Trigger: Modify `$XDG_DATA_HOME/rabbiz-store/state.json` so a Flatpak/Paru item's `id` contains shell metacharacters (e.g., `htop; id > /tmp/pwn`) and trigger an install; or install/upgrade a compromised AUR package via `rabbiz-store`.
Recommendation: Never pass package names through `sh -c`; invoke `Command::new("flatpak")` / `Command::new("paru")` / `Command::new("apt-get")` directly with an argument array (`args([...])`) and validate package IDs against a strict character allowlist (`[a-zA-Z0-9._+-]+`). Persist only user state (`installed`, `version`) rather than trusting full catalog metadata from `state.json`. For AUR packages, avoid `--noconfirm` blind builds or restrict Store to curated signed repositories/Flatpak.
References: CWE-78 (OS Command Injection), CWE-494 (Download of Code Without Integrity Check)
```

### OTA-007
```
ID: OTA-007
Title: Unverified Google Chrome .deb downloaded from floating URL and extracted directly into / as root
Severity: Medium
Confidence: Confirmed
Location: scripts/install_chrome_arm.sh:17-26
Evidence:
  17: DEB_URL="https://dl.google.com/linux/direct/google-chrome-stable_current_arm64.deb"
  ...
  23:   wget -q --show-progress -O chrome-arm64.deb "'"${DEB_URL}"'"
  24:   mkdir -p /tmp/chrome-extract && cd /tmp/chrome-extract
  25:   bsdtar -xf /tmp/chrome-arm64.deb data.tar.xz
  26:   tar -xJf data.tar.xz --no-same-owner --no-same-permissions -C /
Impact: The script fetches a floating `current` `.deb` over HTTPS without verifying Google's GPG signing key (no `debsig-verify`/`dpkg-sig`/signed APT `Release` verification) and without a pinned SHA-256 digest, then unpacks `data.tar.xz` directly into `/` as `root`. Any compromise of the download mirror, CA, or `/tmp/chrome-arm64.deb` (see also OTA-004) results in arbitrary root filesystem overwrite.
Exploit/Trigger: Run `./scripts/install_chrome_arm.sh` when the upstream artifact or transport is compromised.
Recommendation: Verify the downloaded `.deb` against a pinned SHA-256 digest (matching `scripts/install_fex_rootfs.sh:16,28`) or verify Google's official APT repository `InRelease` GPG signature before extracting, and stage inside a root-owned `mktemp -d` directory.
References: CWE-494 (Download of Code Without Integrity Check), CWE-353 (Missing Support for Integrity Check)
```

### OTA-008
```
ID: OTA-008
Title: backup_muyu_stock.sh resolves wrong REPO_ROOT and writes unignored device serials and EFS/persist partitions inside the git repository
Severity: Medium
Confidence: Confirmed
Location: devices/xiaomi/muyu/scripts/backup_muyu_stock.sh:9-12, 61-92; .gitignore:7
Evidence:
  # devices/xiaomi/muyu/scripts/backup_muyu_stock.sh:9-12
   9: REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
  10: BACKUP_DIR="${REPO_ROOT}/backups/xiaomi-pad-7-pro"
  11: INFO_DIR="${BACKUP_DIR}/device-info"
  12: PART_DIR="${BACKUP_DIR}/stock-rom/partitions"
  ...
  61: cat > "${INFO_DIR}/summary.txt" <<EOF
  62: Serial: ${TARGET_SERIAL}
  ...
  84:         persist modemst1 modemst2 fsg fsc

  # .gitignore:7
   7: /backups/
Impact: Because the script lives 3 directories below the repo root (`devices/xiaomi/muyu/scripts/`), `cd "$(dirname "${BASH_SOURCE[0]}")/.."` resolves to `devices/xiaomi/muyu/` rather than the repository root. Meanwhile `.gitignore:7` anchors `/backups/` to the repository root only. Running `backup_muyu_stock.sh` therefore writes `summary.txt` (hardware serial), full `getprop.txt`, and raw dumps of `persist`, `modemst1`, `modemst2`, `fsg`, and `fsc` (containing IMEI, Wi-Fi/Bluetooth MAC addresses, and per-device calibration/keystore data) into an **unignored** path (`devices/xiaomi/muyu/backups/xiaomi-pad-7-pro/`), creating a high risk of accidentally committing sensitive device identifiers and NV partitions to git (violating `AGENTS.md`'s no-secret/serial/IMEI/MAC rule).
Exploit/Trigger: Run `./devices/xiaomi/muyu/scripts/backup_muyu_stock.sh` and subsequently run `git add .` / `git commit`.
Recommendation: Fix `REPO_ROOT` to `"$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"` (or use `git rev-parse --show-toplevel`) so backups land in the gitignored top-level `/backups/` directory, and also add `**/backups/` to `.gitignore` as defense in depth.
References: CWE-200 (Exposure of Sensitive Information to an Unauthorized Actor), CWE-538 (Insertion of Sensitive Information into Externally-Accessible File or Directory)
```

### OTA-009
```
ID: OTA-009
Title: deploy_shell.sh extracts rootfs-overlay directly into / without --no-same-owner and without acquiring the device lock
Severity: Medium
Confidence: Confirmed
Location: scripts/deploy_shell.sh:159-160, 182-187
Evidence:
  159: COPYFILE_DISABLE=1 tar -C "${REPO_ROOT}/rootfs-overlay" -cf - . \
  160:   | ssh "${TARGET}" "tar -C / -xf -"
  ...
  182:   # tar preserves the macOS uids baked into the archive (501/20), which
  183:   # leaves /, /home and /home/rabbiz owned by a non-existent uid and breaks
  184:   # runuser/su + systemd path-transition checks. Restore real owners.
  185:   chown root:root / /home 2>/dev/null || true
  186:   chown rabbiz:rabbiz /home/rabbiz /home/rabbiz/.local /home/rabbiz/.local/share 2>/dev/null || true
  187:   chown -R rabbiz:rabbiz /home/rabbiz/.config
Impact: Extracting `tar -C / -xf -` as `root` without `--no-same-owner` transiently reassigns ownership of `/`, `/home`, `/etc`, `/usr`, and `/home/rabbiz` to the developer host's UID/GID (e.g., `501:20` on macOS, or `1000:1000` on Linux — which may collide with `rabbiz`'s UID `1000` on the device, momentarily making `/` and `/etc` owned by the unprivileged `rabbiz` user!). If SSH drops between line 160 and line 185, the device is left with broken root ownership (breaking `su`/`runuser`/systemd) or `rabbiz`-owned system directories. Additionally, `deploy_shell.sh` does not serialize through `scripts/device_session.sh`, risking concurrent mutation races with `de_deploy.sh` or `de_apps_deploy.sh`.
Exploit/Trigger: Run `./scripts/deploy_shell.sh` from a Linux host with UID 1000 (matching `rabbiz`) or interrupt SSH between lines 160 and 185.
Recommendation: Pass `--no-same-owner --no-same-permissions` (or `--owner=0 --group=0` on archive creation) to `tar`, explicitly `chown rabbiz:rabbiz` only `/home/rabbiz/...` paths, and execute the remote deploy steps inside `scripts/device_session.sh`.
References: CWE-732 (Incorrect Permission Assignment for Critical Resource), CWE-362 (Race Condition)
```

### OTA-010
```
ID: OTA-010
Title: System partition udev rule hides partitions in UI only (UDISKS_IGNORE) without read-only protection or UFS device coverage
Severity: Low
Confidence: Confirmed
Location: rootfs-overlay/etc/udev/rules.d/99-hide-system-partitions.rules:1-12
Evidence:
   5: # UDISKS_IGNORE only affects udisks' presentation; mounting still works.
   6: ACTION=="remove", GOTO="a21s_hide_end"
   7: SUBSYSTEM!="block", GOTO="a21s_hide_end"
   8: 
   9: KERNEL=="mmcblk0p*", ENV{UDISKS_IGNORE}="1"
  10: KERNEL=="loop0p*",  ENV{UDISKS_IGNORE}="1"
Impact: While the rule correctly documents that `UDISKS_IGNORE="1"` only hides raw Android/Samsung partitions from udisks file-manager sidebars, critical firmware/NV partitions (`BOOTLOADER`, `EFS`, `SEC_EFS`, `VBMETA`, `BOOT`, `KEYSTORAGE` in `a21s-pit.txt`) are not marked read-only at the kernel block layer (`ATTR{ro}="1"` / `blockdev --setro`). Any accidental root `dd` or misconfigured tool can still overwrite `EFS` (permanently losing IMEI) or `BOOTLOADER` (hard-bricking the device). Furthermore, `KERNEL=="mmcblk0p*"` is eMMC-specific and will not cover UFS LUN partitions (`sd[a-f]*`) on Phase 2 Xiaomi Pad 7 Pro (`muyu`).
Exploit/Trigger: Accidental write by root/script to `/dev/mmcblk0p*` (e.g., wrong partition index during kernel flash) or deploying the shared overlay to a UFS-based device.
Recommendation: Add udev rules (keyed by `ENV{ID_PART_ENTRY_NAME}` for `BOOTLOADER`, `EFS`, `SEC_EFS`, `CPEFS`, `KEYSTORAGE`, `VBMETA`, `PIT`, etc.) that set `RUN+="/usr/bin/blockdev --setro /dev/%k"` so non-updateable firmware/NV partitions are kernel-enforced read-only during normal boots. Extend `UDISKS_IGNORE` matching when `muyu` UFS support lands.
References: CWE-732 (Incorrect Permission Assignment for Critical Resource)
```

### OTA-011
```
ID: OTA-011
Title: Declared [permissions] in rabbiz.toml are never enforced on .rbz installation
Severity: Info
Confidence: Confirmed
Location: tools/rabbiz-sdk/crates/rabbiz-pack/src/lib.rs:99-110, 459-482
Evidence:
  101: pub struct PermissionsSection {
  102:     #[serde(default)]
  103:     pub network: bool,
  104:     #[serde(default)]
  105:     pub notifications: bool,
  106:     #[serde(default)]
  107:     pub intents: bool,
  108:     #[serde(default = "default_true")]
  109:     pub storage: bool,
  110: }
  ...
  476:     let desktop_path = apps_dir.join(format!("{app_id}.desktop"));
  477:     let desktop_content = manifest.generate_desktop_entry(&bin_path);
  478:     std::fs::write(&desktop_path, desktop_content)
Impact: `rabbiz.toml` invites developers and users to expect permission-gated sandboxing (`network = false`, `storage = false`, etc.), but `install_rbz_local` never reads `manifest.permissions` and generates a plain `.desktop` file that executes the installed binary directly with full `rabbiz` session user privileges (full network, filesystem, D-Bus, and Wayland access).
Exploit/Trigger: Any `.rbz` app declaring `network = false` and `storage = false` can still freely access the network and all files under `/home/rabbiz`.
Recommendation: Clearly document in `rabbiz-pack` and SDK docs that `[permissions]` is currently advisory/unimplemented until a sandbox runner (e.g., `bwrap` / `systemd-run` / Landlock / seccomp) wraps `Exec=` for `.rbz` apps.
References: CWE-265 (Privilege / Sandbox Issues)
```

---

## Positive Observations

1. **Pinned SHA-256 verification on FEX guest rootfs download**: `scripts/install_fex_rootfs.sh:16,28` pins `SQSH_SHA256` and verifies `/usr/share/fex-emu/RootFS/ArchLinux.sqsh` with `sha256sum -c -` before enabling `fex-guestos.service`.
2. **Safe staging pattern in `setup_qt_scanout.sh`**: `scripts/setup_qt_scanout.sh:8-10` uses `mktemp -d /tmp/a21s-qt-scanout.XXXXXX` with an `EXIT` cleanup trap and `tar --no-same-owner`, providing a clean template for fixing OTA-004 and OTA-009.
3. **Arch Linux ARM `/lib -> usr/lib` symlink guardrails**: Both `scripts/deploy_shell.sh:116-119` and `scripts/build_emu_image.sh:113-116` explicitly refuse to proceed if `rootfs-overlay/lib` exists, preventing boot-breaking rootfs corruption.
4. **Memory-safe `.rbz` parser with bounds checks**: `tools/rabbiz-sdk/crates/rabbiz-pack/src/lib.rs:5,372,385` enforces `#![forbid(unsafe_code)]`, bounds `file_count <= 16384`, uses checked slice arithmetic (`read_slice`), and blocks `..` and leading `/` in archive member `rel_path` entries.
5. **Serialized device mutation lock**: `scripts/device_session.sh:10-20` uses `flock` on `/run/lock/a21s-device-test.lock` with a bounded timeout to prevent concurrent deployment corruption.

---

## Reviewed Files Machine List

```files
rootfs-overlay/etc/udev/rules.d/99-hide-system-partitions.rules|reviewed|direct-read|udev partition hiding rule
devices/xiaomi/muyu/scripts/backup_muyu_stock.sh|reviewed|direct-read|muyu stock partition backup script
.gitignore|reviewed|direct-read|verified anchored /backups/ vs backup_muyu_stock.sh path
tools/rabbiz-apps/crates/rabbiz-store/src/backend.rs|reviewed|direct-read|package manager and curated rbz install backend
tools/rabbiz-apps/crates/rabbiz-store/src/main.rs|reviewed|targeted-read|state persistence, update/install command dispatch, CLI/MCP handlers
tools/rabbiz-sdk/crates/rabbiz-pack/Cargo.toml|reviewed|direct-read|rabbiz-pack dependencies
tools/rabbiz-sdk/crates/rabbiz-pack/src/lib.rs|reviewed|direct-read|rbz manifest, packer, verifier, unpacker, local installer
tools/rabbiz-sdk/crates/rabbiz-pack/src/sha256.rs|reviewed|direct-read|pure-Rust SHA-256 implementation
tools/rabbiz-sdk/crates/rabbiz-cli/src/lib.rs|reviewed|targeted-read|rabbiz-pack CLI and MCP callers
scripts/de_deploy.sh|reviewed|direct-read|DE root deployment script
scripts/de_apps_deploy.sh|reviewed|direct-read|apps root deployment script
scripts/deploy_shell.sh|reviewed|direct-read|shell and rootfs-overlay deployment script
scripts/install_chrome_arm.sh|reviewed|direct-read|Chrome ARM64 deb install script
scripts/install_fex_rootfs.sh|reviewed|direct-read|FEX guestos squashfs install script
scripts/mirror_setup.sh|reviewed|direct-read|wayvnc package download and install script
scripts/build_emu_image.sh|reviewed|direct-read|VM image build script
scripts/bootstrap_build_env.sh|reviewed|direct-read|kernel and uniLoader bootstrap script
scripts/setup_qt_scanout.sh|reviewed|direct-read|Qt scanout install script
scripts/setup_dumb_destroy_fix.sh|reviewed|direct-read|dumb-destroy shim install script
scripts/device_session.sh|reviewed|direct-read|device flock serialization wrapper
devices/samsung/a21s/docs/a21s-flash-boot-checklist.md|reviewed|direct-read|A21s boot partition flash procedure
devices/samsung/a21s/docs/a21s-boot-recovery.md|reviewed|direct-read|A21s boot image anatomy and AVB footer details
devices/samsung/a21s/docs/a21s-pit.txt|reviewed|targeted-read|A21s 42-partition PIT layout verification
```
