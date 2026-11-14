# Engineering Handoff — Galaxy A21s (`SM-A217F` / Exynos 850)

Last updated: 2026-11-14 (verified on live hardware)

---

## 1. Current Verified Device State

- **Kernel:** Mainline Linux `7.3.0-g14a425b3a7dc-dirty` (`#7`, `LLVM=1` Clang) with `exynos850-a217f.dtb` (`patches/linux/0001-a21s-wlbt-wifi-bt-dts.patch` applied).
- **Boot & Display:** `uniLoader` (`06a1bba`) + `decon850` + `dsi` (`td4150`) + `panfrost`/`panvk` (`/dev/dri/renderD128`) + `steamos-session.service` (`active`).
- **Networking:**
  - `usb0`: `172.16.42.1/24` managed by `systemd-networkd` (`Metric=700`), `unmanaged` in NetworkManager.
  - `wlan0`: `scsc_wlan.ko` managed by `NetworkManager` (`wifi.scan-rand-mac-address=no`), visible to Steam `-gamepadui` over `org.freedesktop.NetworkManager` D-Bus.
- **Bluetooth & Gamepads:**
  - `scsc_bt.ko` (`hci0`, `F0:00:01:00:A2:17`) + BlueZ `5.88` + PipeWire/WirePlumber (`gamer` user session).
  - Kernel HID stack enabled (`CONFIG_UHID=y`, `CONFIG_HIDRAW=y`, `CONFIG_INPUT_JOYDEV=y`, `CONFIG_BT_HIDP=m`, `CONFIG_HID_{MICROSOFT,PLAYSTATION,NINTENDO,STEAM}=m`, `CONFIG_JOYSTICK_XPAD=m`). `/dev/uhid` is present; the temporary `xbox-ble-uinput` stopgap has been removed from the device.
- **Audio (`abox` safety guard):**
  - `/etc/modprobe.d/blacklist-abox.conf` on the device blacklists `abox_snd`, `aud3004x_codec`, and `exynos3830_aud3004x` so `udev` does not auto-load them at boot.
  - Bluetooth audio (`bluez5` SPA) and USB-C audio (`snd-usb-audio.ko`) work without `abox`.

---

## 2. Useful Tools & Commands

### Bootstrap & Build (Fresh Machine)
```bash
# 1. Clone upstream linux (14a425b3a) + uniLoader (06a1bba), apply DTS patch, prep defconfig
./scripts/bootstrap_build_env.sh

# 2. Build kernel, DTBs, and in-tree modules with LLVM/Clang
make -C src/linux O=../../build/linux ARCH=arm64 LLVM=1 -j$(nproc) Image dtbs modules

# 3. Build out-of-tree SCSC WLBT modules (Wi-Fi + BT)
make -C src/linux O=../../build/linux ARCH=arm64 LLVM=1 -j$(nproc) M=$PWD/drivers/scsc modules
```

### Safe Module Deployment (Arch Linux ARM)
> **Never overwrite `/lib`** — `/lib` is a symlink to `usr/lib`. Always stage to a temp directory and extract into `/usr/lib/modules/`:
```bash
rm -rf /tmp/a21s-mods
make -C src/linux O=../../build/linux ARCH=arm64 LLVM=1 INSTALL_MOD_PATH=/tmp/a21s-mods modules_install
make -C src/linux O=../../build/linux ARCH=arm64 LLVM=1 INSTALL_MOD_PATH=/tmp/a21s-mods M=$PWD/drivers/scsc modules_install
tar -C /tmp/a21s-mods/lib/modules -cf - "$(uname -r)" | ssh root@172.16.42.1 "tar -C /usr/lib/modules -xf - && depmod -a"
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
- **Download Mode recovery:** Enter fresh Download Mode (`Power + Vol Down` ~7–10 s, then `Vol Up + Vol Down` + plug USB-C, then `Vol Up`), and run `heimdall flash --BOOT build/images/boot-avb.img` directly (**do not** run `heimdall detect` right before `heimdall flash`, as `detect` consumes the Loke handshake).

---

## 3. Key Gotchas & Lessons Learned (Things to Watch Out For)

1. **`boot.img` packing parameters:**
   - Use **uncompressed** `src/uniLoader/uniLoader` (not `uniLoader.gz`) and the exact `src/tools/AIK-Linux/split_img/boot.img-*` metadata (`boot.img-dtb`, `boot.img-cmdline`, `boot.img-os_version`, `boot.img-os_patch_level`) + `avbtool.py`. Missing/mismatched AVB footer, `dtb_offset`, or `uniLoader` binary drops S-Boot straight into Download Mode.
   - The live rootfs is **Arch Linux ARM64** on `/dev/mmcblk0p31` (not postmarketOS UUIDs).
2. **Keep `CONFIG_EXYNOS_IOMMU=n` for now:**
   - Enabling `CONFIG_EXYNOS_IOMMU=y` breaks `exynos-drm` (`decon850_component_ops` fails bind with `-19` / `-ENODEV`), causing a **black screen** on boot. Any ABOX work must either run without `EXYNOS_IOMMU` or keep `exynos_drm_dma` unaffected.
3. **Device Tree `#address-cells` under `&soc`:**
   - `&soc` in `exynos850.dtsi` uses `#address-cells = <1>; #size-cells = <1>;`. New peripheral nodes (e.g. `abox`) must use 1-cell addresses (`reg = <0x14a50000 0x10000>`), not 2-cell (`<0x0 0x14a50000 0x10000>`), which caused `-14` (`-EFAULT`) in `abox-gic` and `abox` probes.
   - Check `patches/linux/0003-a21s-abox-audio-dts.patch` for brace balance and 1-cell `reg` formatting before applying it.
4. **ABOX `AUD` Power Domain (`pd_aud`) & Probe Safety:**
   - `exynos850.dtsi` does not define `pd_aud` power domains yet. Accessing ABOX SFRs (`0x14a50000`) while the `AUD` power domain is off hangs the SoC.
   - Keep `abox_snd` blacklisted in `/etc/modprobe.d/` while developing and test with `modprobe`/`insmod` over SSH instead of auto-loading via `udev`.
5. **`aud3004x` ACPM Bus:**
   - `exynos_acpm_read_reg()` in `drivers/vendor-include/soc/samsung/acpm_mfd.h` is a no-op stub returning `0` when `CONFIG_EXYNOS_ACPM` is unset. Wire `aud3004x` to mainline `devm_acpm_get_by_node()` (`<linux/firmware/samsung/exynos-acpm-protocol.h>`) against `&acpm_ipc`.
6. **Always keep `clk_ignore_unused` and `wifibt_if@b0000000`:**
   - Removing `clk_ignore_unused` from `chosen/bootargs` causes an immediate SoC reset on WLBT `SM_MSG_START_REQ`.
