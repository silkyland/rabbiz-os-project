# Samsung Galaxy A21s (SM-A217F) — Mainline Linux Wi-Fi & Bluetooth Bringup

This document records how **Wi-Fi (`wlan0`)** and **Bluetooth (`hci0`)** were ported and brought up on the Samsung Galaxy A21s (`SM-A217F`, Exynos 850 / `S5E3830`, `S612` WLBT RF) running Mainline Linux `7.3.0-g14a425b3a7dc` + Arch Linux ARM.

---

## 1. Architecture Overview

The Exynos 850 integrates Samsung's **Maxwell / SCSC WLBT** subsystem (`119c0000.scsc_wifibt`), which runs shared firmware (`wifi/mx140.bin`) on an embedded Cortex-R4/M4 core and communicates with the AP via shared DRAM (`wifibt_if`, 12 MiB), mailboxes (`MBOX`), and PMU/APM syscons.

Four out-of-tree kernel modules live in `drivers/scsc/` and are built against `build/linux`:
1. `scsc_platform_mif.ko` — Exynos 850 (`S5E3830`) platform MIF, PMU reset/release, BAAW, IRQs, and shared memory mapping.
2. `scsc_mx.ko` — Maxwell core manager (`mxman`), firmware loader (`mx140_file`), `mxlog`, and service management (`SM_MSG_*`).
3. `scsc_wlan.ko` — Wi-Fi driver (`wlan0`, `cfg80211`, `HIP4`, `NACHO_S612_A21` MIB).
4. `scsc_bt.ko` — Bluetooth driver (`hci0` via `scsc_bluez` + `rfkill`).

---

## 2. Device Tree & Boot Image Requirements

### 2.1 `src/linux/arch/arm64/boot/dts/exynos/exynos850-a217f.dts`
- **`chosen/bootargs` (`clk_ignore_unused`):**
  Mainline `clk_disable_unused()` disables ungoverned CMU clock gates at `late_initcall`. Without `clk_ignore_unused`, sending `SM_MSG_START_REQ` for WLAN causes an immediate hardware SoC reset.
  ```dts
  bootargs = "... clk_ignore_unused rw";
  ```
- **Reserved Shared Memory (`reserved-memory`):**
  ```dts
  wifibt_if: wifibt_if@b0000000 {
      compatible = "exynos,wifibt_if";
      reg = <0x0 0xb0000000 0xc00000>;
      no-map;
  };
  ```
- **WLBT Syscons & Platform Node (`&soc`):**
  - `i3c_apm_pmic: syscon@11a50000`
  - `dbus_baaw: syscon@14c20000`
  - `pbus_baaw: syscon@14c30000`
  - `wlbt_remap: syscon@14c50000`
  - `boot_cfg: syscon@14c60000`
  - `scsc_wifibt@119c0000` with `MBOX` (SPI 54), `ALIVE` (SPI 38), `CFG_REQ` (SPI 230), `WDOG` (SPI 231), `samsung,syscon-phandle = <&pmu_system_controller>`, and `memory-region = <&wifibt_if>`.

### 2.2 Building & Flashing `boot-avb.img`
Samsung S-Boot requires an uncompressed `uniLoader` binary (`src/uniLoader/uniLoader`, **not** `uniLoader.gz`) and `--dtb` with `--dtb_offset 0x00000000` (otherwise S-Boot fails with `load failed FDT_ERR_BADMAGIC`), while Linux uses the DTB embedded in `src/uniLoader/blob/dtb`:

