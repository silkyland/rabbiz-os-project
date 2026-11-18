# Handoff — Cellular modem (`a21s-cpif`, Shannon s318ap)

Date: 2026-11-16. Goal set by the owner: boot the modem, then SIPC link layer, then read SIM / network state.
Current state: **CP boots to ONLINE and holds an established IPC session.** No SIM installed — SIM/network validation is still impossible.
Background and vendor findings: `devices/samsung/a21s/docs/a21s-modem-power-findings.md` §1, `socs/exynos850/drivers/cpif/README.md`, `devices/samsung/a21s/docs/a21s-power-battery-modem-plan.md` Phase 4.

> ## RESOLUTION (update, same day)
>
> The hard hang was diagnosed and fixed; the full boot chain now works:
>
> 1. `no-map` on `cp_rmem` blocked AP access to CP RAM `0xd0000000-0xd68fffff`. Removed it in the DTS and switched mapping to the vendor `phys_to_virt`-style path. Firmware upload + secure auth + CP start all verified.
> 2. CP reached `BOOTING` (`cp_status=1`, AP `0x4022`) but stalled: the BOOT firmware waits for two UDL exchanges on raw ring ch 215 — `0x900d->0xa00d`, then `0x9f00->0xaf00` — before jumping to MAIN. `cpif_boot.c` gained raw boot-queue read/write; `tools/cbd/a21s-cbd.c` gained `boot_exchange()`. Both verified on-device.
> 3. MAIN then sent `INIT_START (0xc1)` every ~3.5 s. `cpif_sbd.c` (new) publishes a real SBD v1 map: global desc at `ipc+0x800`, rp/wp arrays, 10 channel pairs (from the A21s DT: cass 0x23, ipc0 0xeb, ipc1 0xec, rfs0 0xf5, csd 0x01, router 0x19, dm0 0x1c, loopback 0xf4, QoS hiprio 0x0a, normal 0x0b; regional SKT/SPR channels excluded), buffer pos/len arrays, buffers end `0x7a8000` of `0x800000`. Mailbox ISR is now threaded; on `INIT_START` it builds SBD and answers `PIF_INIT_DONE`; CP replied `PHONE_START (0xc8)` -> `magic=0xaa`, `access=1`, state ONLINE.
> 4. CP repeats `PHONE_START` every ~100 ms until `INIT_END`. Vendor gates `INIT_END` on `rild_ready()` = FMT + RFS nodes opened. `cpif_ipc.c` now tracks `fmt_opened`/`rfs_opened`; `cpif_mbox.c` sends `INIT_END` on `PHONE_START` when both are open. Verified: open `umts_ipc0`+`umts_rfs0` -> `INIT_END -> CP` -> CP stops spamming.
> 5. Module reload while CP runs is handled: probe adopts the session when `magic==0xaa && access==1` (`sbd_ready`/`phone_started` restored, `init_cmpl` completed). `phone_state` must then be re-set via `COMPLETE_NORMAL_BOOTUP` (returns immediately).
>
> Boot path per vendor order now: `POWER_ON` -> load image -> `REQ_SECURITY` -> `START_CP_BOOTLOADER` -> UDL `0x900d/0x9f00` -> `INIT_START` -> SBD + `PIF_INIT_DONE` -> `PHONE_START` -> `INIT_END` (once RIL opens FMT+RFS).
> `a21s-modem-ctl init` now runs `a21s-cbd --boot` when state != ONLINE.
>
> Still missing for calls/data: real SIPC frame I/O on `umts_ipc*`/`umts_rfs0` (read/write return -EIO; rings exist but nothing dequeues), SBD data-plane TX/RX + doorbells, RIL/libsamsung-ipc bring-up, cp ramdump/BTL decode.


## 0. Do this first

1. The phone was unreachable (no ping, not on USB) after the last test. The owner has to long-press Power (about 10-15 s; the docs mention Power+VolDown) to reboot it. Do not assume it is back; ping `172.16.42.1`.
2. After it boots, read the hang's last kernel messages before anything else:
   `ls /sys/fs/pstore/; cat /sys/fs/pstore/console-ramoops-0` (`CONFIG_PSTORE_CONSOLE/RAM=y`, `ramoops@f0010000` in the DTS). Not verified: a hard bus hang may not leave anything, and a long-press may clear DRAM.
3. Booting normally is safe: `a21s-cpif` loads at boot but only touches CP on a `POWER_ON` ioctl. `a21s-modem-init` runs `a21s-cbd --load-only`, which never calls `POWER_ON`. Do not run `a21s-cbd --boot` casually.

## 1. What was done this session (all uncommitted in the working tree)

