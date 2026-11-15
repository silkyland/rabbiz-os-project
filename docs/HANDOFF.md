# Engineering Handoff — Galaxy A21s (`SM-A217F` / Exynos 850)

Last updated: 2026-11-15

---

## 1. Current Verified Device State

- **Kernel:** Mainline Linux `7.3.0-g14a425b3a7dc-dirty` (`LLVM=1` Clang) with `exynos850-a217f.dtb` (`patches/linux/0001`..`0004` applied).
  - `CONFIG_EXYNOS_IOMMU=y` is enabled in `build/linux/.config` and on the live kernel together with `patches/linux/0004-exynos-drm-dma-iommu-optional.patch` so `decon850` / display continues working. (`configs/a21s_linux_defconfig` has not yet been synced with `CONFIG_EXYNOS_IOMMU=y` — sync it once ABOX is finalized.)
- **Boot & Display:** `uniLoader` (`06a1bba`) + `decon850` + `dsi` (`td4150`) + `panfrost`/`panvk` (`/dev/dri/renderD128`) + `steamos-session.service` (`active`).
- **Networking:**
  - **USB (`usb0`, preferred):** `172.16.42.1/24` (`systemd-networkd`, `unmanaged` in NetworkManager) over NCM gadget `18d1:d001` (`enp119s0u1` on Linux host, NetworkManager `"Wired connection 3"` at `172.16.42.2/24`). Reachable at `ssh root@172.16.42.1` (~1.7 ms).
  - **Wi-Fi (`wlan0`):** `scsc_wlan.ko` managed by `NetworkManager` (`wifi.scan-rand-mac-address=no`), reachable at `ssh root@192.168.1.142`.
- **Bluetooth & Gamepads:**
  - `scsc_bt.ko` (`hci0`, `F0:00:01:00:A2:17`) + BlueZ `5.88` + PipeWire/WirePlumber (`gamer` user session) + kernel HID/UHID stack.
- **Audio (`abox` Phase A in progress):**
  - `/etc/modprobe.d/blacklist-abox.conf` on the device still blacklists `abox_snd`, `aud3004x_codec`, and `exynos3830_aud3004x` so `udev` does not auto-load them at boot.
  - `abox-snd.ko` + `exynos3830-aud3004x.ko` probe cleanly when loaded manually (`modprobe exynos3830-aud3004x`) and register ALSA sound card `1 [Exynos3830Mader]`.
  - `drivers/abox/abox.c` has the broken `DIAG SMMU` `ioremap` (`0x8100`/`0x810c` external abort) removed, keeps `clk_prepare_enable(clk_sysmmu)` (`258048030` Hz), and calls `exynos_smc(SMC_CMD_PREAPRE_PD_ONOFF, 1, 0x14810204, 2)` (vendor `pd-aud` `need_smc = <0x14810204>` TZPC restore, returns `0`).

---

## 2. Current Blocker & Verified Findings (Calliope DSP Core 0)

