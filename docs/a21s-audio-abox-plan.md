# A21s Speaker Audio Plan — ABOX Port (2026-11-14)

Why there is no sound today and what the built-in-speaker path requires.

Related: [a21s-ble-gamepad.md](a21s-ble-gamepad.md) §5 (BT audio userspace), `scripts/volume-keys.{py,service}`.

---

## 0. Status 2026-11-15 — main speaker plays music ✅

**Verified on device:** a 48 kHz S16_LE stereo WAV played through the main speaker (`UAIF1 → SMA1303`) is heard as clean music, after reboot, with `calliope_version=nEB0`. The earpiece (`UAIF0 → aud3004x → EP`) produced audible output earlier. It has **not** been re-tested with music since the `abox_ion.c` fix below.

Playback recipe (after `/root/audio-up.sh`):

```sh
amixer -c 1 cset name='ABOX SPUS ASRC3' 0
echo 1536000 > /sys/bus/i2c/devices/0-001e/playback_init   # SMA1303 PLL from 1.536 MHz BCLK
aplay -D plughw:1,100 --period-size=960 --buffer-size=3840 song.wav
```

### Bugs fixed, in the order they were found

| # | Symptom | Root cause | Fix |
|---|---------|-----------|-----|
| 1 | Speaker silent | `gpb1-0..3` stayed in the idle pinctrl state | `abox_if.c`: `abox_if_select_pinctrl()` switches to `aud-i2s1-pins` during playback |
| 2 | Clicking ("ติ๊กๆ") | `UAIF1`/`UAIF2` in `exynos3830_dai[]` had no `.dai_fmt`. `exynos3830_audio_probe()` only rewrites DT-listed links (`[0..13]`), so `abox_uaif_hw_params()` hit `default:` → `ABOX_UAIF1_CTRL1 (0x0514)` got `VALID_STR=VALID_END=15`, keeping 1 of 16 bits | `exynos3830_aud3004x.c`: `.dai_fmt = I2S \| NB_NF \| CBC_CFC` on `UAIF0..2`. Check: `0514` reads `00800000` |
| 3 | `ABOX RDMA3 Width` = 8 | Default kcontrol value | `scripts/abox_speaker.sh` sets it to `16` |
| 4 | **Cicada/mosquito noise on *both* speakers** (the real bug) | `abox_ion.c` mainline shim (replaces ION) built its `sg_table` with `sg_alloc_table()` + `sg_dma_address()` only, leaving **no `struct page`**. `abox_iommu_map_sg()` → `iommu_map_sg()` uses `sg_phys()` = `page_to_phys(NULL)`, so the Calliope IOVA for the PCM buffer pointed at **random RAM**. The DSP pipeline timing was perfect (log-00 clean), but the samples were garbage | Build the table with `dma_get_sgtable(dev, &sgt, vaddr, dma_addr, size)` |
| 5 | **Playback returns `EIO` ~1 s after trigger; DSP dumps registers** (`data abort(0x1, 0xab0cab0c, 0x8000bd24, 0x805)`, `r01: dead0002`, `pc : 0000b454`) | **OPEN.** Reproduces on a clean boot with PipeWire/WirePlumber masked (zero probe lines), `calliope_version=nEB0`, 0 boot retries, and a 15 KiB S16 buffer at the documented params above. So it is *not* the 768 KiB DSP ceiling, not S32 width, not capture probing. After the abort the DSP never recovers (later plays get `EIO` with no new abort) | Not fixed. See `docs/HANDOFF.md` §0.1 for the full evidence |

> [!NOTE]
> Debug lesson: if `log-00` shows `PCMOUT0 → BD_SRCCOM0 → BD_MIXER → RDMA3` running cleanly, the registers look right, and **both** codecs make the same kind of noise, suspect the shared DMA/IOMMU buffer mapping before the codec or I2S format.

### Things that turned out not to be the cause
- `ABOX UAIF1 Width=32` (64fs, 3.072 MHz BCLK with `playback_init 3072000`): changed the noise from cicada to mosquito. Keep 16 / 1.536 MHz.
- SMA1303 regs `01..04`, PLL `8b..8f` (`1.536MHz` entry `07 e0 8b 03`), `fb: 08` without `CLK_MON` fault were all fine.

