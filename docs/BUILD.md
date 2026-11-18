# RabbizOS Build & Patch Guide (Galaxy A21s `SM-A217F`)

Because `src/` and `build/` are gitignored, a fresh clone only contains tracked driver sources (`socs/exynos850/drivers/scsc/`), kernel patches (`devices/samsung/a21s/kernel/patches/`), and the kernel defconfig (`devices/samsung/a21s/kernel/a21s_linux_defconfig`).

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
2. Applies **`devices/samsung/a21s/kernel/patches/0001-a21s-wlbt-wifi-bt-dts.patch`** to `arch/arm64/boot/dts/exynos/exynos850-a217f.dts`:
   - Adds `clk_ignore_unused` to `chosen/bootargs` (prevents `clk_disable_unused()` SoC reset on WLBT `SM_MSG_START_REQ`).
   - Reserves `wifibt_if` at `0xb0000000` (`0xc00000`).
   - Adds `&soc` syscon nodes (`i3c_apm_pmic`, `dbus_baaw`, `pbus_baaw`, `wlbt_remap`, `boot_cfg`) and `scsc_wifibt@119c0000`.
3. Clones `https://github.com/samsungexynos850/uniLoader.git` (`06a1bba`) into `src/uniLoader`.
4. Copies `devices/samsung/a21s/kernel/a21s_linux_defconfig` to `build/linux/.config`, runs `olddefconfig`, and runs `modules_prepare`.

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

*(After testing, refresh `devices/samsung/a21s/kernel/a21s_linux_defconfig` with `make -C src/linux O=../../build/linux ARCH=arm64 savedefconfig && cp build/linux/defconfig devices/samsung/a21s/kernel/a21s_linux_defconfig`.)*

---

## 4. Build Kernel (`Image` + `dtbs`) and SCSC Modules (`socs/exynos850/drivers/scsc/`)

```bash
# 1. Build kernel Image, DTBs, and in-tree modules (e.g. HID_*.ko)
make -C src/linux O=../../build/linux ARCH=arm64 LLVM=1 -j$(nproc) Image dtbs modules

# 2. Build out-of-tree SCSC WLBT modules (scsc_platform_mif, scsc_mx, scsc_wlan, scsc_bt)
make -C src/linux O=../../build/linux ARCH=arm64 LLVM=1 M=$PWD/socs/exynos850/drivers/scsc modules
```

---

## 5. Deploy & Test Safely

### A. Deploy Modules (NEVER overwrite `/lib`!)
On Arch Linux ARM, `/lib` is a symlink to `usr/lib`. Always install modules under `/usr/lib/modules/$(uname -r)/`:

```bash
# Out-of-tree SCSC modules
scp socs/exynos850/drivers/scsc/*.ko root@<wifi-ip>:/usr/lib/modules/7.3.0-g14a425b3a7dc/extra/
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

## 6. `rabbiz-osk` (Rust on-screen keyboard)

Host-only work (logic, layouts, PNG previews) needs no device and no cross toolchain:

```bash
cd tools/rabbiz-osk
cargo test
cargo run -p osk-render --bin osk-preview      # → build/osk/*.png
```

**Device build — no cross toolchain required.** Docker Desktop on Apple Silicon runs
`linux/arm64` containers, so an official Rust image already *is* an aarch64 toolchain
(verified 2026-11-17: `uname -m` = `aarch64`, `rustc --version` = 1.99.1):

```bash
docker run --rm -v "$PWD":/w -w /w/tools/rabbiz-osk rust:1.99-bookworm \
  /usr/local/cargo/bin/cargo build --release -p osk-render --bin osk-preview
```

Use `bash -c` (not `bash -lc`) or the absolute `/usr/local/cargo/bin/cargo` path — a login
shell resets `PATH` and `cargo` disappears. Output lands in
`tools/rabbiz-osk/target/release/` (root-owned: `docker run --rm -v "$PWD":/w busybox chown -R
"$(id -u):$(id -g)" /w/tools/rabbiz-osk/target`). The binary is glibc-linked (Debian 2.36) and
runs on Arch Linux ARM as-is.

Build the keyboard itself (adds `libxkbcommon-dev`; the crate also needs the protocol `client`
feature):

```bash
docker run --rm -v "$PWD":/w -w /w/tools/rabbiz-osk rust:1.99-bookworm bash -c '
  apt-get update -qq && apt-get install -y -qq libxkbcommon-dev pkg-config
  /usr/local/cargo/bin/cargo build --release -p osk-wayland'
```

Deploy and run a *preview* (writes PNGs, never touches the seat — squeekboard keeps running):

```bash
scp tools/rabbiz-osk/target/release/osk-preview root@172.16.42.1:/usr/local/bin/rabbiz-osk-preview
ssh root@172.16.42.1 "flock --close --timeout 300 /run/lock/a21s-device-test.lock \
  bash -c '/usr/local/bin/rabbiz-osk-preview /tmp/osk-png'"
scp root@172.16.42.1:/tmp/osk-png/en-us-base-dark.png build/osk-device/
```

Screenshot the live session (the shell runs as `gamer`, uid 1001):

```bash
ssh root@172.16.42.1 "runuser -u gamer -- env XDG_RUNTIME_DIR=/run/user/1001 \
  WAYLAND_DISPLAY=wayland-0 grim -o DSI-1 /tmp/shot.png"
```

Rules that still apply: hold the device lock for anything that draws or measures, never run
two input methods on the seat (squeekboard is the live one until K7), and never compile on the
device (2.6 GB RAM).

Switch the live keyboard (root on the device):

```bash
rabbiz-osk-switch start     # stops squeekboard, runs /usr/local/bin/rabbiz-osk as gamer
rabbiz-osk-switch status
rabbiz-osk-switch stop      # restores squeekboard (the session default)
```

Only one input method may own the seat; `start` waits for squeekboard to disappear before it
launches ours. To exercise it without the phone in hand, run the GTK4 client and inject taps:

```bash
runuser -u gamer -- env XDG_RUNTIME_DIR=/run/user/1001 WAYLAND_DISPLAY=wayland-0 \
  DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1001/bus GSK_RENDERER=cairo \
  python3 /tmp/osk_test_client.py &          # tools/rabbiz-osk/tests/osk_test_client.py
python3 /root/touch_swipe.py 72 1455        # tap the 'a' key (this file lives in scripts/)
```

`GSK_RENDERER=cairo` matters: with the default renderer the GTK4 client dies with a Wayland
protocol error on this device.

If the panel is asleep, `a21s-power-key wake` turns it on; a touch swipe up then unlocks it
(the shell's lock screen is a swipe gesture, not logind's lock).

