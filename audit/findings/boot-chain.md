# Domain Audit: Boot Chain (`BOOT`)

## Scope & Files Reviewed

### Inspected Files

- `rootfs-overlay/etc/systemd/system/getty@tty1.service` (`stat`/`ls-symlink`) — verified symlink to `/dev/null`
- `rootfs-overlay/etc/systemd/system/rabbiz-splash.service` (`direct-read`)
- `rootfs-overlay/usr/local/bin/rabbiz-splash` (`direct-read`)
- `rootfs-overlay/etc/systemd/system/rabbiz-session.service` (`direct-read`)
- `rootfs-overlay/usr/local/bin/rabbiz-session` (`direct-read`)
- `rootfs-overlay/usr/local/bin/rabbiz-labwc-session` (`direct-read`)
- `rootfs-overlay/etc/systemd/system/rabbiz-auth-helper.service` (`direct-read`)
- `rootfs-overlay/etc/systemd/system/a21s-drm-keeper.service` (`direct-read`)
- `rootfs-overlay/etc/systemd/system/a21s-display-pin.service` (`direct-read`)
- `rootfs-overlay/etc/sysctl.d/10-a21s-quiet.conf` (`direct-read`)
- `rootfs-overlay/etc/systemd/system.conf.d/10-a21s-quiet.conf` (`direct-read`)
- `rootfs-overlay/etc/systemd/logind.conf.d/10-a21s-powerkey.conf` (`direct-read`)
- `scripts/bootstrap_build_env.sh` (`direct-read`)
- `scripts/build_emu_image.sh` (`direct-read`)
- `scripts/deploy_shell.sh` (`targeted-read`, lines 170–245)
- `devices/samsung/a21s/kernel/a21s_linux_defconfig` (`targeted-read` + `rg-scan`, lines 1–80, 1120–1176, and security/boot config grep)
- `devices/samsung/a21s/kernel/patches/0001-a21s-wlbt-wifi-bt-dts.patch` (`direct-read`)
- `devices/samsung/a21s/docs/a21s-mainline-boot.md` (`direct-read`)
- `devices/samsung/a21s/docs/a21s-boot-recovery.md` (`direct-read`)
- `devices/samsung/a21s/docs/a21s-flash-boot-checklist.md` (`direct-read`)
- `devices/samsung/a21s/device.toml` (`direct-read`)
- `devices/xiaomi/muyu/scripts/backup_muyu_stock.sh` (`direct-read`)
- `docs/BUILD.md` (`direct-read`)
- `docs/PLAN.md` (`rg-scan`, boot chain design verification)
- `docs/ARCHITECTURE.md` (`rg-scan`, boot chain design verification)
- `audit/tools/kernel-hardening.txt` (`direct-read`, triaged and verified in source)
- `audit/tools/shellcheck.txt` (`direct-read`, triaged and verified in source)
- Untracked local build artifacts verified read-only via `build/avbtool.py info_image`: `build/images/boot-avb.img`, `build/images/vbmeta-disabled.img` (gitignored, not in tracked repo).

### Files / Components NOT Inspected (and Why)

- `src/linux/` and `src/uniLoader/`: Gitignored external upstream repositories (not tracked in this repo).
- PostmarketOS initramfs binary (`ramdisk`, ~1.78 MB) and stock Samsung `android_dt_table`: Not tracked in this repository; live inside Docker build volume or device flash.
- Live on-device `/etc/pam.d/login`, `/etc/shadow`, `/etc/fstab`, `/etc/ssh/sshd_config`, and `/dev/mmcblk0p*` partitions: Read-only repository audit without live device access.
- `devices/samsung/a21s/kernel/patches/0002..0013`: Only scanned via `rg` for `bootargs`/`cmdline`/`console`/`security` strings; full driver patch audit is out of scope for `BOOT` (covered by Kernel domain).

---

## Findings

