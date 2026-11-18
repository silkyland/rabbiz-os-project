---
status: verified
lane: kernel
device: samsung/a21s
kernel: 7.3.0-g14a425b3a7dc-dirty
author: devin-kernel-agent
ai_assisted: true
date: 2026-11-18
evidence:
  - file: devices/samsung/a21s/docs/HANDOFF.md
  - file: devices/samsung/a21s/docs/a21s-audio-abox-plan.md
related:
  - devices/samsung/a21s/findings/2026-11-18-als-zero-chub-scan.md
supersedes: []
---

# The Calliope march — the eleven ABOX root causes (extracted from HANDOFF §2)

Consolidated from the engineering handoff so the debugging saga survives as
one finding. All fixes are already in `socs/exynos850/drivers/abox/` and the
A21s kernel patches. Symptom at the time: `modprobe exynos3830-aud3004x` →
Core 0 crashed before `ABOX_BOOT_DONE` (`Invalid calliope state: 2`), no
PCMs, occasional ASoC NULL-deref oops.

## The signature

Firmware-instrumented debugging (patched exception vectors, milestone
markers, SRAM/DRAM probing via `0.abox-debug` debugfs + a custom
`abox_diag` module) showed Core 0 taking a **prefetch abort in Thumb
state, marching through zeros to `0x81200000`** — one byte past the
mapped `0x1200000` DRAM window — while the host-visible DRAM buffer was
full of valid firmware. The DSP was fetching zeros.

## Root causes (all fixed)

1. **DMA IOVA mapped as physical** — `dmam_alloc_coherent()` returns an
   IOVA (`0xfe000000` via `sysmmu_aud`), not a PA. The driver fed it to
   the Calliope SysMMU as a PA → DSP fetched empty RAM. Fixed by mapping
   `virt_to_phys(dram_base)` instead.
2. **Cores running during firmware download** — `abox_cpu_power(true)`
   is a no-op when already powered; the running DSP stamped exception
   frames over the new image. Fixed by `cpu_enable(false);
   cpu_power(false); udelay(100)` before download.
3. **Stale debug firmware in `/lib/firmware`** — an instrumented
   `calliope_sram.bin` halted the core on recoverable faults. Restoring
   the pristine blob restored `ABOX_BOOT_DONE`.
4. **Missing `samsung,aux` → topology never loaded** — the `sound` node
   needed `samsung,aux = <&abox_tplg>` or `abox_tplg.bin` never loaded
   and no FE PCMs existed.
5. **dump-card NULL deref** — dummy dai_links lacked `platforms`;
   `snd_soc_add_pcm_runtimes` dereferenced it. Fixed with
   `snd-soc-dummy-platform`.
6. **NULL drvdata in `abox_dma_hw_params_set`** — `snd-soc-dummy` cpu
   DAIs → `dev_get_drvdata` NULL. NULL-guarded.
7. **Stale blob Image regression** — `src/uniLoader/blob/Image` once
   predated `CONFIG_EXYNOS_IOMMU=y`; flashing it removed all sysmmu
   domains. Rule: refresh blob Image+dtb before every repack.
8. **`BUG_ON` killed the DSP IPC workqueue** — an error storm past 128
   consecutive IPC failures oopsed the `abox_ipc` kworker → every later
   mixer/PCM IPC hung in D-state. Replaced with `dev_err_ratelimited`.
9. **`samsung-abox-dump` re-bound the card** — its probe re-triggered
   `snd_soc_bind_card` on the live card → `snd_soc_dapm_free` NULL deref
   → kworker died holding the DAPM mutex → all alsa calls hung. Probe
   now skips the component/dai_link for `id >= 0`.
10. **sma1303 as codec_dai destabilized card bind** — kept codec links
    `snd-soc-dummy`; the amp is configured out-of-band via the
    `playback_init` sysfs attr instead.
11. **No `s2mpu12` regulator driver → amp unpowered** — PMIC audio rails
    were off; `s2mpu12-regulator.c` registers 29 rails over the ACPM
    regmap. `vdd_aldo1/2` came up → `sma1303` read chip version `0x11`.

## Reusable methods

- `dma_addr_t` from `dmam_alloc_coherent` on an IOMMU'd device is an
  IOVA; the PA question needs `virt_to_phys`/`vmalloc_to_page`.
- Compare `build/linux/.config` vs `/proc/config.gz` when a flashed
  kernel behaves older than the tree.
- Instrumented firmware markers + halt-on-fault stubs locate *where* a
  DSP dies, but must be reverted — stale debug firmware is itself a bug
  source (cause #3).
- `BUG_ON` in IPC error paths converts a recoverable DSP fault into a
  permanent wedge; prefer ratelimited errors.
