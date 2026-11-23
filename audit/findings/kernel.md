# Domain Audit: Kernel (`KERN`)

## Scope & Files Reviewed

### Directly Read (`direct-read`)
- `audit/tools/kernel-hardening.txt`
- `audit/tools/binary-blobs.txt`
- `devices/samsung/a21s/kernel/a21s_linux_defconfig` (all 1176 lines)
- `devices/samsung/a21s/kernel/patches/0001-a21s-wlbt-wifi-bt-dts.patch`
- `devices/samsung/a21s/kernel/patches/0002-asoc-topology-visible.patch`
- `devices/samsung/a21s/kernel/patches/0003-a21s-abox-audio-dts.patch`
- `devices/samsung/a21s/kernel/patches/0004-exynos-drm-dma-iommu-optional.patch`
- `devices/samsung/a21s/kernel/patches/0005-a21s-power-modem-dts.patch`
- `devices/samsung/a21s/kernel/patches/0006-s2mu106-fuelgauge-compatible.patch`
- `devices/samsung/a21s/kernel/patches/0007-a21s-sma1303-amp-dts.patch`
- `devices/samsung/a21s/kernel/patches/0008-a21s-sensors-regulators-dts.patch`
- `devices/samsung/a21s/kernel/patches/0009-a21s-tmu-thermal-zones.patch`
- `devices/samsung/a21s/kernel/patches/0010-dsi-underrun-link-recovery.patch`
- `devices/samsung/a21s/kernel/patches/0011-a21s-cpu-idle-states.patch`
- `devices/samsung/a21s/kernel/patches/0012-a21s-et520-spi-dts.patch`
- `devices/samsung/a21s/kernel/patches/0013-a21s-decon850-keep-win0-mapcolor-on-disable.patch`
- `patches/mesa/0001-wayland-opt-in-scanout-buffers.patch`
- `patches/squeekboard/0001-clipboard-paste.patch`
- `patches/squeekboard/README.md`
- `rootfs-overlay/etc/sysctl.d/10-a21s-quiet.conf`
- `rootfs-overlay/etc/modules-load.d/a21s-hardware.conf`
- `socs/exynos850/drivers/fingerprint/a21s_et5xx.c`
- `socs/exynos850/drivers/tmu/exynos850-tmu.c`
- `socs/exynos850/drivers/vibrator/a21s_vibrator.c`
- `socs/exynos850/drivers/scsc/wlan/scsc_wlan_mmap.c`

### Targeted Read (`targeted-read`)
- `build/linux/.config` (gitignored build artifact; queried via `rg` to verify resolved Linux 7.3.0 arm64 Kconfig defaults vs `a21s_linux_defconfig` savedefconfig)
- `scripts/bootstrap_build_env.sh` (lines 25–50, to verify patch application loop)
- `socs/exynos850/drivers/cpif/cpif_core.c` (lines 140–350)
- `socs/exynos850/drivers/cpif/cpif_boot.c` (lines 1–420)
- `socs/exynos850/drivers/cpif/cpif_power.c` (lines 180–350)
- `socs/exynos850/drivers/scsc/misc/mxproc.c` (lines 1–95, 160–260, 340–450)
- `socs/exynos850/drivers/scsc/wlan/udi.c` (lines 410–570, 690–770)
- `socs/exynos850/drivers/scsc/bt/scsc_bt_module.c` (lines 67–70, 2120–2220)
- `socs/exynos850/drivers/abox/aud3004x-dump.c` (lines 350–480)
- `socs/exynos850/drivers/sensorhub/a21s_chub.c` (lines 100–250)
- `rootfs-overlay/etc/udev/rules.d/99-a21s-modem-fp.rules` (via `rg` hit verification)
- `rootfs-overlay/etc/udev/rules.d/99-a21s-sensors-vib.rules` (via `rg` hit verification)
- `socs/exynos850/drivers/scsc/scsc_bt.ko.zst`, `scsc_mx.ko.zst`, `scsc_platform_mif.ko.zst`, `scsc_wlan.ko.zst` (binary metadata, `zstd -dc | strings`, tail signature check, `git log`)

### Pattern-Scanned (`rg-scan`)
- All remaining `.c`/`.h` files under `socs/exynos850/drivers/scsc/` (261 files total), `abox/` (64 files), `acpm/` (10 files), `cpif/` (11 files), `s2mu106/` (17 files, including untracked build artifacts `.o`/`.ko`), and `sensorhub/` (2 files) were scanned via `rg` for `copy_from_user`, `copy_to_user`, `unlocked_ioctl`, `compat_ioctl`, `0666`, `0777`, `0664`, `0660`, `S_IWOTH`, `S_IWUGO`, `S_IRWXUGO`, `DEVICE_ATTR`, `CLASS_ATTR`, `module_param`, `debugfs_create`, `proc_create`, `misc_register`, `cdev_add`, `strcpy`, `strcat`, `sprintf`, `vsprintf`, and `sscanf`.

### NOT Inspected (Explicitly Excluded)
- `socs/exynos850/drivers/vendor-include/` (23 files) — `skipped` (`vendor-include`: Samsung vendor compatibility header shims; not directly executed logic, though macro definitions were not audited line-by-line).
- Upstream Linux kernel tree in `src/linux/` (gitignored upstream repo) — `skipped` (out of scope for RabbizOS repo audit; only RabbizOS patches and defconfig were audited).
- Untracked build artifacts in `socs/exynos850/drivers/s2mu106/*.o`, `*.ko`, `*.cmd` — `skipped` (local build outputs).
- Full line-by-line review of all ~350 vendor-ported C/H files in `socs/exynos850/drivers/scsc/` and `socs/exynos850/drivers/abox/` was not performed; only `rg`-flagged hotspots were directly read. We make no claim of absence of additional bugs in unread portions of those vendor trees.

---

## Findings