### BOOT-001
```
ID: BOOT-001
Title: Verified Boot completely disabled (unlocked S-Boot, unsigned AVB boot image, no dm-verity or kernel module signing)
Severity: High
Confidence: Confirmed
Location: devices/samsung/a21s/docs/a21s-flash-boot-checklist.md:79-80 (also devices/samsung/a21s/docs/a21s-boot-recovery.md:16,37-38; devices/samsung/a21s/docs/a21s-mainline-boot.md:4; devices/samsung/a21s/kernel/a21s_linux_defconfig:71-75,255-258)
Evidence:
  1) devices/samsung/a21s/docs/a21s-mainline-boot.md:4:
     - **Bootloader chain:** Samsung S-Boot (`OEM LOCK: OFF`, `VBMETA` `flags=2` `rollback_index=12`, stub `DTBO` `custom0=0x00, custom1=0xff`, `rollback_index=12`) -> `samsungexynos850/uniLoader` (`a21s_defconfig`) -> `samsungexynos850/linux` (`staging` branch, `exynos850-a217f.dtb`)

  2) devices/samsung/a21s/docs/a21s-flash-boot-checklist.md:79-80:
     python3 /work/avb/avbtool.py add_hash_footer --image /tmp/boot-avb-new.img \
       --partition_name boot --partition_size 37748736 --rollback_index 12
     (No --key or --algorithm flag is passed; produces an unsigned SHA-256 hash footer.)

  3) Confirmed against local built artifacts via `python3 build/avbtool.py info_image`:
     build/images/boot-avb.img:        Authentication Block: 0 bytes | Algorithm: NONE | Rollback Index: 12 | Flags: 0
     build/images/vbmeta-disabled.img: Authentication Block: 0 bytes | Algorithm: NONE | Rollback Index: 12 | Flags: 2 | Descriptors: (none)

  4) devices/samsung/a21s/kernel/a21s_linux_defconfig:
     - CONFIG_MODULES=y (line 71), CONFIG_BLK_DEV_DM=y (line 255), CONFIG_DM_CRYPT=y (line 256)
     - CONFIG_DM_VERITY, CONFIG_MODULE_SIG, CONFIG_MODULE_SIG_FORCE, and CONFIG_SECURITY_LOCKDOWN_LSM are absent/disabled.
     - No signing keys or key management scripts exist in the repository.

Impact: There is zero cryptographic integrity verification across the entire boot and OS chain after Samsung S-Boot. Any attacker with brief physical access (Samsung Download Mode / Heimdall) or temporary local root (`dd` to `/dev/mmcblk0p18`, modifying `/usr/lib/modules/`, or modifying the rootfs image) can persistently backdoor `uniLoader`, the kernel `Image`, the embedded DTB, the initramfs, kernel modules, or rootfs binaries without detection (persistent bootkit / evil-maid attack).

Exploit/Trigger: Physical access to the device (VolUp+VolDown + USB Download Mode + `heimdall flash --BOOT`), or local root access writing to `/dev/mmcblk0p18` or `/usr/lib/modules/`.

Recommendation:
  - Fact vs. hardware constraint: On Samsung Exynos 850 (`SM-A217F`), stock BL1/BL2/S-Boot are e-fuse locked to Samsung keys (`docs/PLAN.md:13,44`), so `OEM LOCK: OFF` and disabled stock AVB are required to boot mainline Linux on this hardware. Document this explicitly as an accepted hardware threat-model limitation for `a21s`.
  - Add downstream integrity controls that do not depend on Samsung S-Boot:
    1) Enable `CONFIG_MODULE_SIG=y` and `CONFIG_MODULE_SIG_FORCE=y` in `a21s_linux_defconfig` with a managed signing key (or ephemeral build key) so unsigned kernel modules cannot be loaded.
    2) Enable `CONFIG_DM_VERITY=y` and evaluate a read-only verified OS image (`/usr` or rootfs) paired with `dm-crypt` (`CONFIG_DM_CRYPT=y`) for `/home` and mutable state.
    3) For Phase 2 devices (e.g., Xiaomi Pad 7 Pro `muyu` or future hardware supporting custom AVB keys / UEFI Secure Boot), establish an offline key management and image-signing pipeline before release.

References: CWE-345 (Insufficient Verification of Data Authenticity), CWE-494 (Download/Load of Code Without Integrity Check), Android Verified Boot 2.0
```