```bash
make -C build/linux ARCH=arm64 LLVM=1 exynos/exynos850-a217f.dtb
cp build/linux/arch/arm64/boot/dts/exynos/exynos850-a217f.dtb src/uniLoader/blob/dtb
make -C src/uniLoader ARCH=aarch64 LLVM=1 -j$(nproc)

mkbootimg \
  --kernel src/uniLoader/uniLoader \
  --ramdisk src/uniLoader/blob/ramdisk \
  --dtb src/tools/AIK-Linux/split_img/boot.img-dtb \
  --dtb_offset 0x00000000 \
  --header_version 2 \
  --pagesize 2048 \
  --base 0x10000000 \
  --kernel_offset 0x00008000 \
  --ramdisk_offset 0x01000000 \
  --tags_offset 0x00000100 \
  --cmdline "$(cat src/tools/AIK-Linux/split_img/boot.img-cmdline)" \
  --os_version "$(cat src/tools/AIK-Linux/split_img/boot.img-os_version)" \
  --os_patch_level "$(cat src/tools/AIK-Linux/split_img/boot.img-os_patch_level)" \
  -o build/images/boot-linux.img

cp build/images/boot-linux.img build/images/boot-avb.img
python3 ./build/avbtool.py add_hash_footer \
  --image build/images/boot-avb.img \
  --partition_size 37748736 \
  --partition_name boot \
  --rollback_index 12
```

Flash either from Download Mode (`heimdall flash --BOOT build/images/boot-avb.img`) or directly from Linux (`/dev/mmcblk0p18`).

---

## 3. Kernel Driver Porting (`drivers/scsc/`)

### 3.1 `scsc_platform_mif.ko` (`drivers/scsc/misc/platform_mif_3830.c`)
- Removed `scsc_platform_mif -> scsc_mx` circular `depmod` dependency in `platform_wdog_isr()` (`mxman_recovery_disabled()`).
- Added `of_reserved_mem_lookup()` fallback for `memory-region`.
- Switched `platform_get_resource(pdev, IORESOURCE_IRQ, i)` to `platform_get_irq_byname()`.
- Replaced `vmap(phys_to_page(...))` / `vunmap()` in `platform_mif_map_region()` / `platform_mif_unmap_region()` with `ioremap_wc()` / `iounmap()`.

### 3.2 `scsc_mx.ko` (`drivers/scsc/misc/` & `include/scsc/`)
- **`mx140_file.c`**: Changed `base_dir_request_fw` from `"../etc/wifi"` to `"wifi"` (mainline `request_firmware()` rejects `..` path components), replaced `vfs_stat` with `filp_open` + `i_size_read()`, and switched `memcpy()` to `memcpy_toio()` for `ioremap_wc` memory.
- **`mxman.c`**:
  - Fixed `mxman_wifi_kobject_ref_get()` / `mxman_wifi_kobject_ref_put()` refcounting & NULL checks.
  - Guarded `mif->recovery_disabled_reg` (in `transports_init()`) and `mif->recovery_disabled_unreg` (in `mxman_stop()`) against `NULL` to prevent `pc : 0x0` kernel oopses.
- **`scsc_logring.h`**: Replaced `dev_printk_emit((lvl), NULL, fmt, ...)` with `printk(fmt, ...)` to fix `mxlog_message_handler` NULL `dev` oops.

### 3.3 `scsc_wlan.ko` (`drivers/scsc/Kbuild`, `wlan/cm_if.c`, `wlan/reg_info.c`)
- Added `wlan/cm_if.o` to `scsc_wlan-y` in `drivers/scsc/Kbuild`.
- Disabled `MODULE_IMPORT_NS(VFS_internal_I_am_really_a_filesystem_and_am_NOT_a_driver)` in `wlan/cm_if.c`.
- Updated `reg_file_t` in `wlan/reg_info.c` to `"wifi/slsi_reg_database.bin"`.