```
ID: KERN-001
Title: Kernel Module Signature Verification and Lockdown Disabled; Unsigned Pre-compiled .ko.zst Blobs Tracked in Git
Severity: High
Confidence: Confirmed
Location: devices/samsung/a21s/kernel/a21s_linux_defconfig:71-75 (omitted), build/linux/.config:951,8857, socs/exynos850/drivers/scsc/*.ko.zst
Evidence:
# build/linux/.config:951
# CONFIG_MODULE_SIG is not set
# build/linux/.config:8857
# CONFIG_SECURITY_LOCKDOWN_LSM is not set

# Verified via zstd -dc <blob> | strings (no "~Module signature appended~" trailer on any of the 4 blobs):
socs/exynos850/drivers/scsc/scsc_bt.ko.zst          vermagic=7.3.0-g14a425b3a7dc SMP preempt mod_unload aarch64 (unsigned)
socs/exynos850/drivers/scsc/scsc_mx.ko.zst          vermagic=7.3.0-g14a425b3a7dc SMP preempt mod_unload aarch64 (unsigned)
socs/exynos850/drivers/scsc/scsc_platform_mif.ko.zst vermagic=7.3.0-g14a425b3a7dc SMP preempt mod_unload aarch64 (unsigned)
socs/exynos850/drivers/scsc/scsc_wlan.ko.zst        vermagic=7.3.0-g14a425b3a7dc SMP preempt mod_unload aarch64 (unsigned)
Impact:
1. Any process with CAP_SYS_MODULE (root) can load arbitrary unsigned kernel modules to persist in kernel space or bypass userland controls.
2. Four pre-compiled, unsigned binary kernel modules (`scsc_*.ko.zst`, totaling ~367 KB compressed) are committed directly into the repository without cryptographic signatures or verifiable reproducible-build provenance. They can silently drift from the tracked C sources in `socs/exynos850/drivers/scsc/` or be tampered with in a supply-chain compromise.
Exploit/Trigger:
Local root compromise or malicious commit/PR modifying the opaque `.ko.zst` blobs in git.
Recommendation:
1. Remove pre-compiled `socs/exynos850/drivers/scsc/*.ko.zst` binaries from git and build them reproducibly from source during kernel/rootfs packaging (and add `*.ko` / `*.ko.zst` to `.gitignore`).
2. Enable `CONFIG_MODULE_SIG=y`, `CONFIG_MODULE_SIG_SHA512=y`, and (once all out-of-tree modules are signed in the build pipeline) `CONFIG_MODULE_SIG_FORCE=y` plus `CONFIG_SECURITY_LOCKDOWN_LSM=y` (`CONFIG_LOCK_DOWN_KERNEL_FORCE_INTEGRITY=y`).
References: CWE-345 (Insufficient Verification of Data Authenticity), CWE-494 (Download of Code Without Integrity Check)
```

```
ID: KERN-002
Title: No Mandatory Access Control (MAC) / LSM Active and No Verified Rootfs (DM_VERITY Disabled)
Severity: High
Confidence: Confirmed
Location: devices/samsung/a21s/kernel/a21s_linux_defconfig:1140-1141, build/linux/.config:2648,8851-8859,8867, devices/samsung/a21s/kernel/patches/0001-a21s-wlbt-wifi-bt-dts.patch:10
Evidence:
# build/linux/.config:2648
# CONFIG_DM_VERITY is not set
# build/linux/.config:8851-8859,8867
# CONFIG_SECURITY_SMACK is not set
# CONFIG_SECURITY_TOMOYO is not set
# CONFIG_SECURITY_APPARMOR is not set
# CONFIG_SECURITY_LOADPIN is not set
# CONFIG_SECURITY_YAMA is not set
# CONFIG_SECURITY_SAFESETID is not set
# CONFIG_SECURITY_LOCKDOWN_LSM is not set
# CONFIG_SECURITY_LANDLOCK is not set
# CONFIG_SECURITY_IPE is not set
CONFIG_LSM="landlock,lockdown,yama,loadpin,safesetid,ipe,bpf"
# (CONFIG_SECURITY_SELINUX is also absent/not enabled)

# devices/samsung/a21s/kernel/patches/0001-a21s-wlbt-wifi-bt-dts.patch:10
+ bootargs = "androidboot.hardware=exynos850 androidboot.boot_devices=soc@0/12100000.mmc androidboot.selinux=permissive androidboot.verifiedbootstate=orange ... rw";
Impact:
Despite `CONFIG_SECURITY=y`, zero traditional MAC or hardening LSMs (SELinux, AppArmor, Yama, Landlock, Lockdown, LoadPin) are compiled into the kernel; only `bpf` is active in `CONFIG_LSM`. Without `CONFIG_SECURITY_YAMA`, any compromised app running as the session user (`rabbiz`) can `ptrace` any other same-UID process (compositor, shell, browser, credential agents) to steal secrets or inject code. Without `CONFIG_SECURITY_LANDLOCK`, userland sandboxes cannot apply unprivileged filesystem/network restrictions. Without `CONFIG_DM_VERITY` (and with `rw` rootfs), offline or runtime tampering of system binaries goes undetected.
Exploit/Trigger:
Any compromised userland application running under the default session user (`rabbiz`) or physical access to storage.
Recommendation:
1. Immediately enable `CONFIG_SECURITY_YAMA=y` (with `kernel.yama.ptrace_scope=1` or `2`) and `CONFIG_SECURITY_LANDLOCK=y` — both are low-friction, zero-policy-maintenance hardening wins.
2. Enable `CONFIG_SECURITY_APPARMOR=y` (or SELinux) and ship confining profiles for network-facing daemons and user apps.
3. Plan `CONFIG_DM_VERITY=y` with a read-only system partition for production builds.
References: CWE-284 (Improper Access Control), CWE-693 (Protection Mechanism Failure)
```

```
ID: KERN-003
Title: Fingerprint Driver (a21s-et5xx) Performs No Biometric Verification and Exposes World-Writable Software Unlock Injection
Severity: High
Confidence: Confirmed
Location: socs/exynos850/drivers/fingerprint/a21s_et5xx.c:80-85,117,188-204,307-318, rootfs-overlay/etc/udev/rules.d/99-a21s-modem-fp.rules:1,3, rootfs-overlay/etc/modules-load.d/a21s-hardware.conf:2
Evidence:
# socs/exynos850/drivers/fingerprint/a21s_et5xx.c:188-204
static ssize_t esfp_write(struct file *filp, const char __user *buf,
			  size_t count, loff_t *ppos)
{
	...
	if (!strncmp(cmd, "reset", 5))
		et5xx_pulse_reset(g_fp);
	else
		et5xx_record_event(g_fp);
	return count;
}

# socs/exynos850/drivers/fingerprint/a21s_et5xx.c:307-318
static ssize_t trigger_unlock_store(struct kobject *kobj, ...)
{
	...
	if (val)
		et5xx_record_event(g_fp);
	return count;
}

# rootfs-overlay/etc/udev/rules.d/99-a21s-modem-fp.rules:1,3
KERNEL=="esfp0", MODE="0666"
ACTION=="add", SUBSYSTEM=="misc", KERNEL=="esfp0", RUN+="/bin/chmod 666 /sys/kernel/a21s_fp/trigger_unlock /sys/kernel/a21s_fp/sleep_pin"
Impact:
1. Hardware bypass: `et5xx_irq_thread()` (`a21s_et5xx.c:87-119`) treats ANY debounced rising edge on the DRDY GPIO as a valid unlock event (`et5xx_record_event()` -> `"unlock <n>\n"` on `/dev/esfp0`). No fingerprint image capture or template matching occurs — any person's finger touching the sensor emits an unlock event.
2. Software bypass: Both `/dev/esfp0` (`esfp_write`) and `/sys/kernel/a21s_fp/trigger_unlock` are `chmod 0666` by udev and perform no capability checks. Any unprivileged local process can write a single byte to either path to spoof a biometric unlock event (or toggle `sleep_pin` / reset the sensor).
Exploit/Trigger:
Module `a21s-et5xx` is autoloaded at boot via `rootfs-overlay/etc/modules-load.d/a21s-hardware.conf:2`. Any physical touch or unprivileged `echo 1 > /sys/kernel/a21s_fp/trigger_unlock` / `echo x > /dev/esfp0` triggers an unlock event.
Recommendation:
1. Do NOT use `a21s-et5xx` DRDY edges as an authentication factor (only as a wake-screen trigger if desired, never lockscreen unlock) until real SPI/TEE biometric template verification is implemented.
2. Remove the `trigger_unlock` sysfs write attribute and `esfp_write` event-injection path (or gate behind `CONFIG_DEBUG_FS` + `CAP_SYS_ADMIN`).
3. Remove `MODE="0666"` and `chmod 666` from `rootfs-overlay/etc/udev/rules.d/99-a21s-modem-fp.rules`; restrict device access to root or a dedicated `fprintd` group (`0660`).
References: CWE-287 (Improper Authentication), CWE-732 (Incorrect Permission Assignment for Critical Resource)
```

```
ID: KERN-004
Title: World-Writable /proc/driver/mxman_ctrl0/mx_panic (0666) Enables Unprivileged Wi-Fi/BT Subsystem DoS
Severity: Medium
Confidence: Confirmed
Location: socs/exynos850/drivers/scsc/misc/mxproc.c:179-180,419-422
Evidence:
# socs/exynos850/drivers/scsc/misc/mxproc.c:419-422
	MX_PROCFS_ADD_FILE(mxproc, mx_fail, parent, S_IRUSR | S_IWUSR | S_IRGRP | S_IWGRP | S_IROTH);
	MX_PROCFS_ADD_FILE(mxproc, mx_freeze, parent, S_IRUSR | S_IWUSR | S_IRGRP | S_IWGRP | S_IROTH);
	MX_PROCFS_ADD_FILE(mxproc, mx_panic, parent, S_IRUSR | S_IWUSR | S_IRGRP | S_IWGRP | S_IROTH | S_IWOTH);
	MX_PROCFS_ADD_FILE(mxproc, mx_suspend, parent, S_IRUSR | S_IWUSR | S_IRGRP | S_IWGRP | S_IROTH);

# socs/exynos850/drivers/scsc/misc/mxproc.c:179-180
#else
	if (mxproc)
		mxman_force_panic(mxproc->mxman);
#endif
Impact:
`/proc/driver/mxman_ctrl0/mx_panic` is created with mode `0666` (`S_IWOTH` set). Any unprivileged local user or compromised application can write any byte to this procfs entry to immediately invoke `mxman_force_panic()`, forcibly crashing and resetting the Maxwell Wi-Fi and Bluetooth subsystem (persistent local DoS of wireless connectivity and trigger of firmware memdumps).
Exploit/Trigger:
Any unprivileged process executes `echo 1 > /proc/driver/mxman_ctrl0/mx_panic` whenever `scsc_mx.ko.zst` is loaded.
Recommendation:
Change `mx_panic` (and `mx_fail`, `mx_freeze`, `mx_suspend`) permissions in `socs/exynos850/drivers/scsc/misc/mxproc.c:419-422` to `0600` (`S_IRUSR | S_IWUSR`) and add a `capable(CAP_SYS_ADMIN)` check in `mx_procfs_mx_panic_write()`, then rebuild `scsc_mx.ko.zst` from source.
References: CWE-732 (Incorrect Permission Assignment for Critical Resource), CWE-276 (Incorrect Default Permissions)
```

```
ID: KERN-005
Title: Display Controller (DPU) IOMMU Explicitly Disabled in DTS and Bypassed in Exynos DRM
Severity: Medium
Confidence: Confirmed
Location: devices/samsung/a21s/kernel/patches/0003-a21s-abox-audio-dts.patch:18-20, devices/samsung/a21s/kernel/patches/0004-exynos-drm-dma-iommu-optional.patch:10,19
Evidence:
# devices/samsung/a21s/kernel/patches/0003-a21s-abox-audio-dts.patch:18-20
+&sysmmu_dpu {
+	status = "disabled";
+};

# devices/samsung/a21s/kernel/patches/0004-exynos-drm-dma-iommu-optional.patch:9-11
-	if (!IS_ENABLED(CONFIG_EXYNOS_IOMMU))
+	if (!IS_ENABLED(CONFIG_EXYNOS_IOMMU) || !device_iommu_mapped(dev))
 		return 0;
Impact:
The Exynos850 Display Processing Unit (`sysmmu_dpu`) IOMMU is disabled in the device tree, and `exynos_drm_register_dma()` is patched to silently skip IOMMU attachment when `device_iommu_mapped(dev)` is false. This removes hardware DMA isolation for the display engine, giving DPU unrestricted physical RAM DMA access and increasing the severity of any DRM/DECON/framebuffer programming bug or compromised display-plane configuration.
Exploit/Trigger:
Always active at boot on SM-A217F; exploitable if a bug in Exynos DRM or DECON allows programming an attacker-controlled DMA address or out-of-bounds stride/height.
Recommendation:
Investigate the root cause of `sysmmu_dpu` incompatibility (likely bootloader-inherited `cont_splash_mem` identity mapping / reserved-memory region fault on probe) and re-enable `&sysmmu_dpu` with proper reserved-memory direct mapping (`iommu-addresses` / `dma-ranges` or bootloader handoff cleanup) so DPU DMA is IOMMU-isolated.
References: CWE-1256 (Improper Restriction of Software Interfaces to Hardware Features), CWE-1188 (Insecure Default Initialization of Resource)
```

```
ID: KERN-006
Title: Core Kernel Memory-Corruption Mitigations Disabled (FORTIFY_SOURCE, HARDENED_USERCOPY, Heap Zeroing, SLAB Hardening)
Severity: Medium
Confidence: Confirmed
Location: devices/samsung/a21s/kernel/a21s_linux_defconfig (omitted), build/linux/.config:1062-1063,1069,8884-8885,8887,8893-8894
Evidence:
# build/linux/.config:
1062:# CONFIG_SLAB_FREELIST_RANDOM is not set
1063:# CONFIG_SLAB_FREELIST_HARDENED is not set
1069:# CONFIG_SHUFFLE_PAGE_ALLOCATOR is not set
8884:# CONFIG_INIT_ON_ALLOC_DEFAULT_ON is not set
8885:# CONFIG_INIT_ON_FREE_DEFAULT_ON is not set
8887:# CONFIG_ZERO_CALL_USED_REGS is not set
8893:# CONFIG_FORTIFY_SOURCE is not set
8894:# CONFIG_HARDENED_USERCOPY is not set
Impact:
With ~350 out-of-tree vendor driver files (`scsc`, `abox`, `cpif`, `acpm`) compiled against this kernel, disabling compile-time and runtime memory-safety defenses (`FORTIFY_SOURCE`, `HARDENED_USERCOPY`, `SLAB_FREELIST_HARDENED`, `SLAB_FREELIST_RANDOM`, `INIT_ON_ALLOC_DEFAULT_ON`) substantially lowers the bar to turn latent buffer overflows, UAFs, or uninitialized-memory reads (such as those in KERN-010) into reliable local privilege escalation (LPE) or KASLR-defeating kernel infoleaks.
Exploit/Trigger:
Triggered whenever any kernel memory-corruption or uninitialized-memory bug is exercised.
Recommendation:
Explicitly enable the following low-overhead KSPP-recommended options in `devices/samsung/a21s/kernel/a21s_linux_defconfig`:
- `CONFIG_FORTIFY_SOURCE=y`
- `CONFIG_HARDENED_USERCOPY=y`
- `CONFIG_SLAB_FREELIST_RANDOM=y`
- `CONFIG_SLAB_FREELIST_HARDENED=y`
- `CONFIG_SHUFFLE_PAGE_ALLOCATOR=y`
- `CONFIG_INIT_ON_ALLOC_DEFAULT_ON=y` (and benchmark `CONFIG_INIT_ON_FREE_DEFAULT_ON=y` / `CONFIG_ZERO_CALL_USED_REGS=y`)
References: Kernel Self Protection Project (KSPP) Recommended Settings, CWE-693
```

```
ID: KERN-007
Title: Excessive Kernel Debug Exposure (DEBUG_FS, KEXEC, DEVMEM, MAGIC_SYSRQ, KALLSYMS_ALL, Unhardened sysctl.d)
Severity: Medium
Confidence: Confirmed
Location: devices/samsung/a21s/kernel/a21s_linux_defconfig:17-18,34,36-37,1164-1165, build/linux/.config:184-185,294,317-318,3687,8840,9348-9349,9352,9558, rootfs-overlay/etc/sysctl.d/10-a21s-quiet.conf:1
Evidence:
# devices/samsung/a21s/kernel/a21s_linux_defconfig:
17:CONFIG_IKCONFIG=y
18:CONFIG_IKCONFIG_PROC=y
34:CONFIG_KALLSYMS_ALL=y
36:CONFIG_KEXEC=y
37:CONFIG_KEXEC_FILE=y
1164:CONFIG_MAGIC_SYSRQ=y
1165:CONFIG_DEBUG_FS=y

# build/linux/.config:
3687:CONFIG_DEVMEM=y
8840:# CONFIG_SECURITY_DMESG_RESTRICT is not set
9349:CONFIG_MAGIC_SYSRQ_DEFAULT_ENABLE=0x1
9558:# CONFIG_IO_STRICT_DEVMEM is not set

# rootfs-overlay/etc/sysctl.d/10-a21s-quiet.conf:1
kernel.printk = 1 4 1 3
Impact:
1. Unprivileged local users can read `dmesg` (`CONFIG_SECURITY_DMESG_RESTRICT` not set) and `/proc/config.gz`, leaking kernel addresses, hardware mappings, and exact build configuration to aid KASLR bypass and exploit tailoring.
2. `CONFIG_DEVMEM=y` without `CONFIG_IO_STRICT_DEVMEM` allows root to read/write MMIO device memory directly; `CONFIG_KEXEC=y` allows loading an arbitrary unverified kernel at runtime.
3. `CONFIG_DEBUG_FS=y` exposes numerous vendor debugfs write hooks (e.g., `abox_dump.c:607`, `exynos3830_aud3004x.c:626-630`, `scsc_wifilogger_debugfs.c:406`, `acpm.c:235-237`), several created with mode `0664`.
4. `rootfs-overlay/etc/sysctl.d/` ships no kernel security hardening sysctls (`kptr_restrict`, `dmesg_restrict`, `perf_event_paranoid`, `sysrq`, `suid_dumpable`, `unprivileged_bpf_disabled`).
Exploit/Trigger:
Default runtime configuration on every booted device.
Recommendation:
1. Disable `CONFIG_DEVMEM`, `CONFIG_KEXEC`, `CONFIG_KEXEC_FILE`, `CONFIG_IKCONFIG_PROC`, and `CONFIG_KALLSYMS_ALL` (or restrict via `kptr_restrict=2`) in production defconfig; set `CONFIG_SECURITY_DMESG_RESTRICT=y` and `CONFIG_MAGIC_SYSRQ_DEFAULT_ENABLE=0x0`.
2. Add a hardening sysctl drop-in (e.g., `rootfs-overlay/etc/sysctl.d/20-rabbiz-hardening.conf`) setting:
   `kernel.kptr_restrict = 2`, `kernel.dmesg_restrict = 1`, `kernel.perf_event_paranoid = 3`, `kernel.unprivileged_bpf_disabled = 1`, `kernel.sysrq = 0`, `fs.suid_dumpable = 0`, `fs.protected_hardlinks = 1`, `fs.protected_symlinks = 1`, `fs.protected_fifos = 2`, `fs.protected_regular = 2`.
References: CWE-200 (Exposure of Sensitive Information), CWE-489 (Active Debug Code)
```

```
ID: KERN-008
Title: Out-of-Bounds MMIO Read and Unrestricted CP RAM Linear-Map Poke in Autoloaded a21s-cpif Debug Sysfs (peek)
Severity: Medium
Confidence: Confirmed
Location: socs/exynos850/drivers/cpif/cpif_core.c:220-283, socs/exynos850/drivers/cpif/cpif_power.c:232-236,256-259,268-300, rootfs-overlay/etc/modules-load.d/a21s-hardware.conf:1
Evidence:
# socs/exynos850/drivers/cpif/cpif_power.c:232-237
	if (which == 1) {
		if (!c->vgpio)
			return -ENODEV;
		*val = readl(c->vgpio + off);
		return 0;
	}
# socs/exynos850/drivers/cpif/cpif_power.c:256-259
		if (!m || off >= c->shmem[idx[which - 3]].size)
			return -EINVAL;
		*val = readl(m + off);
# socs/exynos850/drivers/cpif/cpif_power.c:285-300
int cpif_lin_poke(struct cpif_dev *c, u32 off, u32 val)
{
	...
	if (off >= c->cp_size)
		return -EINVAL;
	...
	writel_relaxed(val, (void __iomem *)p);
	return 0;
}
# socs/exynos850/drivers/cpif/cpif_core.c:220-226,275-283
static char peek_buf[64];
static ssize_t peek_show(...) { return sysfs_emit(buf, "%s\n", peek_buf); }
...
static DEVICE_ATTR_RW(peek);
Impact:
1. `cpif_reg_peek()` performs NO bounds check on `off` when `which == 1` (`vgpio`, which is only `0x2000` bytes in DTS). Passing an arbitrary `u32` offset reads arbitrary kernel virtual memory past `c->vgpio` (causing kernel oops/panic or SoC hard hang).
2. For `which == 3..6` and `cpif_lin_peek()`/`cpif_lin_poke()`, checking `off >= size` (instead of `off > size - sizeof(u32)` and `(off & 3) == 0`) permits unaligned accesses and up to 3-byte out-of-bounds reads/writes past the mapped region end.
3. `linw` exposes an unconditional debug backdoor to overwrite arbitrary 32-bit words in CP RAM (`0xd0000000`, 136 MB) via the kernel linear map in an autoloaded production driver (`a21s-cpif`), and `peek_show` is world-readable (`0644`) while accessing global `peek_buf` without locking (data race).
Exploit/Trigger:
Root writes `vgpio 0x10000000` or `linw <off> <val>` to `/sys/devices/platform/soc@0/14020000.cpif/peek`. Any unprivileged user can read the last peeked register/memory value via `cat .../peek`.
Recommendation:
1. Remove the `peek`, `power_step`, and `legacy_init` bring-up debug sysfs attributes from production builds (or move them behind `#ifdef CONFIG_DEBUG_FS` with mode `0600`).
2. If retained for bring-up, store `vgpio_size` from `resource_size()`, validate `off <= size - sizeof(u32)` and `(off & 3) == 0` on all paths, and protect `peek_buf` under `c->lock`.
References: CWE-125 (Out-of-bounds Read), CWE-787 (Out-of-bounds Write), CWE-489 (Active Debug Code)
```

```
ID: KERN-009
Title: bootstrap_build_env.sh Unconditionally Applies Two "DO NOT FLASH" Boot-Hanging Kernel Patches (0011 and 0012)
Severity: Medium
Confidence: Confirmed
Location: scripts/bootstrap_build_env.sh:32-40, devices/samsung/a21s/kernel/patches/0011-a21s-cpu-idle-states.patch:5,16-20, devices/samsung/a21s/kernel/patches/0012-a21s-et520-spi-dts.patch:3-8
Evidence:
# scripts/bootstrap_build_env.sh:32-40
for patch in "$ROOT_DIR"/devices/samsung/a21s/kernel/patches/*.patch; do
  [[ -e "$patch" ]] || continue
  ...
    git -C "$ROOT_DIR/src/linux" apply "$patch"
done

# devices/samsung/a21s/kernel/patches/0011-a21s-cpu-idle-states.patch:5
!!! DO NOT FLASH WITHOUT THE COMPANION KERNEL QUIRK !!!

# devices/samsung/a21s/kernel/patches/0012-a21s-et520-spi-dts.patch:5-8
!!! DO NOT FLASH !!!
Tested 2026-11-19: enabling usi_spi_0 / spi_0 (0x13940000) hangs at
"booting kernel" (likely TZPC Secure World lock on SPI0 for TEE
fingerprint). Keep for reference only; do not include in boot-avb.img.
Impact:
Any fresh build bootstrapped via `./scripts/bootstrap_build_env.sh` automatically applies both `0011` (PSCI `CPU_SUSPEND` without required vendor MPIDR shift quirk) and `0012` (confirmed to hang at "booting kernel" due to TZPC Secure World lock on SPI0). Flashing a kernel/DTB built from a clean bootstrap will soft-brick/hang the device at boot.
Exploit/Trigger:
Running the documented bootstrap command `./scripts/bootstrap_build_env.sh` followed by kernel/DTB build and flash.
Recommendation:
Move experimental/unbootable reference patches (`0011`, `0012`) out of `devices/samsung/a21s/kernel/patches/` into a separate directory (e.g., `devices/samsung/a21s/kernel/patches-experimental/`) or drive `bootstrap_build_env.sh` from an explicit `series` manifest file rather than a wildcard `*.patch` glob.
References: CWE-665 (Improper Initialization), Build/Release Reliability Defect
```

```
ID: KERN-010
Title: Stack Buffer Overflows and Unterminated Stack Buffer in Out-of-Tree Vendor Drivers (scsc_bt, scsc_wlan, abox)
Severity: Low
Confidence: Confirmed
Location: socs/exynos850/drivers/scsc/bt/scsc_bt_module.c:67-70,2152-2155,2192-2205, socs/exynos850/drivers/scsc/wlan/udi.c:723-735, socs/exynos850/drivers/abox/aud3004x-dump.c:434-442
Evidence:
# 1. socs/exynos850/drivers/scsc/bt/scsc_bt_module.c:67-70,2192-2205
#define SCSC_BTLOG_MAX_STRING_LEN       (37)
#define SCSC_BTLOG_BUF_LEN              (19)
#define SCSC_BTLOG_BUF_MAX_CHAR_TO_COPY (16)
...
char btlog_enables_buf[SCSC_BTLOG_BUF_LEN + 1]; // 20 bytes [0..19]
u32 start_index = buffer_len - SCSC_BTLOG_BUF_MAX_CHAR_TO_COPY - newline_len;
...
u32 char_to_copy = start_index; // when buffer_len == 36 and newline_len == 0 -> char_to_copy == 20
memcpy(btlog_enables_buf, buffer, char_to_copy);
btlog_enables_buf[char_to_copy] = '\0'; // writes btlog_enables_buf[20] -> 1-byte stack OOB write!

# 2. socs/exynos850/drivers/scsc/wlan/udi.c:723-735
char build_info[200];
...
mxman_get_fw_version(build_info, 200);
len = strlen(build_info);
sprintf(build_info + len, " "); // OOB stack write if len >= 199
if (len >= 200) { ... }
mxman_get_driver_version(build_info + len + 1, 200 - len - 1); // negative size if len >= 200

# 3. socs/exynos850/drivers/abox/aud3004x-dump.c:434-442
char input[MAX_STRING_LENGTH];
if (strlen(buf) > MAX_STRING_LENGTH) { ... return 0; }
memcpy(input, buf, strlen(buf)); // no NUL terminator copied!
ret = command_parsing(input);    // reads past stack buffer
Impact:
1. `scsc_bt_module.c`: Writing a 36-byte hex string without trailing newline to the `firmware_btlog_enables` module parameter causes a confirmed 1-byte NUL stack buffer overflow (`btlog_enables_buf[20]`).
2. `udi.c`: If firmware returns a >=199-byte version string, `UNIFI_GET_SW_VERSION` ioctl overflows `build_info[200]` on the stack and passes a negative length to `mxman_get_driver_version()`.
3. `aud3004x-dump.c`: Writing to `codec_sysfs` passes an uninitialized, non-NUL-terminated stack buffer `input` to `command_parsing()`, causing stack out-of-bounds read.
All three are compounded by `CONFIG_FORTIFY_SOURCE` being disabled (KERN-006).
Exploit/Trigger:
1 & 3 require root write to module param / sysfs (`0644`); 2 requires malicious/corrupted WLAN firmware string plus `UNIFI_GET_SW_VERSION` ioctl.
Recommendation:
1. In `scsc_bt_module.c`, bound `char_to_copy < sizeof(btlog_enables_buf)` (or reduce `SCSC_BTLOG_MAX_STRING_LEN` to 35 + newline).
2. In `udi.c`, replace `sprintf` with `scnprintf` and guard `len < sizeof(build_info) - 2` before appending `" "` and calling `mxman_get_driver_version()`.
3. In `aud3004x-dump.c`, use `strscpy(input, buf, sizeof(input))` and check `count >= sizeof(input)`.
References: CWE-121 (Stack-based Buffer Overflow), CWE-170 (Improper Null Termination)
```

```
ID: KERN-011
Title: Bloated Generic Server/Desktop Kernel Surface Compiled into Mobile Phone Kernel
Severity: Low
Confidence: Confirmed
Location: devices/samsung/a21s/kernel/a21s_linux_defconfig:41,60,68-69,166,441,984-993,1132-1136, build/linux/.config:521,718,785,3581,7224,7309
Evidence:
# devices/samsung/a21s/kernel/a21s_linux_defconfig:
41:CONFIG_XEN=y
60:CONFIG_ACPI=y
68:CONFIG_VIRTUALIZATION=y
69:CONFIG_KVM=y
166:CONFIG_PCI=y
441:CONFIG_LEGACY_PTY_COUNT=16
984:CONFIG_VFIO=y
993:CONFIG_STAGING=y
1132:CONFIG_NFS_FS=y
1136:CONFIG_9P_FS=m
Impact:
`a21s_linux_defconfig` appears derived from mainline `arch/arm64/configs/defconfig` without pruning unrelated server/hypervisor/PCIe/staging subsystems. Compiling Xen, ACPI, KVM, VFIO, PCIe host controllers, NFS, 9p, legacy PTYs, and staging drivers into a phone kernel unnecessarily increases binary size, boot memory footprint, and the kernel attack surface exposed to local privilege escalation bugs.
Exploit/Trigger:
Always present in the compiled kernel image.
Recommendation:
Prune unused server/hypervisor/bus/filesystem options (`XEN`, `ACPI`, `KVM`, `VFIO`, `PCI`, `STAGING`, `LEGACY_PTYS`, `NFS_FS`, `9P_FS`, unused SoC Ethernet/Wi-Fi/SATA/SAS drivers) from `a21s_linux_defconfig` to minimize kernel attack surface.
References: Principle of Least Functionality (NIST SP 800-53 CM-7)
```

```
ID: KERN-012
Title: Baseline ARM64 Kernel Hardening Flags Verified Enabled in Resolved build/linux/.config (Clarifying Savedefconfig Omissions)
Severity: Info
Confidence: Confirmed
Location: build/linux/.config:132,523,551,569,611,848-849,853-854,904,909,911,7487,9557
Evidence:
132:CONFIG_BPF_UNPRIV_DEFAULT_OFF=y
523:CONFIG_UNMAP_KERNEL_AT_EL0=y
551:CONFIG_ARM64_PTR_AUTH=y
569:CONFIG_ARM64_BTI=y
611:CONFIG_RANDOMIZE_BASE=y
848:CONFIG_SECCOMP=y
849:CONFIG_SECCOMP_FILTER=y
853:CONFIG_STACKPROTECTOR=y
854:CONFIG_STACKPROTECTOR_STRONG=y
904:CONFIG_VMAP_STACK=y
909:CONFIG_STRICT_KERNEL_RWX=y
911:CONFIG_STRICT_MODULE_RWX=y
7487:CONFIG_IOMMU_DEFAULT_DMA_STRICT=y
9557:CONFIG_STRICT_DEVMEM=y
Impact:
Informational clarification: `audit/tools/kernel-hardening.txt` scanned `a21s_linux_defconfig` (a minimal `savedefconfig`), which omits Kconfig options that default to `y` on Linux 7.3.0 arm64. Direct verification of `build/linux/.config` confirms that KASLR, `STACKPROTECTOR_STRONG`, `STRICT_KERNEL_RWX`, `STRICT_MODULE_RWX`, `VMAP_STACK`, `SECCOMP_FILTER`, `BPF_UNPRIV_DEFAULT_OFF`, `UNMAP_KERNEL_AT_EL0`, `STRICT_DEVMEM`, and `IOMMU_DEFAULT_DMA_STRICT` ARE enabled in the built kernel.
Note (Fact vs Hardware): Although `CONFIG_ARM64_PTR_AUTH=y` and `CONFIG_ARM64_BTI=y` are enabled in `.config`, Exynos 850 uses ARM Cortex-A55 (ARMv8.2-A) cores, which do not implement ARMv8.3 PAC or ARMv8.5 BTI in hardware; those instructions execute as NOPs on SM-A217F.
Exploit/Trigger: N/A (informational).
Recommendation:
Keep these enabled; optionally add a CI check against the resolved `.config` (rather than only `savedefconfig`) using `kconfig-hardened-check` to prevent regressions.
References: Linux arm64 Kconfig defaults
```

---

## Positive Observations

1. **Clean, defensive custom TMU driver (`socs/exynos850/drivers/tmu/exynos850-tmu.c`)**:
   - Validates `resource_size(res)` before MMIO reads (`exynos850-tmu.c:145-146`), validates eFuse trim values (`sensor->code_85 <= sensor->code_25`, preventing divide-by-zero at line 72–73, `exynos850-tmu.c:161-164`), uses read-only `0444` hwmon attributes (`exynos850-tmu.c:53`), and leaves firmware-programmed hardware thermal thresholds intact.
2. **Overflow-safe bounds check in `cpif_boot.c` firmware upload (`socs/exynos850/drivers/cpif/cpif_boot.c:397-401`)**:
   - `CPIF_IOC_LOAD_CP_IMAGE` checks `!img.len || img.len > c->cp_size || img.m_offset > c->cp_size - img.len` (avoiding integer wraparound) and streams in bounded 64 KB chunks (`cpif_boot.c:412-419`).
3. **Bounds checks on `UNIFI_SET_MIB` / `UNIFI_GET_MIB` (`socs/exynos850/drivers/scsc/wlan/udi.c:437,506,543`)**:
   - Checks `mib_data_length > UDI_MIB_SET_LEN_MAX || mib_data_size > UDI_MIB_SET_LEN_MAX || mib_data_length > mib_data_size` before `kmalloc` and `copy_from_user`/`copy_to_user`.
4. **Clean, minimal userland patches (`patches/mesa/0001-wayland-opt-in-scanout-buffers.patch`, `patches/squeekboard/0001-clipboard-paste.patch`)**:
   - `squeekboard`'s `clipboard.rs` uses `Command::new("wl-paste").args([...])` (no shell interpolation) and validates UTF-8/NUL via `CString::new`.
5. **Strong default ARM64 baseline protections in resolved `.config` (`build/linux/.config`)**:
   - `CONFIG_RANDOMIZE_BASE=y`, `CONFIG_STACKPROTECTOR_STRONG=y`, `CONFIG_STRICT_KERNEL_RWX=y`, `CONFIG_STRICT_MODULE_RWX=y`, `CONFIG_VMAP_STACK=y`, `CONFIG_SECCOMP_FILTER=y`, `CONFIG_BPF_UNPRIV_DEFAULT_OFF=y`, `CONFIG_UNMAP_KERNEL_AT_EL0=y`, and `CONFIG_IOMMU_DEFAULT_DMA_STRICT=y` are active.

---

## Reviewed Files Machine List

```files
audit/tools/kernel-hardening.txt|reviewed|direct-read|tool-triage
audit/tools/binary-blobs.txt|reviewed|direct-read|tool-triage
devices/samsung/a21s/kernel/a21s_linux_defconfig|reviewed|direct-read|full-config-audit
build/linux/.config|reviewed|targeted-read|resolved-kconfig-verification
devices/samsung/a21s/kernel/patches/0001-a21s-wlbt-wifi-bt-dts.patch|reviewed|direct-read|kernel-patch-audit
devices/samsung/a21s/kernel/patches/0002-asoc-topology-visible.patch|reviewed|direct-read|kernel-patch-audit
devices/samsung/a21s/kernel/patches/0003-a21s-abox-audio-dts.patch|reviewed|direct-read|kernel-patch-audit
devices/samsung/a21s/kernel/patches/0004-exynos-drm-dma-iommu-optional.patch|reviewed|direct-read|kernel-patch-audit
devices/samsung/a21s/kernel/patches/0005-a21s-power-modem-dts.patch|reviewed|direct-read|kernel-patch-audit
devices/samsung/a21s/kernel/patches/0006-s2mu106-fuelgauge-compatible.patch|reviewed|direct-read|kernel-patch-audit
devices/samsung/a21s/kernel/patches/0007-a21s-sma1303-amp-dts.patch|reviewed|direct-read|kernel-patch-audit
devices/samsung/a21s/kernel/patches/0008-a21s-sensors-regulators-dts.patch|reviewed|direct-read|kernel-patch-audit
devices/samsung/a21s/kernel/patches/0009-a21s-tmu-thermal-zones.patch|reviewed|direct-read|kernel-patch-audit
devices/samsung/a21s/kernel/patches/0010-dsi-underrun-link-recovery.patch|reviewed|direct-read|kernel-patch-audit
devices/samsung/a21s/kernel/patches/0011-a21s-cpu-idle-states.patch|reviewed|direct-read|kernel-patch-audit
devices/samsung/a21s/kernel/patches/0012-a21s-et520-spi-dts.patch|reviewed|direct-read|kernel-patch-audit
devices/samsung/a21s/kernel/patches/0013-a21s-decon850-keep-win0-mapcolor-on-disable.patch|reviewed|direct-read|kernel-patch-audit
patches/mesa/0001-wayland-opt-in-scanout-buffers.patch|reviewed|direct-read|patch-audit
patches/squeekboard/0001-clipboard-paste.patch|reviewed|direct-read|patch-audit
patches/squeekboard/README.md|reviewed|direct-read|patch-audit
rootfs-overlay/etc/sysctl.d/10-a21s-quiet.conf|reviewed|direct-read|sysctl-audit
rootfs-overlay/etc/modules-load.d/a21s-hardware.conf|reviewed|direct-read|module-autoload-audit
rootfs-overlay/etc/udev/rules.d/99-a21s-modem-fp.rules|reviewed|targeted-read|cross-check-fp-perms
rootfs-overlay/etc/udev/rules.d/99-a21s-sensors-vib.rules|reviewed|targeted-read|cross-check-vib-perms
scripts/bootstrap_build_env.sh|reviewed|targeted-read|verify-patch-loop
socs/exynos850/drivers/fingerprint/a21s_et5xx.c|reviewed|direct-read|custom-driver-audit
socs/exynos850/drivers/tmu/exynos850-tmu.c|reviewed|direct-read|custom-driver-audit
socs/exynos850/drivers/vibrator/a21s_vibrator.c|reviewed|direct-read|custom-driver-audit
socs/exynos850/drivers/cpif/cpif_core.c|reviewed|targeted-read|custom-driver-audit
socs/exynos850/drivers/cpif/cpif_boot.c|reviewed|targeted-read|custom-driver-audit
socs/exynos850/drivers/cpif/cpif_power.c|reviewed|targeted-read|custom-driver-audit
socs/exynos850/drivers/sensorhub/a21s_chub.c|reviewed|targeted-read|custom-driver-audit
socs/exynos850/drivers/scsc/misc/mxproc.c|reviewed|targeted-read|procfs-perms-audit
socs/exynos850/drivers/scsc/wlan/udi.c|reviewed|targeted-read|ioctl-usercopy-audit
socs/exynos850/drivers/scsc/wlan/scsc_wlan_mmap.c|reviewed|direct-read|mmap-chardev-audit
socs/exynos850/drivers/scsc/bt/scsc_bt_module.c|reviewed|targeted-read|param-overflow-audit
socs/exynos850/drivers/abox/aud3004x-dump.c|reviewed|targeted-read|sysfs-stack-audit
socs/exynos850/drivers/scsc/scsc_bt.ko.zst|reviewed|targeted-read|binary-blob-signature-vermagic
socs/exynos850/drivers/scsc/scsc_mx.ko.zst|reviewed|targeted-read|binary-blob-signature-vermagic
socs/exynos850/drivers/scsc/scsc_platform_mif.ko.zst|reviewed|targeted-read|binary-blob-signature-vermagic
socs/exynos850/drivers/scsc/scsc_wlan.ko.zst|reviewed|targeted-read|binary-blob-signature-vermagic
socs/exynos850/drivers/scsc/|reviewed|rg-scan|risky-api-and-perms-scan-remaining-files
socs/exynos850/drivers/abox/|reviewed|rg-scan|risky-api-and-perms-scan-remaining-files
socs/exynos850/drivers/acpm/|reviewed|rg-scan|risky-api-and-perms-scan-remaining-files
socs/exynos850/drivers/cpif/|reviewed|rg-scan|risky-api-and-perms-scan-remaining-files
socs/exynos850/drivers/s2mu106/|reviewed|rg-scan|risky-api-and-perms-scan-remaining-files
socs/exynos850/drivers/vendor-include/|skipped|none|vendor-include
```