### BOOT-002
```
ID: BOOT-002
Title: Passwordless auto-login graphical session on boot with keyguard fail-closed bypass when MODE != de
Severity: High
Confidence: Confirmed
Location: rootfs-overlay/etc/systemd/system/rabbiz-session.service:31-35,47,60 and rootfs-overlay/usr/local/bin/rabbiz-session:58-82,89-131
Evidence:
  1) rootfs-overlay/etc/systemd/system/rabbiz-session.service:30-35,47,60:
     Type=simple
     User=rabbiz
     SupplementaryGroups=seat input video render audio
     PAMName=login
     ...
     ExecStart=/usr/local/bin/rabbiz-session
     ...
     [Install]
     WantedBy=multi-user.target

  2) rootfs-overlay/usr/local/bin/rabbiz-session:58-82:
     if [ -z "$MODE" ]; then
         mode_file="$XDG_CONFIG_HOME/rabbiz-de/mode"
         if [ -r "$mode_file" ]; then
             read -r m < "$mode_file" || true
             case "$m" in
                 de|labwc) MODE="$m" ;;
                 *) echo "rabbiz-session: invalid mode '$m' in $mode_file — labwc" >&2 ;;
             esac
         fi
         if [ -z "$MODE" ] && [ -e /etc/rabbiz-de/enabled ]; then
             MODE=de
         fi
     fi
     [ -z "$MODE" ] && MODE=labwc
     ...
     if [ "$MODE" != "de" ]; then
         exec "$LABWC_LAUNCHER"
     fi

  3) rootfs-overlay/usr/local/bin/rabbiz-session:121-131:
     to_labwc() {
         if keyguard_requires_lock; then
             echo "rabbiz-session: $1 — refusing labwc fallback because keyguard lock is configured (fail-closed)" >&2
             exit 1
         fi
         ...
         exec "$LABWC_LAUNCHER"
     }

  4) rootfs-overlay/usr/local/bin/rabbiz-session:89-118:
     `keyguard_requires_lock()` reads user-writable `$XDG_CONFIG_HOME/rabbiz-shell/prefs.json` and returns 1 (no lock) if `"lock_method": "swipe"`, without consulting the root-owned `rabbiz-auth-helper.service` store (`/var/lib/rabbiz-auth`).

Impact: At boot, `rabbiz-session.service` automatically creates a PAM `login` session and launches a graphical Wayland desktop as user `rabbiz` without any display-manager authentication. While `to_labwc()` attempts to fail closed when a keyguard lock is configured, that check is completely bypassed at line 80 (`if [ "$MODE" != "de" ]`) whenever `/etc/rabbiz-de/enabled` is absent (default per `scripts/de_deploy.sh:117`) or whenever the user-writable file `~/.config/rabbiz-de/mode` contains `labwc` (which takes precedence over `/etc/rabbiz-de/enabled`). Because `labwc` does not enforce the `rabbiz-de` keyguard lock (`docs/BUILD.md:212`), booting directly into `labwc` exposes an unlocked desktop session. Additionally, trusting user-writable `prefs.json` for `keyguard_requires_lock()` allows any process running as `rabbiz` to weaken the fail-closed check before a reboot/crash.

Exploit/Trigger:
  - Physical access after reboot when `/etc/rabbiz-de/enabled` is not set (default), or when `~/.config/rabbiz-de/mode` is set to `labwc`.
  - Any code running as `rabbiz` writing `labwc` to `~/.config/rabbiz-de/mode` or `"lock_method": "swipe"` to `~/.config/rabbiz-shell/prefs.json` to ensure next boot skips keyguard lock.

Recommendation:
  1. Call `keyguard_requires_lock` before `exec "$LABWC_LAUNCHER"` at `rabbiz-session:80` (refusing `labwc` startup if a lock is configured).
  2. Query the root-owned `rabbiz-auth-helper` daemon (rather than user-writable `prefs.json` / `keyguard.json`) to determine whether a PIN/password credential is enrolled.
  3. Do not allow user-writable `$XDG_CONFIG_HOME/rabbiz-de/mode` to downgrade from `de` to `labwc` when a keyguard lock is active.
  4. Long-term, move authentication before session startup using a Wayland greeter (e.g., `greetd`) or logind-integrated lock screen.

References: CWE-287 (Improper Authentication), CWE-636 (Not Failing Securely / Fail Open)
```

