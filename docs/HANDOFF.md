# Engineering Handoff — Galaxy A21s (`SM-A217F` / Exynos 850)

Last updated: 2026-11-15 (moving dev environment to Linux host)

---

## 1. Current Verified Device State

- **Kernel:** Mainline Linux `7.3.0-g14a425b3a7dc-dirty` (`LLVM=1` Clang) with `exynos850-a217f.dtb` (`patches/linux/0001`..`0004` applied).
  - `CONFIG_EXYNOS_IOMMU=y` is enabled on the live kernel together with `patches/linux/0004-exynos-drm-dma-iommu-optional.patch` so `decon850` / display continues working. (Note: `configs/a21s_linux_defconfig` has not yet been synced with `CONFIG_EXYNOS_IOMMU=y` — sync it once ABOX is finalized.)
- **Boot & Display:** `uniLoader` (`06a1bba`) + `decon850` + `dsi` (`td4150`) + `panfrost`/`panvk` (`/dev/dri/renderD128`) + `steamos-session.service` (`active`).
- **Networking:**
  - `wlan0`: `scsc_wlan.ko` managed by `NetworkManager` (`wifi.scan-rand-mac-address=no`), reachable over Wi-Fi at `root@192.168.1.142`.
  - `usb0`: `172.16.42.1/24` (`systemd-networkd`, `unmanaged` in NetworkManager). Currently showing `<NO-CARRIER>` on the previous macOS host (not a Download Mode issue; check USB cable/port/RNDIS on the Linux host).
- **Bluetooth & Gamepads:**
  - `scsc_bt.ko` (`hci0`, `F0:00:01:00:A2:17`) + BlueZ `5.88` + PipeWire/WirePlumber (`gamer` user session) + kernel HID/UHID stack.
- **Audio (`abox` Phase A in progress):**
  - `/etc/modprobe.d/blacklist-abox.conf` on the device still blacklists `abox_snd`, `aud3004x_codec`, and `exynos3830_aud3004x` so `udev` does not auto-load them at boot.
  - `abox-snd.ko` + `exynos3830-aud3004x.ko` probe cleanly when loaded manually (`modprobe exynos3830-aud3004x`) and register ALSA sound card `1 [Exynos3830Mader]`.
  - `drivers/abox/abox.c` currently has the `clk_sysmmu` (`gout_aud_sysmmu_clk`) fix verified (`258048030` Hz) **plus** a temporary `DIAG SMMU` print in `abox_enable()` reading `vm1`/`pt1` (`0x8100`/`0x810c`) and `sysreg` (`0x400`/`0x404`/`0x40c`). That diagnostic `abox-snd.ko` was deployed and the device was rebooted right before this handoff.

---

## 2. Current Blocker & Exact Root Cause (Calliope DSP Core 0)

- **Symptom:** `modprobe exynos3830-aud3004x` probes ALSA card 1, then Calliope firmware boot fails with `Invalid calliope state: 2`.
- **What we proved:**
  1. `14850000.sysmmu` is SysMMU v7 with VCR (`sysmmu_v7_vm_variant`), and VM 0 is enabled (`ctrl=0x5`, `ctrl_vm=0x9`, `pt_vm=0xc6584` / `0x94f80`, `ver=0x70800000`).
  2. `0.abox-debug/calliope_dram` (`data->dram_base`, phys `0xfe000000`) **does** contain valid firmware (`CLP80012` at `0x0`, valid ARM instructions at `0x35600`).
  3. Calliope Core 0 Reset (`0xb194` -> `0x0040` -> `0x0058 __scatterload`) jumps (`0x004c: bx r0`) to `0x80035600`.
  4. Core 0's AXI reads at `0x80035600` (and `0x8000009d`) are **bypassing `14850000.sysmmu` VM 0** (`0` SysMMU faults): Core 0 reads `0x00000000`s from free physical `System RAM` at `0x80000000` (Linux `Kernel code` starts at `0x90000000` in `/proc/iomem`), slides to `0x81200000`, and hits Prefetch Abort (`0x002c` -> `0xb320` -> `0xaaa8` -> `0x0114` -> `0x8000009d`), trapping Core 0 at `pc=0x0c..0x1c`. Meanwhile Core 1 jumps to `0x11000` (`calliope_sram_2.bin`) and runs (`pc=0x00014464`).
