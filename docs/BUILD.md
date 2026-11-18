# RabbizOS Build & Patch Guide (Galaxy A21s `SM-A217F`)

Because `src/` and `build/` are gitignored, a fresh clone only contains tracked driver sources (`drivers/scsc/`), kernel patches (`patches/linux/`), and the kernel defconfig (`configs/a21s_linux_defconfig`).

---

## 1. Prerequisites (Build Host)

Install ARM64 cross-compilation and Android boot image tools:
- **Ubuntu/Debian/Arch (recommended, matches current build):** `clang lld llvm bc bison flex openssl make git mkbootimg` (`LLVM=1` — zero GNU cross-compiler prefix needed)
- **macOS (OrbStack / Docker / Lima Linux container recommended):** `clang lld llvm bc bison flex openssl` inside a Linux container.

---

## 2. Bootstrap `src/` and `build/`

Run the bootstrap script from the repository root:

```bash
chmod +x scripts/bootstrap_build_env.sh
./scripts/bootstrap_build_env.sh
```

What this sets up automatically:
1. Clones `https://github.com/samsungexynos850/linux.git` (`staging` @ `14a425b3a`) into `src/linux`.
2. Applies **`patches/linux/0001-a21s-wlbt-wifi-bt-dts.patch`** to `arch/arm64/boot/dts/exynos/exynos850-a217f.dts`:
   - Adds `clk_ignore_unused` to `chosen/bootargs` (prevents `clk_disable_unused()` SoC reset on WLBT `SM_MSG_START_REQ`).
   - Reserves `wifibt_if` at `0xb0000000` (`0xc00000`).
   - Adds `&soc` syscon nodes (`i3c_apm_pmic`, `dbus_baaw`, `pbus_baaw`, `wlbt_remap`, `boot_cfg`) and `scsc_wifibt@119c0000`.
3. Clones `https://github.com/samsungexynos850/uniLoader.git` (`06a1bba`) into `src/uniLoader`.
4. Copies `configs/a21s_linux_defconfig` to `build/linux/.config`, runs `olddefconfig`, and runs `modules_prepare`.

---

## 3. Enabling Extra Kernel Options (e.g. `CONFIG_UHID` for BLE Gamepads)

If you need to enable options like the BlueZ `input-hog` HID stack (`docs/a21s-ble-gamepad.md` §4):

```bash
src/linux/scripts/config --file build/linux/.config \
  -e CONFIG_UHID -e CONFIG_HIDRAW -e CONFIG_INPUT_JOYDEV \
  -m CONFIG_HID_MICROSOFT -m CONFIG_HID_PLAYSTATION \
  -m CONFIG_HID_NINTENDO -m CONFIG_HID_STEAM -m CONFIG_XPAD

make -C src/linux O=../../build/linux ARCH=arm64 LLVM=1 olddefconfig
```

*(After testing, refresh `configs/a21s_linux_defconfig` with `make -C src/linux O=../../build/linux ARCH=arm64 savedefconfig && cp build/linux/defconfig configs/a21s_linux_defconfig`.)*

---

## 4. Build Kernel (`Image` + `dtbs`) and SCSC Modules (`drivers/scsc/`)

```bash
# 1. Build kernel Image, DTBs, and in-tree modules (e.g. HID_*.ko)
make -C src/linux O=../../build/linux ARCH=arm64 LLVM=1 -j$(nproc) Image dtbs modules

# 2. Build out-of-tree SCSC WLBT modules (scsc_platform_mif, scsc_mx, scsc_wlan, scsc_bt)
make -C src/linux O=../../build/linux ARCH=arm64 LLVM=1 M=$PWD/drivers/scsc modules
```

---

## 5. Deploy & Test Safely

### A. Deploy Modules (NEVER overwrite `/lib`!)
On Arch Linux ARM, `/lib` is a symlink to `usr/lib`. Always install modules under `/usr/lib/modules/$(uname -r)/`:

```bash
# Out-of-tree SCSC modules
scp drivers/scsc/*.ko root@<wifi-ip>:/usr/lib/modules/7.3.0-g14a425b3a7dc/extra/
ssh root@<wifi-ip> "depmod -a"

# If you also built new in-tree modules (like hid-microsoft.ko):
make -C src/linux O=../../build/linux ARCH=arm64 INSTALL_MOD_PATH=/tmp/a21s-mods modules_install
# Copy /tmp/a21s-mods/lib/modules/* -> /usr/lib/modules/ on device (never touch /lib directly!)
```

### B. Test New Kernel with `kexec` (Before Flashing `boot.img`)
`CONFIG_KEXEC=y` is enabled on the device:

```bash
scp build/linux/arch/arm64/boot/Image root@<wifi-ip>:/tmp/Image
scp build/linux/arch/arm64/boot/dts/exynos/exynos850-a217f.dtb root@<wifi-ip>:/tmp/exynos850-a217f.dtb
ssh root@<wifi-ip> "kexec -l /tmp/Image --dtb=/tmp/exynos850-a217f.dtb --reuse-cmdline && systemctl kexec"
```

### C. Pack & Flash `boot-avb.img` (`uniLoader`)
1. Copy `build/linux/arch/arm64/boot/Image`, `exynos850-a217f.dtb`, and your `ramdisk` into `src/uniLoader/blob/`.
2. Build `uniLoader` with `a21s_defconfig` (**uncompressed** `src/uniLoader/uniLoader`).
3. Pack `build/images/boot-avb.img` via `mkbootimg` with `--dtb <stock-boot.img-dtb> --dtb_offset 0x00000000` and flash to `/dev/mmcblk0p18` (`dd`).