### BOOT-003
```
ID: BOOT-003
Title: Kernel boot configuration enables kexec, debugfs, Magic SysRq, and legacy permissive/rw bootargs
Severity: Medium
Confidence: Confirmed
Location: devices/samsung/a21s/kernel/a21s_linux_defconfig:34,36-37,70,1163-1165 and devices/samsung/a21s/kernel/patches/0001-a21s-wlbt-wifi-bt-dts.patch:10
Evidence:
  1) devices/samsung/a21s/kernel/a21s_linux_defconfig:
     34: CONFIG_KALLSYMS_ALL=y
     36: CONFIG_KEXEC=y
     37: CONFIG_KEXEC_FILE=y
     70: CONFIG_KPROBES=y
     1163: CONFIG_DEBUG_KERNEL=y
     1164: CONFIG_MAGIC_SYSRQ=y
     1165: CONFIG_DEBUG_FS=y

  2) devices/samsung/a21s/docs/a21s-boot-recovery.md:22:
     - kexec is NOT usable on this device (hangs even with the running DTB).

  3) devices/samsung/a21s/kernel/patches/0001-a21s-wlbt-wifi-bt-dts.patch:10:
     +		bootargs = "androidboot.hardware=exynos850 androidboot.boot_devices=soc@0/12100000.mmc androidboot.selinux=permissive androidboot.verifiedbootstate=orange androidboot.init_fatal_reboot_target=recovery firmware_class.path=/vendor/firmware/ log_buf_len=4M loop.max_part=7 clk_ignore_unused rw";

Impact: Production/device kernels carry unnecessary debug and kernel-replacement surfaces:
  - `CONFIG_KEXEC=y` / `CONFIG_KEXEC_FILE=y` allow a root process to load and execute an arbitrary unsigned kernel in memory even though `kexec` is documented as non-functional on `a21s`.
  - `CONFIG_DEBUG_FS=y`, `CONFIG_KALLSYMS_ALL=y`, and `CONFIG_KPROBES=y` expand kernel introspection and exploitation surfaces.
  - `CONFIG_MAGIC_SYSRQ=y` combined with compiled-in serial consoles (`CONFIG_SERIAL_SAMSUNG_CONSOLE=y`, line 452) or attached USB keyboards allows triggering kernel crashes, reboots, or state dumps.
  - DTS `chosen/bootargs` mounts the root filesystem `rw` directly and retains legacy Android `androidboot.selinux=permissive` and `androidboot.verifiedbootstate=orange` flags.

Exploit/Trigger: Local root process invoking `kexec_load`/`kexec_file_load`, local/physical access to debugfs or Magic SysRq via USB keyboard or UART console.

Recommendation:
  1. Split `a21s_linux_defconfig` into a hardened base config and an opt-in debug fragment; disable `CONFIG_KEXEC`, `CONFIG_KEXEC_FILE`, `CONFIG_DEBUG_FS`, `CONFIG_MAGIC_SYSRQ`, and `CONFIG_KALLSYMS_ALL` in release builds.
  2. Remove unused Android-era parameters (`androidboot.selinux=permissive`, `androidboot.verifiedbootstate=orange`) from `0001-a21s-wlbt-wifi-bt-dts.patch` (retaining `clk_ignore_unused`, which is required per `a21s-boot-recovery.md:76`).
  3. Add kernel hardening cmdline defaults (e.g., `init_on_alloc=1`, `slab_nomerge`, `ro` root when immutable rootfs is adopted).

References: CWE-489 (Active Debug Code), Kernel Self Protection Project (KSPP)
```

### BOOT-004
```
ID: BOOT-004
Title: bootstrap_build_env.sh ignores uniLoader fetch/checkout failures and pins short commit hashes
Severity: Medium
Confidence: Confirmed
Location: scripts/bootstrap_build_env.sh:5-8,42-49
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
  47: else

Impact: Because lines 45–46 append `|| true`, any failure to fetch or check out `UNILOADER_COMMIT` (for example, if upstream advances >20 commits or rejects unadvertised SHA-1 fetches) is silently ignored under `set -euo pipefail`. The build environment will silently compile and pack whatever untrusted `HEAD` commit is currently on the remote `uniLoader` default branch into `boot-avb.img`. Additionally, short 7–9 character SHA-1 prefixes (`06a1bba`, `14a425b3a`) are weaker than full 40-character commit hashes for supply-chain pinning.

Exploit/Trigger: Running `./scripts/bootstrap_build_env.sh` on a fresh machine when `06a1bba` is not fetched by `fetch --depth 20` or if the upstream repository is modified/compromised.

Recommendation:
  1. Replace `LINUX_COMMIT` and `UNILOADER_COMMIT` with full 40-character SHA-1 commit hashes.
  2. Remove `|| true` from lines 45–46, use `git fetch origin "$UNILOADER_COMMIT"` (falling back to full fetch if needed), and assert `[[ "$(git -C "$ROOT_DIR/src/uniLoader" rev-parse HEAD)" == "$UNILOADER_COMMIT" ]]` before continuing.

References: CWE-494 (Download of Code Without Integrity Check), CWE-755 (Improper Handling of Exceptional Conditions)
```

