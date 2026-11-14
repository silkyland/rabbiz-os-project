# A21s Speaker Audio Plan — ABOX Port (2026-11-14)

Why there is no sound today and what the built-in-speaker path requires.

Related: [a21s-ble-gamepad.md](a21s-ble-gamepad.md) §5 (BT audio userspace), `scripts/volume-keys.{py,service}`.

---

## 1. Current state (verified on device)

- **No sound card**: `/proc/asound` absent, `lsmod` shows no `snd_*`, PipeWire `wpctl status` → 0 Devices/Sinks.
- Kernel `7.3.0-g14a425b3a7dc-dirty` has `CONFIG_SND=m` + lots of codecs and `CONFIG_SND_SOC_SAMSUNG=m`, `snd-usb-audio.ko` present in modules tree — but **no ABOX driver**.
- Devicetree carries audio pinctrl only (`aud-i2s0/1-pins`, `aud-codec-mclk-pins`, `aud-fm-pins` under `pinctrl@14a60000`); **no `sound`/ABOX node**.
- **Userspace is done**: `bluez` 5.88 + `pipewire` + `wireplumber` + `pipewire-pulse` (bluez5 SPA codecs SBC/AAC/aptX/LDAC/HFP); `pipewire.socket`, `pipewire-pulse.socket`, `wireplumber.service` all enabled+active in `gamer` session; Steam already registered as a PipeWire client.

## 2. Why the speaker needs an ABOX port

Exynos 850 routes all audio through **ABOX** (Audio BOX — embedded audio DSP with own firmware, DMA, I2S, GIC), then to the codec + speaker amp. Mainline has **no mobile-ABOX driver**: the only upstream work is the *Exynos Automotive* ABOX series (`samsung,exynosauto.yaml`, `sound/soc/samsung/auto_abox/`, ew.kim@samsung.com, July 2025, still in review) — different SoC family, useful as reference only.

This is the same class of task as the SCSC WLBT port: extract the vendor driver, adapt to mainline APIs, drive it from the device tree.

## 3. Where the pieces live (all on the build machine / Android `super`)

| Piece | Location | Notes |
|---|---|---|
| ABOX platform driver | `src/exynos850-vendor-kernel` — grep `abox` under `drivers/soc/samsung/` and `sound/soc/samsung/` | firmware load, mailbox/IPC, SRAM, clk/PM |
| ASoC card + codec | vendor `sound/soc/samsung/` + `sound/soc/codecs/`; get exact compatibles from the **vendor DTB** (`src/tools/AIK-Linux/split_img/boot.img-dtb`, decompile with `dtc`) | codec + speaker-amp compatibles identify which codec driver to port |
| ABOX firmware (`abox*.bin` / `calliope*`) | Android `super` (`/dev/mmcblk0p30`) → `lpunpack`/`simgt2img` → vendor logical partition `/vendor/firmware` | same source the `mx140` Wi-Fi FW came from |
| DT node | vendor `exynos850-a217f*.dts` (`samsung,exynos850-abox` node, `aud-*` pin groups already partially present) | add `sound {}` card node + `status="okay"` |

## 4. Work items (build machine)

- [ ] Decompile vendor DTB → extract abox/card/codec nodes & pinctrl usage into `exynos850-a217f.dts`.
- [ ] Port vendor `abox` platform driver to 7.3 (expect: mailbox API changes, `class_create` single-arg, `proc_ops`, `firmware` const, ioremap style — same class of fixes as `scsc_*`).
- [ ] Port ASoC card + codec/amp drivers; wire `sound-dai` links to `abox` PCM/DSIF frontends.
- [ ] Extract `abox` firmware from `super` → `/lib/firmware/` (never `tar` a `lib/` into `/`).
- [ ] Ship `alsa-ucm-conf`/mixer init if needed; verify `aplay -l`, `speaker-test` on the speaker sink in PipeWire.

## 5. Volume keys — done today

`volume-keys.service` runs `/usr/local/bin/volume-keys.py` (this repo `scripts/`): listens on every evdev device advertising `KEY_VOLUME*` — currently `gpio-keys` (phone buttons) and `MX Keys Keyboard` — and calls `wpctl` in the `gamer` session (`5%±`, mute toggle, capped at 150%). Verified the `runuser`/`wpctl` path reaches PipeWire (`get-volume` → 1.00); it activates the moment a sink exists — works for BT and USB sinks too, not just the future ABOX card.

