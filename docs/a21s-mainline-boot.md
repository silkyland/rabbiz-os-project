# Galaxy A21s (`SM-A217F`) Mainline Linux Boot Log (`M1-R1-T4`)

- **Date:** 2026-11-14
- **Bootloader chain:** Samsung S-Boot (`OEM LOCK: OFF`, `VBMETA` `flags=2` `rollback_index=12`, stub `DTBO` `custom0=0x00, custom1=0xff`, `rollback_index=12`) -> `samsungexynos850/uniLoader` (`a21s_defconfig`) -> `samsungexynos850/linux` (`staging` branch, `exynos850-a217f.dtb`)
- **Kernel USB Gadget Enumeration (Host `dmesg`):**
  - `idVendor=18d1, idProduct=d001, bcdDevice= 7.03`
  - `Product: Samsung Galaxy A21s`, `Manufacturer: Samsung`, `SerialNumber: postmarketOS`
  - `cdc_ncm 1-1:1.0 enp119s0u1`: Host IP `172.16.42.2/24`, Device IP `172.16.42.1`
- **Connectivity:** `ping 172.16.42.1` -> 2/2 packets received, 0% packet loss, ~1.70 ms RTT.
- **Next step (`M2 (R1)`):** Unpack `pmos-ramdisk` to enable `telnetd`/SSH debug shell and build/flash Native ARM64 Linux rootfs to `USERDATA` for DRM/KMS (`decon`/`dsi`), `Panfrost`/`PanVK`, and `Gamescope`.
