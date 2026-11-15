# a21s-cpif — Shannon `s318ap` CPIF skeleton

Skeleton port of vendor `drivers/soc/samsung/cpif` (Linux 4.19.325) to
Mainline 7.3 for the Galaxy A21s (`samsung,exynos-cp`, SIPC v5.0).

Status: **skeleton — compiles, not validated on hardware.** Implements
the firmware-upload path; SIPC data plane is stubbed.

## What works (in principle)

- CP power sequencing translated from vendor `pmucal_cp_exynos3830.h`
  into direct regmap ops on the PMU syscon (`pmu_system_controller`
  @ `0x11860000`), plus the VGPIO_TX_MONITOR wait at `0x11a50000`.
- `cp_shmem/regions` DT parsing: maps the named sub-regions inside
  `cp_rmem` (`0xd0000000`/`0x8800000`).
- TOC dump: parses `struct cp_toc` + `struct cp_mem_map` from the CP
  image after `LOAD_CP_IMAGE` so the non-secure shmem layout can be
  verified against the DT table.
- `/dev/umts_boot0` ioctl surface identical to vendor
  (`cpif_uapi.h` mirrors `modem_prj.h` numbers): `POWER_ON`,
  `LOAD_CP_IMAGE`, `POWER_RESET`, `START_CP_BOOTLOADER`,
  `GET_CP_STATUS`.
- `/dev/umts_ipc0`, `/dev/umts_ipc1`, `/dev/umts_rfs0` nodes exist;
  reads/writes return `-EIO` until the link layer lands.

## What is NOT implemented yet

- `IOCTL_REQ_SECURITY` — needs the SMC ids used by vendor
  `shmem_security_request()` (`SMC_ID`, `SMC_ID_CLK`,
  `CP_CHECK_SIGN_NOT_FINISH` under `CONFIG_CP_SECURE_BOOT`). If the
  A21s ships with secure-boot CP images, this must be solved or the
  modem will refuse to boot.
- `START_CP_BOOTLOADER` mailbox handshake — vendor pokes
  `mbox_set_interrupt()`/`mbox_sw_reset()` and waits for the CP boot
  loader's "done" indication. Needs the `cp_mailbox` driver
  (`0x11920000`, IRQ `GIC_SPI 51`) and `mcu_ipc`.
- SIPC link layer — TX/RX ring buffers in the `IPC` shmem region,
  `link_device_memory_sbd.c` descriptor format, flow control,
  `/dev/rmnet*` netdevs.
- `IOCTL_COMPLETE_NORMAL_BOOTUP` — waits for boot completion.
- Dump/crash paths (`IOCTL_START_CP_DUMP`, `TRIGGER_CP_CRASH`,
  `GET_CP_BOOTLOG`, `GET_LOG_DUMP`).
- NV/EFS handling — `RADIO`/`CP_DEBUG`/`EFS` partition contents must be
  reachable by userspace; vendor `cbd` also feeds `nv_data` items.

## Userspace

`tools/cbd/a21s-cbd` is the companion loader: opens `/dev/umts_boot0`,
streams `modem.bin` (or reads it straight from the `RADIO` GPT
partition), then runs the security + boot sequence.

## References

- Vendor source: `src/exynos850-vendor-kernel/drivers/soc/samsung/cpif/`
  (sparse checkout — `git show HEAD:<path>` to read blobs)
- Power sequences: `drivers/soc/samsung/cal-if/exynos3830/pmucal_cp_exynos3830.h`
- Boot protocol: `bootdump_io_device.c`, `link_device.c`
  (`link_load_cp_image`, `shmem_security_request`)
- TOC/shmem: `shm_ipc.c`
- Findings: `docs/a21s-modem-power-findings.md`