## 6. Interim real-audio paths (zero extra work)

- **Bluetooth speaker/headphones**: pair like the gamepad/keyboard → bluez5 SPA creates the sink automatically.
- **USB-C audio dongle/hub**: `snd-usb-audio.ko` is in the modules tree → plug and it registers.

## Port progress (in-flight, not yet runtime-verified)

**Status:** `drivers/abox/` (76 files, ported from
`src/exynos850-vendor-kernel/sound/soc/samsung/abox/`) compiles as
out-of-tree `abox-snd.ko` against mainline 7.3 — all 23 vendor objects
+ `smc_compat.c` link cleanly. `modpost` still pending on the first
full in-tree `modules` build (Module.symvers).

**Build:** `drivers/abox/Kbuild` — single module with merged
`module_init` registering all 14 platform drivers (vendor built them
as separate built-in initcalls). Config defines:
`SND_SOC_SAMSUNG_ABOX_VERSION=0x23000`, `ABOX_V3`,
`DRAM_SIZE=0x1200000`, `SOC_EXYNOS3830`.

**Mainline API fixes applied (mechanical):**
- `module_platform_driver` x14 → shared init/exit table in abox.c
- `.remove` int→void (14 files); `set_fs`/`get_fs` removed (dump/log)
- `dai_link` string fields → `cpus[]`/`codecs[]`/`platforms[]`
- `snd_pcm_ops`/`snd_compr_ops` → flattened component ops +
  `struct snd_compress_ops` (leading `component` arg)
- `rtd->cpu_dai` → `snd_soc_rtd_to_cpu(rtd, 0)`;
  `snd_soc_component_read(3-arg)` → `snd_soc_component_read_v` shim
  (abox.h); `snd_soc_kcontrol_component` shim (private_data)
- `digital_mute`→`mute_stream`, `symmetric_*` renames,
  `SND_SOC_DAIFMT_CBS_CFS`→`CBC_CFC`, `CBM_CFM`→`CBP_CFP`
- `wakeup_source_init/trash` → `wakeup_source_register/unregister`
- `strncpy/strlcpy` → `strscpy`; `memblock_free`→`memblock_phys_free`
- `FW_ACTION_HOTPLUG`→`true`; `iommu_map_sg` +GFP arg
- `w->dapm->dev/component` → `snd_soc_dapm_to_dev/component()`
- `dai->rate/channels/sample_width` → `abox_if_data` fields
- `devm_pinctrl_get`/`linux/compat.h`/`linux/memblock.h`/`linux/vmalloc.h` includes

**Vendor-only deps resolved via `drivers/vendor-include/` + stubs:**
- `exynos_smc`/`exynos_smc_readsfr` implemented in `smc_compat.c`
  (inline `smc #0`, mirrors vendor .S)
- `PM_QOS_*` / `pm_qos` → tracking-only `abox_qos` (real DVFS hints
  dropped for bring-up — revisit later)
- `linux/shm_ipc.h`, `modem_notifier.h`, `exynos-pmu.h`,
  `exynos-itmon.h`, `exynos_iovmm.h` → vendor headers with built-in
  stubs when CONFIG_* unset
- `linux/smc.h` renamed `exynos_smc.h` (collides w/ uapi AF_SMC hdr)
- `ion_alloc_dmabuf` → `dma_alloc_coherent` + `dma_buf_export` shim
  in abox_ion.c
- `IRQF_GIC_MULTI_TARGET` define in exynos_smc.h (0x10000000)

**Kernel-side changes needed:** `patches/linux/0002-...` makes
`SND_SOC_TOPOLOGY` user-visible (select-only upstream) + config:
`SND_SOC_TOPOLOGY=y`, `SND_SOC_COMPRESS`, `SND_SOC_SAMSUNG`.

**Still TODO:** DTS fragment (vendor `exynos3830.dts` abox subtree →
`exynos850-a217f.dts`), sysmmu_abox + cmu_aud + pd-aud deps, ABOX
firmware extraction (`calliope_*.bin` from super.img), AUD3004X codec
+ exynos3830_sound machine driver, runtime probe.