### 3.4 `scsc_bt.ko` (`drivers/scsc/bt/`)
Imported from `src/exynos850-vendor-kernel/drivers/misc/samsung/scsc_bt/` and enabled with `-DCONFIG_SCSC_BT=1 -DCONFIG_SCSC_BT_BLUEZ=1`:
- **`scsc_bluez.c`**:
  - Replaced `linux/wakelock.h` and `../../../bluetooth/h4_recv.h` with `<net/bluetooth/hci_h4.h>` and `h4_recv_skb(hdev, NULL, NULL, ...)`.
  - Removed `hdev->dev_type = HCI_BREDR` (removed in mainline `struct hci_dev`).
  - Fixed `slsi_bt_send_frame()` to update `hdev->stat` before `kfree_skb(skb)` and return `0` on success (`bt_fs->write()` returns byte count `> 0`).
  - Combined `slsi_bluez_init()` / `slsi_bluez_exit()` into `scsc_bt_module_init()` / `scsc_bt_module_exit()` and initialized `rfkill_init_sw_state(btz_rfkill, 0)` (unblocked).
- **`scsc_shm.c`**: Added `scsc_bt_copy_to_buf()` and `scsc_bt_copy_from_buf()` helpers so `scsc_bluez` kernel-space buffers (`receive_buffer`, `skb->data`) do not fail `copy_to_user()` / `copy_from_user()` with `-EFAULT` / `-EACCES`.
- **`scsc_bt_module.c`**: Updated `proc_create_data()` to `struct proc_ops`, `class_create("scsc_char")` (single-arg), and `const struct firmware *`.

---

## 4. Building & Deploying Modules

```bash
make -C build/linux ARCH=arm64 LLVM=1 M=$PWD/drivers/scsc -j$(nproc) modules
zstd -f -q drivers/scsc/scsc_platform_mif.ko drivers/scsc/scsc_mx.ko drivers/scsc/scsc_wlan.ko drivers/scsc/scsc_bt.ko
scp drivers/scsc/*.ko.zst root@172.16.42.1:/lib/modules/7.3.0-g14a425b3a7dc/updates/
ssh root@172.16.42.1 "depmod -a && sync"
```

> **Important:** On Arch Linux ARM, `/lib` is a symlink to `usr/lib`. Never extract tarballs with `lib/` into `/` (`tar -xf - -C /`); copy `.ko.zst` files directly into `/lib/modules/$(uname -r)/updates/`.

Auto-load configuration on the A21s (`/etc/modules-load.d/scsc-wifi.conf`):
```ini
scsc_platform_mif
scsc_mx
scsc_wlan
scsc_bt
```

---

## 5. Userspace Wi-Fi Configuration & Verification

- Installed `iw`, `wpa_supplicant`, and `pcsclite` (`aarch64`).
- Configured `/etc/wpa_supplicant/wpa_supplicant-wlan0.conf` (`country=TH`, target SSID/PSK) and `/etc/systemd/network/25-wlan0.network` (`DHCP=yes`), then enabled `wpa_supplicant@wlan0.service`.

### Verified Results
- **Modules:** `scsc_platform_mif`, `scsc_mx`, `scsc_wlan`, `scsc_bt` all auto-load cleanly on boot.
- **Wi-Fi (`wlan0`):** Connected to `Goodhome_5G` (5 GHz, `freq: 5745.0`, `tx bitrate: 325.0 MBit/s`), `ping 1.1.1.1` succeeded with 0% packet loss (~12.7 ms).
- **Bluetooth (`hci0`):** `/sys/class/bluetooth/hci0` registered, `rfkill list` shows `scsc-bluez-rfkill`, `hci0`, and `phy0` all unblocked (`Soft blocked: no`, `Hard blocked: no`), and `SM_MSG_START_CFM` / `SM_MSG_STOP_CFM` for `service: 2` complete cleanly without oopses.

---

## 6. Bluetooth Userspace (BlueZ) for `steam -gamepadui`

Steam's gamepad UI pairs devices by talking to `org.bluez` on the **system D-Bus**, so `bluetoothd` must be running — the kernel `hci0` alone is not enough.

