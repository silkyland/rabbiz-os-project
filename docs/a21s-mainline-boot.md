# Galaxy A21s (`SM-A217F`) Mainline Linux Boot Log (`M1-R1-T4`)

- **Date:** 2026-11-14
- **Bootloader chain:** Samsung S-Boot (`OEM LOCK: OFF`, `VBMETA` `flags=2` `rollback_index=12`, stub `DTBO` `custom0=0x00, custom1=0xff`, `rollback_index=12`) -> `samsungexynos850/uniLoader` (`a21s_defconfig`) -> `samsungexynos850/linux` (`staging` branch, `exynos850-a217f.dtb`)
- **Kernel (`uname -a`):** `Linux (none) 7.3.0-g14a425b3a7dc #2 SMP PREEMPT_DYNAMIC Sat Nov 14 14:46:27 +07 2026 aarch64 Linux`
- **Telnet debug shell (`172.16.42.1:23`) verified hardware:**
  - **Storage:** `mmcblk0: mmc0:0001 DX68MB 29.1 GiB` (`p1`–`p37`)
  - **Display DRM/KMS:** `[drm] Initialized exynos 1.1.0 for exynos-drm on minor 0` (`fb0: exynosdrmfb`, `panel-synaptics-tddi 13070000.dsi.0`)
  - **GPU (`Panfrost`):** `[drm] Initialized panfrost 1.7.0 for 11500000.gpu on minor 2` (`mali-g52 id 0x7212`, `377 MHz`, `/dev/dri/card0..2`, `/dev/dri/renderD128..129`)
  - **Touchscreen:** `ovt-td4150 spi0.0: TD4150 ready: 719x1599, 5 objects` (`/dev/input/event0..2`)
  - **USB-C Networking:** `cdc_ncm` (`172.16.42.1` device / `172.16.42.2` host, ~1.70 ms RTT)
- **Next step (`M2 (R1)`):** Deploy Native ARM64 Linux rootfs onto `USERDATA` (`mmcblk0p...`) with ZRAM, Mesa `Panfrost`/`PanVK`, and test `kmscube` / `vulkaninfo` directly on DRM/KMS.

## Black screen ~30 s after boot (fixed 2026-11-15)

- **Symptom:** After `s2mpu12-regulator.ko` was added for audio, the panel went fully black about 30 s after every boot. SSH, Steam and audio kept working.
- **Root cause:** On current boots exynos-drm fails to bind (`decon850_component_ops: -19`), so the screen is driven by `simpledrm` on the framebuffer uniLoader set up. Nothing in the DTS consumes the panel supplies (`vio-supply = <&reg_dummy>`, see TODO in `exynos850-a217f.dts`). Once the S2MPU12 rails were registered, `regulator_late_cleanup()` turned off the unused boot-on rails:
  ```
  [   31.714188] vdd_grip_3p3: disabling
  [   31.714441] vdd_ldo28: disabling        <- ldo28 = lcd_vddi
  ```
- **Fix:** `drivers/abox/s2mpu12-regulator.c` marks every `regulator-boot-on` rail as `always_on` at probe (`"<rail>: boot-on rail kept enabled"` in dmesg). Re-enabling the rail at runtime does not restore the panel because it needs uniLoader's init sequence, so a reboot is required.
- **Proper fix later:** wire `vio-supply = <&vdd_ldo28>` (plus LM36274 `vsn/vsp` + backlight) in the DTS, then drop the blanket `always_on`.