### BOOT-005
```
ID: BOOT-005
Title: Hardcoded UID 1001 in rabbiz-session.service breaks boot session on non-1001 user IDs
Severity: Low
Confidence: Confirmed
Location: rootfs-overlay/etc/systemd/system/rabbiz-session.service:26-27,38-39 (related: rootfs-overlay/usr/local/bin/rabbiz-splash:188-194)
Evidence:
  rootfs-overlay/etc/systemd/system/rabbiz-session.service:
  26: Wants=user@1001.service
  27: After=user@1001.service systemd-user-sessions.service
  ...
  31: User=rabbiz
  ...
  38: Environment=XDG_RUNTIME_DIR=/run/user/1001
  39: Environment=DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1001/bus

Impact: On a fresh rootfs where `rabbiz` is created with UID `1000` (anticipated in `rabbiz-splash:190-194`, which checks both `/run/user/1001` and `/run/user/1000`), `rabbiz-session.service` hardcodes `XDG_RUNTIME_DIR=/run/user/1001` and `DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1001/bus`, causing a mismatch with `pam_systemd`'s `/run/user/1000`. This breaks Wayland socket discovery, user D-Bus activation, and `systemd --user` services (PipeWire, portals) on boot.

Exploit/Trigger: Deploying `rootfs-overlay/` onto a freshly provisioned Arch Linux ARM rootfs where `rabbiz` is the first regular user (UID 1000).

Recommendation: Rely on `pam_systemd` (`PAMName=login`) and `rabbiz-session:32-33` (`XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"`) instead of hardcoding `1001` in `rabbiz-session.service`, or template the UID during image/deploy provisioning.

References: CWE-798 / Configuration Reliability Defect
```

### BOOT-006
```
ID: BOOT-006
Title: rabbiz-splash.service runs early as root without systemd sandboxing and unvalidated BMP width bounds
Severity: Low
Confidence: Confirmed
Location: rootfs-overlay/etc/systemd/system/rabbiz-splash.service:7-14 and rootfs-overlay/usr/local/bin/rabbiz-splash:218-235
Evidence:
  1) rootfs-overlay/etc/systemd/system/rabbiz-splash.service:7-14:
     [Service]
     Type=notify
     NotifyAccess=main
     TimeoutStartSec=10
     ExecStartPre=-/usr/bin/dmesg -n 1
     ExecStartPre=-/bin/sh -c 'echo 0 > /sys/class/graphics/fbcon/cursor_blink; printf "\\033[?25l\\033[2J" > /dev/tty1'
     ExecStart=/usr/local/bin/rabbiz-splash

  2) rootfs-overlay/usr/local/bin/rabbiz-splash:218-235:
     off = struct.unpack_from("<I", bmp_bytes, 10)[0]
     w, h, _planes, ibpp = struct.unpack_from("<iiHH", bmp_bytes, 18)
     ...
     x0 = (fw - w) // 2
     ...
     for y in range(h):
         if y0 + y >= fh:
             break
         row = bmp_bytes[off + y * w * 4: off + (y + 1) * w * 4]
         p = (y0 + y) * stride + x0 * 4
         buf[p:p + w * 4] = row

Impact: A Python interpreter running a 25-second polling loop during early boot holds full `root` privileges with no systemd sandboxing (`NoNewPrivileges`, `PrivateNetwork`, capability restriction). Additionally, `paint_splash_base()` does not validate `0 < w <= fw` or buffer length against `off + w * h * 4`; if a BMP wider than the panel framebuffer is installed, `x0` becomes negative and corrupts scanlines or raises an exception (caught by the top-level handler at line 303 so it will not block boot).

Exploit/Trigger: Local tampering with `/usr/share/rabbiz/rabbizos-splash.bmp` or deploying the splash asset to a device with framebuffer width smaller than the BMP width.