- **Next hypothesis to verify immediately:** Core 0's AXI transactions may be hitting a different VCR VM (`vm1`..`vm7` at `0x8100`, etc.) or SysMMU bypass in `SYSREG_AUD` (`0x14a10000`).

---

## 3. Immediate Next Steps (Resume Here on Linux)

1. **Check the `DIAG SMMU` output already deployed on the device:**
   ```bash
   ssh root@192.168.1.142 "modprobe exynos3830-aud3004x && sleep 2 && dmesg | grep -iE 'DIAG SMMU|Calliope is ready|Invalid calliope' | tail -n 15"
   ```
2. **Fix Calliope Core 0 SysMMU translation:**
   - If `vm1` (`0x8100`/`0x810c`) is valid and `0`, test propagating `pt0` + `ctrl_vm=0x9` to `vm1..vm7` or configuring `SYSREG_AUD` (`0x14a10000`) / `exynos850` vendor `sysmmu_aud` settings.
   - Once `Calliope is ready to sing` appears in `dmesg`, test playback:
     ```bash
     ssh root@192.168.1.142 "timeout 3 speaker-test -D plughw:1,0 -c 2 -r 48000 -t sine"
     ```
   - Remove the temporary `DIAG SMMU` print from `drivers/abox/abox.c` (keep the clean `clk_sysmmu` enable/disable).
3. **Phase B — Wire `aud3004x-codec.ko` to mainline ACPM & DTS:**
   - Update `drivers/abox/aud3004x.c` to replace the stub `exynos_acpm_{read,write,update}_reg()` calls with `devm_acpm_get_by_node()` (`<linux/firmware/samsung/exynos-acpm-protocol.h>`) against `&acpm_ipc` (`acpm_chan_id = 2`, `acpm->ops->pmic.{read_reg,write_reg,update_reg}`).
   - Add the `aud3004x` node + UAIF0 codec phandle to `patches/linux/0003-a21s-abox-audio-dts.patch`, rebuild DTB/modules, deploy, and verify physical speaker/headphone output.
4. **Finalize:**
   - Remove `/etc/modprobe.d/blacklist-abox.conf` on the device once boot-safe.
   - Sync `configs/a21s_linux_defconfig` (with `CONFIG_EXYNOS_IOMMU=y` and audio config).

---

## 4. Linux Host Setup & Useful Commands

### Bootstrap & Build (Linux Host — No Docker Needed)
```bash
# 1. Clone upstream linux (14a425b3a) + uniLoader (06a1bba), apply patches/linux/*.patch, prep defconfig
./scripts/bootstrap_build_env.sh

# 2. Ensure CONFIG_EXYNOS_IOMMU=y is enabled in build/linux/.config (until defconfig is synced)
./src/linux/scripts/config --file build/linux/.config -e EXYNOS_IOMMU
make -C src/linux O=../../build/linux ARCH=arm64 LLVM=1 olddefconfig

# 3. Build kernel, DTBs, and in-tree modules with LLVM/Clang
make -C src/linux O=../../build/linux ARCH=arm64 LLVM=1 -j$(nproc) Image dtbs modules

# 4. Build out-of-tree modules (SCSC WLBT + ABOX)
make -C src/linux O=../../build/linux ARCH=arm64 LLVM=1 -j$(nproc) M=$PWD/drivers/scsc modules
make -C src/linux O=../../build/linux ARCH=arm64 LLVM=1 -j$(nproc) M=$PWD/drivers/abox modules
```