### Known caveats
- **DSP buffer ceiling:** the Calliope buffer heap is 768 KiB but ALSA advertises `BUFFER_BYTES_MAX = SZ_1M` (`abox.h:41`), so a legal 1 MiB request is refused *inside* the DSP (`Not enough memory: 1048576 of 786432`) instead of failing in `hw_params`. WirePlumber hits this by opening calliope PCMs with defaults (8 ch, S32, 1 MiB). `0x1200000` in `Kbuild` is the host DRAM window, not this heap.
- **`abox_speaker.sh` runs under `set -e`** and `audio-up.sh` discards its output, so one `cset` that times out at boot silently skips the rest of the routing. It also sets `ABOX SPUS ASRC3 1`, while the working recipe above uses `ASRC3 0` — contradiction still unresolved.
- **Playback currently `EIO`s on a clean boot** (bug #5) — the recipe above was verified on an earlier build/boot and does not reproduce today.
- On some boots Calliope never signals `BOOT_DONE` (`calliope boot retry 1/8..8/8`), so audio needs another reboot. After a DSP runtime `data abort` every later boot also failed — recovery needed a manual power cycle, not a software reboot.
- **Never `rmmod exynos3830_aud3004x`.** `aud3004x_jack_remove` oopses on unload. Reboot instead.
- Loading `s2mpu12-regulator.ko` caused a black screen ~31 s after boot. See [a21s-mainline-boot.md](a21s-mainline-boot.md) § "Black screen ~30 s after boot".

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

## Progress update (continued)

**Modules all build:** `abox-snd.ko`, `aud3004x-codec.ko`,
`exynos3830-aud3004x.ko` — modpost clean (added
`MODULE_IMPORT_NS(DMA_BUF/IIO_CONSUMER)`, `kernel_write` for
`vfs_write`, prealloc fix, exynos_pmu_update stub).

**Machine driver ported** (`exynos3830_aud3004x.c`): static legacy
dai-link fields → `.cpus/.platforms/.codecs` component arrays,
`be_hw_params_fixup` split into per-direction wrappers
(`exynos3830_fixup_pb/cap` → `abox_hw_params_fixup_helper`),
obsolete DPCM trigger constants dropped, `of_gpio.h` → gpiod compat,
`codec_conf/aux_dev` `.of_node`→`.dlc.of_node`,
`snd_soc_of_parse_daifmt`→`snd_soc_daifmt_parse_format`,
`snd_soc_of_get_dai_name` 3-arg, `.remove` int→void,
`snd-soc-dummy` codec fallback kept (codec-less links still bind).

**DTS fragment done** (`patches/linux/0003-a21s-abox-audio-dts.patch`):
`abox-gic` (GIC_SPI 64), `abox@14a50000` full vendor subtree — 2×
abox-core w/ firmware descriptors, 12× rdma, 5× wdma, 5× dual,
6× ddma, 3× uaif, spdy, debug/tplg/vss, 9× ext-bin. Vendor clock
cells → mainline `cmu_aud` IDs (`FOUT_AUD_PLL`, `DOUT_AUD_AUDIF`,
`BUSD`, `CNT`, `DOUT_AUD_UAIF*`, `GOUT_AUD_UAIF*_BCLK`,
`DOUT_AUD_MCLK`, `GOUT_AUD_SPDY_BCLK`), `iommus=<&sysmmu_aud>`,
uaif pinctrl → `aud_i2s{0,1}`/`aud_codec_mclk` pins.
`sound {}` card wired w/ all links; codec phandles omitted →
`snd-soc-dummy` until aud3004x bus is ported.

**Firmware on device:** `calliope_sram{,_2}.bin`,
`calliope_dram{,_2}.bin`, `abox_tplg.bin` extracted from Android
`super` (vendor part) → `/usr/lib/firmware/`. No userspace daemon
needed (driver loads firmware itself).

**Codec dependency found:** `aud3004x@07` sits on `acpm_mfd_bus` —
registers go through Samsung `i2c-speedy` + `acpm` IPC framework
(~1.3k lines + deps). Ported codec compiles but cannot bind without
that bus → **speaker output requires acpm/speedy port** (Phase B).

**Deploy attempt #1:** `kexec -l Image --dtb=new.dtb` → device did
not return (kexec hang; `CONFIG_EXYNOS_IOMMU` not built so sysmmu
node is inert — suspect kexec instability, not DTB). Flash untouched;
force-reboot restores previous system. Next: retry kexec w/ stock
DTB to isolate, else `boot-avb.img` + dd path per BUILD.md §C.

## Codec bus architecture (Phase B discovery)

`aud3004x` is NOT on a normal AP I2C bus. Vendor DT puts it on
`acpm_mfd_bus@11A50000` ("samsung,exynos-acpm-mfd-bus"):

```
aud3004x (i2c_driver)
  → acpm-mfd-bus.c i2c_adapter (139 lines, drivers/i2c/busses/)
    → acpm_mfd.c exynos_acpm_{read,write,bulk}_reg
      → acpm_ipc.c IPC protocol over APM mailbox
        → APM coprocessor firmware (RTOS, in APM SRAM)
          → SPEEDY serial → aud3004x codec
```

Vendor source (already in repo `drivers/acpm/`):
- `acpm.c` (449 ln) — loads APM "plugin" firmware blobs into APM SRAM
  (plugins table at `initdata-base 0x7000`, firmware names read from
  SRAM after base fw boot)
- `acpm_ipc.c` (866 ln) — IPC channels, mailbox @0x11900000,
  SRAM @0x02039000, irq SPI 49
- `acpm_mfd.c` (243 ln) — register accessors (above)
- `acpm-mfd-bus.c` (139 ln) — i2c adapter over the above
- DT: `acpm@11830000` (timer_apm), `acpm_ipc` node,
  `acpm_mfd_bus@11A50000` (disabled in base, enabled per-model)

**Open question:** does uniLoader/s-boot load the base APM firmware?
If not, APM must be brought up from scratch (extract `apm*.bin`
plugins from Android vendor part + port the full chain). Same class
of work as SCSC — codec audio output is gated on this.