Recommendation:
  1. Add early-boot-compatible systemd hardening to `rabbiz-splash.service`: `NoNewPrivileges=yes`, `PrivateNetwork=yes`, `ProtectKernelModules=yes`, `RestrictAddressFamilies=AF_UNIX`, `RestrictNamespaces=yes`, `CapabilityBoundingSet=CAP_SYS_TTY_CONFIG CAP_SYS_ADMIN` (or drop to a dedicated `video`-group user for `ExecStart` while prefixing `ExecStartPre=+` for root-only sysctl/dmesg setup).
  2. Validate `w > 0`, `h > 0`, `w <= fw`, and `off + w * h * 4 <= len(bmp_bytes)` in `paint_splash_base()`.

References: CWE-250 (Execution with Unnecessary Privileges), CWE-129 (Improper Validation of Array Index)
```

### BOOT-007
```
ID: BOOT-007
Title: Tier 3 VM builder binds emulator/control listeners to 0.0.0.0 inside guest and boots root rw with serial console
Severity: Low
Confidence: Confirmed
Location: scripts/build_emu_image.sh:193-194,267,273,275
Evidence:
  193: Environment=RABBIZ_EMU_LISTEN=0.0.0.0:5910
  194: Environment=RABBIZ_CTL_LISTEN=0.0.0.0:5911
  ...
  267: -append "console=ttyAMA0,115200 root=/dev/vda rw rootwait systemd.unified_cgroup_hierarchy=1 rabbiz.vm=1 rabbiz.device=${DEVICE}" \
  ...
  273: -netdev user,id=net0,hostfwd=tcp:127.0.0.1:5910-:5910,hostfwd=tcp:127.0.0.1:5911-:5911,hostfwd=tcp:127.0.0.1:2222-:22 \
  275: -serial mon:stdio

Impact: Inside the Tier 3 development VM, display (`5910`) and control IPC (`5911`) sockets bind to all guest interfaces (`0.0.0.0`). On the default QEMU user-mode network (`-netdev user`), host-side exposure is safely limited to `127.0.0.1`, but attaching the VM to a bridged/tap network would expose unauthenticated emulator control ports on the LAN.

Exploit/Trigger: Running the generated VM image with bridged/tap networking instead of the generated `launch-qemu.sh` SLIRP config.

Recommendation: Document in `10-rabbiz-emu-vm.conf` and `build_emu_image.sh` that the drop-in is strictly for isolated development VMs and must never be deployed to physical devices or bridged networks without authentication.

References: CWE-668 (Exposure of Resource to Wrong Sphere)
```

### BOOT-008
```
ID: BOOT-008
Title: getty@tty1.service is masked (no auto-login root shell on tty1)
Severity: Info
Confidence: Confirmed
Location: rootfs-overlay/etc/systemd/system/getty@tty1.service (symlink -> /dev/null) and scripts/deploy_shell.sh:215
Evidence:
  $ ls -la rootfs-overlay/etc/systemd/system/getty@tty1.service
  lrwxrwxrwx 1 dit dit 9 ... getty@tty1.service -> /dev/null

  scripts/deploy_shell.sh:215:
  # Boot splash + hardware volume/screenshot keys; no login prompt on the panel (getty@tty1 is a /dev/null symlink in the overlay).

Impact: Informational observation confirming that `getty@tty1.service` does NOT configure root auto-login (nor any VT1 console login). Note that `getty@.service` for `tty2..tty6` and `serial-getty@.service` are not masked in `rootfs-overlay/`, so standard password-authenticated gettys remain available if a keyboard or UART is attached.

Exploit/Trigger: N/A (observation).

Recommendation: Keep `getty@tty1.service` masked; if physical UART/USB-keyboard console access should be disabled in release images, explicitly mask `serial-getty@.service` and disable VT switching (`NAutoVTs=0` in `logind.conf.d`).

