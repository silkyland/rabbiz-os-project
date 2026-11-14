# Galaxy A21s (`SM-A217F` / `Exynos 850` / `S5E3830`) Wi-Fi (`WLBT` / `SCSC`) Reverse-Engineering & Mainline Porting Guide

## 1. Hardware Architecture Discovery

Unlike Qualcomm/Broadcom devices that attach Wi-Fi over SDIO or PCIe, **Exynos 850 (`S5E3830`)** integrates Samsung's **WLBT (Maxwell / `mx140`)** Wi-Fi + Bluetooth co-processor directly on the SoC, driven by **SCSC (Samsung Connectivity Solutions Core)**.

Reverse-engineered from `samsungexynos850/android_kernel_samsung_exynos850` (`lineage-24.0`):
- Core MIF / IPC driver: `drivers/misc/samsung/scsc/` (`124` files; SoC-specific `platform_mif_3830.c`, `mif_reg_S5E3830.h`, `mx140_file.c`)
- Wireless `cfg80211` driver (`wlan0`): `drivers/net/wireless/scsc/` (`107` files)

## 2. Device Tree & Memory/IRQ Map (`exynos3830.dts`)

### Reserved Shared Memory
```dts
reserved-memory {
    wifibt_if {
        compatible = "exynos,wifibt_if";
        size = <0xc00000>; /* 12 MiB shared DRAM ring/mailbox buffer */
    };
};
```
Bound via `RESERVEDMEM_OF_DECLARE(wifibt_if, "exynos,wifibt_if", platform_mif_wifibt_if_reserved_mem_setup)` in `platform_mif_3830.c`.

### Syscon Register Blocks
| Role | DT Node / Compatible | Physical Base | Size |
| --- | --- | --- | --- |
| PMU | `pmu_system_controller` (`samsung,syscon-phandle`) | Already in mainline `exynos850.dtsi` | — |
| I3C APM PMIC | `syscon@11A50000` (`i3c_apm_pmic`) | `0x11A50000` | `0x1800` |
| DBUS BAAW | `syscon@14C20000` (`dbus_baaw`) | `0x14C20000` | `0x300` |
| PBUS BAAW | `syscon@14C30000` (`pbus_baaw`) | `0x14C30000` | `0x300` |
| WLBT Remap | `syscon@14C50000` (`wlbt_remap`) | `0x14C50000` | `0x500` |
| Boot Config | `syscon@14C60000` (`boot_cfg`) | `0x14C60000` | `0x1200` |

### Main WLBT Mailbox / Control Device (`scsc_wifibt@119C0000`)
- `compatible = "samsung,scsc_wifibt"`
- `reg = <0x00 0x119c0000 0x180>`
- `interrupts`:
  - `MBOX`: GIC SPI `0x36` (`54`), `IRQ_TYPE_LEVEL_HIGH` (`4`)
  - `ALIVE`: GIC SPI `0x26` (`38`), `IRQ_TYPE_LEVEL_HIGH` (`4`)
  - `CFG_REQ`: GIC SPI `0xe6` (`230`), `IRQ_TYPE_LEVEL_HIGH` (`4`)
  - `WDOG`: GIC SPI `0xe7` (`231`), `IRQ_TYPE_LEVEL_HIGH` (`4`)

## 3. Firmware & Runtime Dependencies

1. **Firmware (`mx140.bin`):**
   - Loaded by `mx140_file.c` (`MX140_FW_BIN = "mx140.bin"`, default path `/vendor/firmware/` or `/etc/wifi/mx140.bin`).
   - On the A21s eMMC, the stock Android `super` partition (`/dev/disk/by-partlabel/super -> /dev/mmcblk0p30`) is still intact and contains the `vendor` logical partition with `mx140.bin` and calibration/country files.
2. **TZPC Security Call (`SMC`):**
   - `platform_mif_3830.c:740`: `exynos_smc(SMC_CMD_CONN_IF, (EXYNOS_WLBT << 32) | EXYNOS_SET_CONN_TZPC, 0, 0);`
   - Can be replaced directly with `arm_smccc_smc()` in Mainline `7.3.0` (`#include <linux/arm-smccc.h>`).
3. **PMIC / LDO Power:**
   - `disable_apm_setup = true` by default in `platform_mif_3830.c` (`/* The APM IPC in FW will be used instead */`), meaning the `s2mpu12-regulator.h` I2C hack is **not** required when APM firmware is active.
4. **Optional Vendor Hooks to Strip:**
   - `CONFIG_SCSC_QOS` (`pm_qos_*` / `PM_QOS_CLUSTER0_FREQ_MIN`): disable `CONFIG_SCSC_QOS` for initial bring-up.
   - `CONFIG_SCSC_LOG_COLLECTION` / `debug-snapshot`: disable for initial bring-up.

## 4. Forward-Porting Plan to Mainline `7.3.0`

1. Extract `mx140.bin` + `/vendor/etc/wifi/*` from `/dev/mmcblk0p30` (`super` -> `vendor`) into `/lib/firmware/` on A21s Arch Linux ARM64 rootfs.
2. Copy `drivers/misc/samsung/scsc` and `drivers/net/wireless/scsc` as out-of-tree modules (or kernel patchset) against `samsungexynos850/linux` `7.3.0`.
3. Update `platform_mif_3830.c`:
   - Replace `exynos_smc(...)` with `arm_smccc_smc(...)`.
   - Guard/remove `<linux/mfd/samsung/s2mpu12-regulator.h>` (`disable_apm_setup = true`).
   - Port deprecated 4.19 APIs (`ioremap_nocache` -> `ioremap`, `class_create(THIS_MODULE, ...)` -> `class_create(...)`, `netif_rx_ni` -> `netif_rx`, `cfg80211` `6.x/7.x` `link_id` APIs in `drivers/net/wireless/scsc/`).
4. Add `wifibt_if`, the 5 `syscon` nodes (`0x11A50000`, `0x14C20000`, `0x14C30000`, `0x14C50000`, `0x14C60000`), and `scsc_wifibt@119C0000` into `arch/arm64/boot/dts/exynos/exynos850-a217f.dts` (and `exynos850.dtsi`).
5. Build `scsc_mx.ko` + `scsc_wlan.ko`, load on A21s, verify `dmesg` `ALIVE` / `CFG_REQ` IRQs and `wlan0` registration.