Files in `socs/exynos850/drivers/cpif/`:

| File | Change |
|---|---|
| `cpif_mbox.c` (new) | cp_mailbox at `0x11920000`: ISSR0-3 = ap2cp_msg / cp2ap_msg / ap2cp_united / cp2ap_united, doorbell through `INTGR1`, ISR on SPI 51 with `INTSR0[31:16]`, `sw_reset`, event log. The child node's `reg` is read raw because the `cpif` DT node has no `ranges` (`of_iomap` failed). |
| `cpif_boot.c` | `REQ_SECURITY` = SMC `0x82000700` with mode `\| cp base` (the `use_mem_map_on_cp` case), polling `0x8` while the result is `0x100`, clock SMC `0x82001011` around it. `START_CP_BOOTLOADER` = vendor `start_normal_boot` (init ISSR, ds_detect/hw_rev, `AP2CP_CFG=1`, poll `cp_status`, set `ap_status`/`pda_active`). `COMPLETE_NORMAL_BOOTUP` waits 15 s for CP `PHONE_START`. `START_CP_BOOTLOADER` no longer aliases `POWER_RESET`. |
| `cpif_power.c` | `CP_CTRL_S` (PMU `0x3014`) read-modify-write goes through SMC (`READ_CTRL=3`, `WRITE_CTRL=4`, low/high 16 bits, as vendor `pmucal_smc_read/write`). `power_on` leaves state OFFLINE (it used to set ONLINE). `power_reset` follows vendor (no-op if OFFLINE, wakeup doorbell, `mbox_sw_reset`). |
| `cpif_uapi.h` / `cpif_core.c` | `enum cpif_modem_state` now equals vendor `enum modem_state` (OFFLINE, CRASH_RESET, CRASH_EXIT, BOOTING, ONLINE, NV_REBUILDING, LOADER_DONE, SIM_ATTACH, SIM_DETACH, CRASH_WATCHDOG). The old order was wrong for any vendor userspace. |
| `cpif.h`, `Kbuild` | mailbox fields, `init_cmpl`, `cpif_mbox_*` prototypes, `cpif_mbox.o`. |

Also touched in the working tree but unrelated to the modem: `devices/samsung/a21s/kernel/a21s_linux_defconfig`, `devices/samsung/a21s/kernel/patches/0005-...` (battery on `&i2c_0`), `shell/A21s/Desktop/HomeScreen.qml` (unlock fly-in), `rootfs-overlay/usr/local/bin/volume-keys.py`, `scripts/audio-up.sh`, `devices/samsung/a21s/docs/repro/`, `scripts/diagnose_audio_charging.sh`. Do not stage them with the modem work.

Deployed on the phone: new `a21s-cpif.ko` at `/usr/lib/modules/7.3.0-g14a425b3a7dc-dirty/extra/a21s-cpif.ko` (old one at `/root/a21s-cpif.ko.bak`). A test copy of the loader is `/tmp/a21s-cbd-new`, built on the device (`gcc` there; no aarch64 cross compiler on the host) and gone after reboot. `/usr/local/bin/a21s-cbd` is still the old build.

## 2. Evidence (what is known)

- Verified: `modprobe` + probe works. cp_rmem `0xd0000000/0x8800000`, regions CP/VSS/IPC/BTL map, the CP image TOC (TOC, BOOT, MAIN, VSS, NV) parses, `/dev/umts_boot0`, `umts_ipc0/1`, `umts_rfs0` exist, `radio` partition is `mmcblk0p22`.
- Verified (earlier boot): reading PMU `0x3014` with plain MMIO gives a synchronous external abort (`regmap_mmio_read32le`, from `cpif_power_on`). The oops killed only the calling task. So `CP_CTRL_S` is secure-only; other PMU regs (`CP_STATUS 0x3004`) read fine.
- Verified: after switching `CP_CTRL_S` to SMC, running `cbd --boot` made the whole phone hang. dmesg was not preserved. **Which step hung is unknown.**
- Hypotheses for the hang, none tested (candidates, ranked by my guess):
  1. A step in `cp_init_seq` (`socs/exynos850/drivers/cpif/cpif_power.c:90`) blocks the bus, for example the wait on `VGPIO_TX_MONITOR` at `0x11a50000+0x1700` (`vgpio` mapping), or `CP_IN`/`CP_STATUS` waits after `CP_CONFIGURATION`.
  2. The SMC `WRITE_CTRL` value or register-id convention is wrong (`cpif_ctrl_s_update`, `cpif_power.c:57`) and the secure monitor stalls.
  3. The SMC wrapper: `arm_smccc_smc` with these ids might need other registers/clobber handling than vendor `exynos_smc`.
  4. CP started without an authenticated image, since `REQ_SECURITY` runs after `POWER_ON` in the cbd flow.