References: systemd.unit(5)
```

---

## Positive Observations

- **No root auto-login on `tty1`:** `rootfs-overlay/etc/systemd/system/getty@tty1.service` is symlinked to `/dev/null` (`scripts/deploy_shell.sh:215`), preventing an unauthenticated or accidental console prompt on the panel.
- **Resilient, bounded boot splash:** `rootfs-overlay/usr/local/bin/rabbiz-splash` bounds its runtime (`MAX_WAIT_S = 25.0`, line 28), handles `SIGTERM`/`SIGINT` cleanly (lines 245–246), and catches exceptions at top level while still calling `sd_notify_ready()` (lines 300–306) so a splash error cannot hang boot.
- **Fail-closed fallback intent:** `rootfs-overlay/usr/local/bin/rabbiz-session:121-125` explicitly attempts to prevent crashing out of `rabbiz-de` into an unlocked `labwc` session when a keyguard lock is configured (needs extension to cover the direct `MODE != de` path and root-owned auth state per `BOOT-002`).
- **KASLR and dm-crypt enabled in kernel config:** `devices/samsung/a21s/kernel/a21s_linux_defconfig:43` enables `CONFIG_RANDOMIZE_BASE=y` and line 256 enables `CONFIG_DM_CRYPT=y`.
- **Safe QEMU host port binding and `/lib` guardrail:** `scripts/build_emu_image.sh:113-116` guards against Arch ARM `/lib -> usr/lib` breakage, and line 273 binds QEMU `hostfwd` ports strictly to `127.0.0.1`.

---

## Reviewed Files Machine List

```files
rootfs-overlay/etc/systemd/system/getty@tty1.service|reviewed|stat-symlink|verified /dev/null mask
rootfs-overlay/etc/systemd/system/rabbiz-splash.service|reviewed|direct-read|boot splash systemd unit
rootfs-overlay/usr/local/bin/rabbiz-splash|reviewed|direct-read|boot splash framebuffer script
rootfs-overlay/etc/systemd/system/rabbiz-session.service|reviewed|direct-read|boot graphical session systemd unit
rootfs-overlay/usr/local/bin/rabbiz-session|reviewed|direct-read|session selector and keyguard fallback logic
rootfs-overlay/usr/local/bin/rabbiz-labwc-session|reviewed|direct-read|fallback compositor launcher
rootfs-overlay/etc/systemd/system/rabbiz-auth-helper.service|reviewed|direct-read|keyguard auth helper unit ordering
rootfs-overlay/etc/systemd/system/a21s-drm-keeper.service|reviewed|direct-read|pre-session DRM keeper unit
rootfs-overlay/etc/systemd/system/a21s-display-pin.service|reviewed|direct-read|display runtime-PM and fbcon unbind unit
rootfs-overlay/etc/sysctl.d/10-a21s-quiet.conf|reviewed|direct-read|boot console printk suppression
rootfs-overlay/etc/systemd/system.conf.d/10-a21s-quiet.conf|reviewed|direct-read|systemd boot status suppression
rootfs-overlay/etc/systemd/logind.conf.d/10-a21s-powerkey.conf|reviewed|direct-read|power key boot/shutdown handling
scripts/bootstrap_build_env.sh|reviewed|direct-read|uniLoader and kernel source bootstrap
scripts/build_emu_image.sh|reviewed|direct-read|Tier 3 VM boot image and QEMU launcher builder
scripts/deploy_shell.sh|reviewed|targeted-read|systemd unit deployment and enablement
devices/samsung/a21s/kernel/a21s_linux_defconfig|reviewed|targeted-read|boot, AVB/verity, kexec, console, and integrity flags
devices/samsung/a21s/kernel/patches/0001-a21s-wlbt-wifi-bt-dts.patch|reviewed|direct-read|DTS chosen/bootargs inspection
devices/samsung/a21s/docs/a21s-mainline-boot.md|reviewed|direct-read|boot chain facts
devices/samsung/a21s/docs/a21s-boot-recovery.md|reviewed|direct-read|boot.img anatomy and AVB footer recipe
devices/samsung/a21s/docs/a21s-flash-boot-checklist.md|reviewed|direct-read|boot-avb.img repack and flash procedure
devices/samsung/a21s/device.toml|reviewed|direct-read|device boot method metadata
devices/xiaomi/muyu/scripts/backup_muyu_stock.sh|reviewed|direct-read|Phase 2 device stock partition backup
docs/BUILD.md|reviewed|direct-read|build, kexec, and uniLoader boot documentation
docs/PLAN.md|reviewed|rg-scan|boot chain design and e-fuse constraints
docs/ARCHITECTURE.md|reviewed|rg-scan|boot chain architecture verification
```