### Safe Module Deployment (Arch Linux ARM)
> **Never overwrite `/lib`** — `/lib` is a symlink to `usr/lib`. Always deploy to `/usr/lib/modules/`:
```bash
KREL=$(make -s -C src/linux O=../../build/linux ARCH=arm64 kernelrelease)

# Quick single-module deploy (e.g. abox-snd.ko):
scp drivers/abox/abox-snd.ko root@192.168.1.142:/usr/lib/modules/$KREL/extra/

# Full module deploy:
rm -rf /tmp/a21s-mods
make -C src/linux O=../../build/linux ARCH=arm64 LLVM=1 INSTALL_MOD_PATH=/tmp/a21s-mods modules_install
make -C src/linux O=../../build/linux ARCH=arm64 LLVM=1 INSTALL_MOD_PATH=/tmp/a21s-mods M=$PWD/drivers/scsc modules_install
make -C src/linux O=../../build/linux ARCH=arm64 LLVM=1 INSTALL_MOD_PATH=/tmp/a21s-mods M=$PWD/drivers/abox modules_install
tar -C /tmp/a21s-mods/lib/modules -cf - "$KREL" | ssh root@192.168.1.142 "tar -C /usr/lib/modules -xf - && depmod -a"
```

### Rebooting Over SSH
Use `/proc/sysrq-trigger` (`/proc/sys/kernel/sysrq-trigger` does not exist):
```bash
ssh -o ConnectTimeout=5 -o ServerAliveInterval=1 -o ServerAliveCountMax=2 root@192.168.1.142 \
  "depmod -a && sync && echo b > /proc/sysrq-trigger" || true
```
Device takes ~100–105 s to come back on `192.168.1.142`.

### Packing & Flashing `boot-avb.img`
```bash
cp build/linux/arch/arm64/boot/Image src/uniLoader/blob/Image
cp build/linux/arch/arm64/boot/dts/exynos/exynos850-a217f.dtb src/uniLoader/blob/dtb
make -C src/uniLoader ARCH=aarch64 LLVM=1 -j$(nproc)

mkbootimg \
  --kernel src/uniLoader/uniLoader \
  --ramdisk src/uniLoader/blob/ramdisk \
  --dtb src/tools/AIK-Linux/split_img/boot.img-dtb \
  --dtb_offset 0x00000000 \
  --header_version 2 --pagesize 2048 --base 0x10000000 \
  --kernel_offset 0x00008000 --ramdisk_offset 0x01000000 --tags_offset 0x00000100 \
  --cmdline "$(cat src/tools/AIK-Linux/split_img/boot.img-cmdline)" \
  --os_version "$(cat src/tools/AIK-Linux/split_img/boot.img-os_version)" \
  --os_patch_level "$(cat src/tools/AIK-Linux/split_img/boot.img-os_patch_level)" \
  -o build/images/boot-linux.img

cp build/images/boot-linux.img build/images/boot-avb.img
python3 ./build/avbtool.py add_hash_footer \
  --image build/images/boot-avb.img \
  --partition_size 37748736 --partition_name boot --rollback_index 12
```
- **Download Mode recovery:** Enter fresh Download Mode (`Power + Vol Down` ~7–10 s, then `Vol Up + Vol Down` + plug USB-C, then `Vol Up`), and run `heimdall flash --BOOT build/images/boot-avb.img` directly (**do not** run `heimdall detect` right before `heimdall flash`).

---

## 5. Key Gotchas & Lessons Learned

1. **`boot.img` packing:** Use **uncompressed** `src/uniLoader/uniLoader` + exact `boot.img-*` metadata + `avbtool.py`. Live rootfs is Arch Linux ARM64 on `/dev/mmcblk0p31`.
2. **`CONFIG_EXYNOS_IOMMU=y` + `exynos-drm`:** Requires `patches/linux/0004-exynos-drm-dma-iommu-optional.patch`; without it, `decon850` fails with `-ENODEV` (`-19`) -> black screen.
3. **DT `#address-cells` under `&soc`:** `&soc` uses `#address-cells = <1>; #size-cells = <1>;`. Use 1-cell `reg` in `patches/linux/0003-a21s-abox-audio-dts.patch`.
4. **ABOX `AUD` Power Domain & Blacklist:** Keep `/etc/modprobe.d/blacklist-abox.conf` while developing and load manually via `modprobe` over SSH.
5. **`aud3004x` ACPM Bus:** `exynos_acpm_read_reg()` in `drivers/vendor-include/soc/samsung/acpm_mfd.h` is a no-op stub; Phase B must switch to mainline `devm_acpm_get_by_node()`.
6. **Always keep `clk_ignore_unused` and `wifibt_if@b0000000`:** Removing `clk_ignore_unused` causes an immediate SoC reset on WLBT `SM_MSG_START_REQ`.