- Not known: whether a stock Android boot leaves CP already powered and the vendor `cal_cp_status()==1` path should apply instead of `cp_init`. Check `cpif_cp_status()` before any write, and log it.

## 3. Recommended next steps

1. After reboot, collect pstore (above). If empty, make failures visible before they happen: add `dev_info` before each `cpif_seq_run` step and write the step index to something that survives a hang (ramoops via `pstore`, or a spare ISSR register), then re-run. Add a `step` module parameter or a sysfs file so `POWER_ON` can be executed one `cp_init_seq` row at a time, with a long pause so you can power-cycle between tries.
2. Resolve the hang. Read-only checks first: current `CP_STATUS`, `CP_CONFIGURATION`, `TOP_OUT`, `MIF_CTRL`, `VGPIO_TX_MONITOR`, using plain MMIO except `CP_CTRL_S`. Compare with `src/exynos850-vendor-kernel/drivers/soc/samsung/cal-if/exynos3830/pmucal_cp_exynos3830.h` (sparse checkout already extended to `drivers/soc/samsung`, `include/soc/samsung`, `arch/arm64/configs`).
3. Then `REQ_SECURITY` -> `START_CP_BOOTLOADER` -> check `cp_status` in ISSR3 -> `COMPLETE_NORMAL_BOOTUP`. The CP's `INIT_START`/`PHONE_START` handling is only logged; the vendor handler is `cmd_init_start_handler` / `cmd_phone_start_handler` in `src/exynos850-vendor-kernel/drivers/soc/samsung/cpif/link_device.c` (~lines 529-690). `PHONE_START` should answer with `INIT_END` after the link is initialised (not implemented).
4. SIPC link layer (not started): legacy fmt/raw queues from the vendor DT (`a21s_eur_open_w00_r00.dts` lines ~984-999, `legacy_fmt_*`, `legacy_raw_*`, `link_attrs 0x7c9`, magic values `MEM_IPC_MAGIC 0xAA`), `init_legacy_link` in `link_device_memory_legacy.c:131`, SIPC5 framing, `umts_ipc0` read/write in `socs/exynos850/drivers/cpif/cpif_ipc.c` (currently returns `-EIO`). Data channels (`rmnetN`) use SBD and are a separate, larger job.
5. Userspace to read SIM/network over the fmt channel: unresolved. `libsamsung-ipc` is claimed in the docs to fit SIPC 5.0, but I did not verify that it supports Shannon s318ap (upstream only targets older modems). Treat it as an assumption and check first, or look at `rebehzat/samsung-s310-cbd-replacement` (cited in `devices/samsung/a21s/docs/a21s-modem-power-findings.md` §5.4).

## 4. Rules and gotchas for whoever continues

- Serialize all device work with `scripts/device_session.sh` (`docs/DEVICE-QUEUE.md`). A hang drops the SSH session while holding the lock; the lock is released when the session ends.
- Never overwrite `/lib` on the rootfs (`AGENTS.md`). Deploy modules to `/usr/lib/modules/$(uname -r)/extra/`.
- Do not edit `rootfs-overlay` into the device wholesale; other tasks have uncommitted files there.
- Keep `clk_ignore_unused` and `wifibt_if` at `0xb0000000` (`AGENTS.md`). `cp_rmem` (`0xd0000000`) does not overlap it.
- Build: `make -C src/linux O=../../build/linux ARCH=arm64 LLVM=1 M=$PWD/drivers/cpif modules`. Loader: build `tools/cbd/a21s-cbd.c` on the device with `gcc -I.` (needs `cpif_uapi.h` next to it). Do not leave a host x86 `a21s-cbd` binary in `tools/cbd/`.
- Current kernel on the device is build `#11` with `CONFIG_EXYNOS_IOMMU=y` and `CONFIG_BATTERY_S2MU005=y`; flashing procedure is in `devices/samsung/a21s/docs/HANDOFF.md` §6 (note: `src/uniLoader/AGENTS.md` was deleted by owner decision, so reading/building `src/uniLoader` is allowed).
- `src/` and `build/` are gitignored. Only `socs/exynos850/drivers/cpif/`, `tools/cbd/`, `patches/`, `docs/` are tracked.

## 5. Open questions for the owner

- Is a SIM inserted, and which carrier/band? Nothing in this work can be validated end to end without one.
- Is it acceptable to risk further hard hangs (each needs a manual power cycle)?