- **Symptom:** `modprobe exynos3830-aud3004x` probes ALSA card 1, then Calliope firmware boot fails with `Invalid calliope state: 2` (`CALLIOPE_ENABLING`).
- **What we proved:**
  1. **`14850000.sysmmu` IS working and translating (NOT bypassed):**
     - `14850000.sysmmu` (v7.4, `0x70800000`, `CAPA1=0x0000e041` `VCR_ENABLED | NO_BLOCK_MODE`) has **only VM0** (`0x8000=0x9`, `0x800c=pt_base`); `0x8040`/`0x8080`/`0x80c0`/`0x8100` are unmapped (Bus error on `/dev/mem`).
     - `iommu_iova_to_phys(0x80035600) == 0xfe035600` (`dram_phys=0xfe000000`), and `0.abox-debug/calliope_dram` matches `/lib/firmware/calliope_dram.bin` (`0x47df8` bytes).
     - When `abox_iommu_map(dev, IOVA_DRAM_FIRMWARE, ...)` was temporarily skipped, `14850000.sysmmu` immediately caught `[READ] PAGE FAULT occurred at 0x80000090` with Core 0 `pc : 800000a0` (confirmed in `/sys/fs/pstore/console-ramoops-0`).
  2. **Exact failure after Core 0 enters `calliope_dram.bin` (`0x80000000`):**
     - With `0x80000000` mapped, Core 0 runs `calliope_dram.bin`, reaches `r04=81200000 r05=80c00094 r09=81200000` (end of `0x1200000` DRAM window / BSS), and `calliope_log` (`0xb00000`) + `handshake` (`0x23800`) remain all zeros.
     - Inspecting `/sys/devices/platform/soc@0/14a50000.abox/0.abox-debug/calliope_sram` vs `/lib/firmware/calliope_sram.bin` shows **only the first 15 words (`0x00..0x38`, the ARM32 exception vector table in SRAM)** were overwritten with register/runtime values:
       - `0x00=0x00000000`, `0x04=0x00000000`, `0x08=0x00002818`, `0x0c=0x00002814`
       - `0x10=0x0000001d`, `0x14=0x14af1400`, `0x18=0xffffffff`, `0x1c=0x00000200`
       - `0x20=0x14af1c08`, `0x24=0x00000007`, `0x28=0x44418186`, `0x2c=0xa4030006`
       - `0x30=0x00005c16`, `0x34=0x0000dafc`, `0x38=0x81200000`
     - Those values (`0x00002818`, `0x00002814`, `0x14af1400`, `0xffffffff`, `0x00000200`, `0xa4030006`, `0x00005c16`, `0x81200000`) match Core 0's own registers (`r02..r12`) dumped via an `stm` to address `0x00000000` (NULL pointer dereference in SRAM!).
     - Core 0 then ends up trapped in `0x0c..0x28` / `0xb3a8` while Core 1 waits at `0x14464`..`0x14548`.

---

## 3. Immediate Next Steps

1. **Find why Calliope Core 0 writes/branches through `0x00000000` after `0x800000a0`:**
   - Rebuild & deploy clean `drivers/abox/abox-snd.ko` (the local `abox.c` has been cleaned of the temporary `skipping dram map` test, but not yet re-deployed after the last panic reboot).
   - Check `calliope_dram.bin` startup (around `0x00..0x200` and `0x35600`) / `samsung,abox-bootargs` (`0x23e00` in SRAM — note `abox.c` reads `samsung,drv-bootargs-area`, whereas `0003-a21s-abox-audio-dts.patch` has `samsung,abox-bootargs` + `samsung,abox-bootargs-offset = <0x23e00>`, and `sram[0x23e00]` was all zeros) or missing `abox` SFR / SRAM setup before Core 0 boots.
   - Once `Calliope is ready to sing` appears in `dmesg`, test playback:
     ```bash
     ssh root@172.16.42.1 "timeout 3 speaker-test -D plughw:1,0 -c 2 -r 48000 -t sine"
     ```
2. **Phase B — Wire `aud3004x-codec.ko` to mainline ACPM & DTS:**
   - Update `drivers/abox/aud3004x.c` (`aud3004x_acpm_{read,write,update}_reg()` at lines 61–103) to replace `exynos_acpm_*` stubs with `devm_acpm_get_by_node()` (`<linux/firmware/samsung/exynos-acpm-protocol.h>`) against `&acpm_ipc` (`acpm_chan_id = 2`, `acpm->ops->pmic.{read_reg,write_reg,update_reg}`).
   - Add the `aud3004x` node + UAIF0 codec phandle to `patches/linux/0003-a21s-abox-audio-dts.patch`, rebuild DTB/modules, deploy, and verify physical speaker/headphone output.
3. **Finalize:**
   - Remove `/etc/modprobe.d/blacklist-abox.conf` on the device once boot-safe.
   - Sync `configs/a21s_linux_defconfig` with `CONFIG_EXYNOS_IOMMU=y` and audio config.

---

## 4. Linux Host Setup & Useful Commands

### Bootstrap & Build
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

# Quick single-module deploy over USB (or 192.168.1.142 over Wi-Fi):
scp drivers/abox/abox-snd.ko root@172.16.42.1:/usr/lib/modules/$KREL/extra/
```

### Rebooting Over SSH
```bash
ssh root@172.16.42.1 "depmod -a && sync && reboot"
# After reboot, bring up USB connection if needed:
nmcli connection up "Wired connection 3" || true
```

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
7. **USB NCM (`usb0`) recovery:** If `18d1:d001` does not appear on the host after a hard power event, toggling Download Mode (`04e8:685d`) and rebooting resets the S2MU106 MUIC D+/D- switch.