- Installed `bluez`, `bluez-utils` (`bluetoothctl`), `bluez-deprecated-tools` (`hciconfig`, `btmgmt`) — enabled `bluetooth.service`.
- Adapter `a21s-steamos` (`F0:00:01:00:A2:17`): `Powered/Discoverable/Pairable: yes`, `hci0` `UP RUNNING PSCAN ISCAN`, LE scan finds game controllers (e.g. Xbox Wireless Controller, HID-over-GATT `0x1812`).
- BT audio: installed `pipewire-audio` (ships `/usr/lib/spa-0.2/bluez5/` — SBC/AAC/aptX/LDAC/HFP codecs), `pipewire-pulse`, `wireplumber`; started `wireplumber.service` + `pipewire-pulse.socket` in the `gamer` user manager (`systemctl --user -M gamer@`).
- **usb0 default route:** `10-usb0.network` now uses `[Route] Gateway=172.16.42.2 Metric=700` instead of `Gateway=` (metric 0). The old config made the dead USB host route shadow `wlan0`'s DHCP route (metric 600), killing all internet whenever the host wasn't NATing; now `wlan0` wins and USB NAT is a fallback.

---

## 7. STOPGAP: `xbox-ble-uinput` bridge — remove after kernel rebuild

The current kernel `7.3.0-g14a425b3a7dc` lacks `CONFIG_UHID`/`CONFIG_HIDRAW`, so BlueZ `input-hog` cannot create evdev nodes for BLE HID devices (`src/service.c:service_accept() input-hog profile accept failed` in `bluetoothd` logs). As a stopgap, `xbox-ble-uinput.service` runs `/usr/local/bin/xbox-ble-uinput.py`: it reads the HID-over-GATT input report of the paired Xbox controller (`A8:8C:3E:74:0A:95`, Report ID 1 on `char001d` under HID service `service0016`) via BlueZ D-Bus and re-emits through `/dev/uinput` (`CONFIG_INPUT_UINPUT=y`) as a `BUS_BLUETOOTH` `045E:0B13` "Xbox Wireless Controller" evdev device. Works, but: single hardcoded MAC, Xbox report layout only (no DS4/DS5/Switch), no rumble (output Report ID 3 on `char0021` unused), no battery node.

### Kernel fix (build machine): enable then rebuild `Image` + modules
```
CONFIG_UHID=y              # BlueZ input-hog creates evdev for every BLE HID pad
CONFIG_HIDRAW=y            # hidraw for Steam/xpadneo per-controller drivers
CONFIG_INPUT_JOYDEV=y      # /dev/input/js*
CONFIG_BT_HIDP=m           # classic HID (DS3/Wii Remote), already =m
CONFIG_HID_MICROSOFT=m     # Xbox: FF quirks
CONFIG_HID_PLAYSTATION=m   # DS4/DS5: rumble, LED, battery, touchpad
CONFIG_HID_NINTENDO=m      # Switch Joy-Con/Pro
CONFIG_HID_STEAM=m         # Steam Deck/controller accessories
CONFIG_XPAD=m              # wired Xbox over USB-OTG (optional)
```
- Kernel build env is on a **different machine** (not this Mac — `src/` + `build/` are empty here). `kexec` is available on-device (`CONFIG_KEXEC=y`) for testing a new `Image` before flashing `boot-avb.img`.
- **Cleanup once a `CONFIG_UHID` kernel is running:** `systemctl disable --now xbox-ble-uinput.service && rm /etc/systemd/system/xbox-ble-uinput.service /usr/local/bin/xbox-ble-uinput.py && systemctl daemon-reload`. The existing bond survives; `input-hog` will then create `/dev/input/event*` natively.
- Verified data path (kept for reference): `bluetoothctl pair/trust/connect` bonds fine; report notifications arrive on `/org/bluez/hci0/dev_A8_8C_3E_74_0A_95/service0016/char001d` — 16-byte payload, no ID prefix: `u16 LX,LY,RX,RY | u10 LT | u10 RT | lo-nibble hat | 15-bit buttons | consumer bit0 (Share)`.
