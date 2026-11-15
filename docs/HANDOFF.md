# Engineering Handoff — Galaxy A21s (`SM-A217F` / Exynos 850)

Last updated: 2026-11-15 (PipeWire integration paused — DSP playback data-aborts on a clean boot)

---

## 0. Latest session — PipeWire/Steam audio integration (in-flight)

**Goal:** unmask PipeWire, expose the ABOX speaker as the default sink for Steam, then M5 (battery/power) + M6 (modem).

### Done & verified on device

- **Unmasked PipeWire at both layers.** Mask existed twice: `/etc/systemd/system/pipewire{,-pulse}.service`, `wireplumber.service → /dev/null` *and* `/home/gamer/.config/systemd/user/{pipewire.service,pipewire.socket,pipewire-pulse.service,pipewire-pulse.socket,wireplumber.service} → /dev/null`. Both removed. User-session enable links (`pipewire.socket`, `pipewire-pulse.socket` in `sockets.target.wants`, `wireplumber` in `pipewire.service.wants`) were already in `/etc/systemd/user/` → sockets auto-start in the `gamer` session.
- **UCM2 written for `Exynos3830-Madera` — the card now produces a real ACP sink.** Deployed to repo `rootfs-overlay/usr/share/alsa/ucm2/`:
  - `conf.d/Exynos3830-Made/Exynos3830-Madera.conf` — master (`Syntax 3`, `SectionUseCase."HiFi" → File "/Exynos3830-Madera/HiFi.conf"`).
  - `Exynos3830-Madera/HiFi.conf` — verb: `SectionDevice."Speaker"`, `PlaybackPCM "hw:${CardId},100"`, Enable `cset "name='SPK SPK On' on"`.
- **UCM lookup gotchas (hard-won, document before forgetting):**
  - alsa-lib 1.2.16 probes `ucm2/conf.d/${CardDriver}/${CardLongName}.conf` then `…/${CardDriver}.conf`. `CardDriver` = `snd_ctl_card_info_get_driver()` = **`Exynos3830-Made`** (the driver field truncates `exynos3830-audio` to 15 chars!). NOT `exynos3830_aud3004x` (that's PipeWire's `alsa.driver_name` from sysfs) and NOT `exynos3830-audio` (kernel sysfs driver). Only `conf.d/Exynos3830-Made/` works.
  - The `conf.d` master file only lists verbs (`Syntax 3` + `SectionUseCase`); `SectionVerb`/`SectionDevice` live in the referenced verb file. Putting `SectionVerb` directly in the conf.d file → `unknown master file field`.
  - UCM `cset` takes exact name + value in one string: `cset "name='SPK SPK On' on"`. The `name=^X*` regex form is NOT valid UCM.
- **Real codec controls:** `Speaker Mute Switch(1:muted_0:un)`/`Speaker Mode`/`Speaker Volume` in the old `audio-up.sh` were silent no-ops (names don't exist — amixer errors were hidden by `2>&1`). The aud3004x speaker switch is **`SPK SPK On`** (numid 593; also `EP EP On`, `HP HP On`, `LINEOUT LINEOUT On`, `MIC1..3`, `INP_SEL_L/R`).
- **Default sink persisted:** `wpctl set-default` → wireplumber stores `alsa_output.platform-sound.HiFi__Speaker__sink` in Default Configured Devices; survives reboot. `pactl` reports PipeWire server + correct default; `pipewire-pulse` socket-activated.
- **Boot integration:** `abox-audio.service` (oneshot → `/root/audio-up.sh`) created + enabled; repo copies in `rootfs-overlay/etc/systemd/system/` and `scripts/audio-up.sh` (updated: drops the bogus csets, writes `playback_init`, sets `SPK SPK On`).

### Open problems

1. **DSP data-aborts on playback ~1 s after trigger — with or without PipeWire (main blocker).** Not a buffer-size, format or probing problem: see §0.1. A 15 KiB S16 buffer at the documented known-good params still kills the DSP.
2. **768 KiB DSP ceiling is real and is exceeded by ALSA's own maximum.** `BUFFER_BYTES_MAX = SZ_1M` while the firmware heap is 768 KiB → a legal request reaches the DSP and is refused there. Capture probing makes it worse but is not the trigger. See §0.1.
3. **Calliope boot flake persists** — 2 of 4 boots today never signaled BOOT_DONE (all 8 `boot_retry` pulses failed). Recovery = reboot only. Pattern hints warm-reboot state dependence (a good boot followed by a wedged/rebooted DSP tends to fail).
4. **Slow mixer enumeration** — `amixer scontrols`/`sget` "Timer expired"; wireplumber `spa.alsa: Unable to load mixer` + `Failed to find a working mixer device (_ucm0003.hw:Exynos3830Mader)`; ~60–90 s before `wpctl` responds post-start. Some of the 722 kcontrols' get path likely hits the DSP mailbox. Effect: **no hardware volume on the sink (soft volume only)** — acceptable for now.
5. Pre-existing cosmetic: `abox_rdma close timeout`.

### 0.1 Round 2 — DRAM ceiling characterised, playback blocker moved (2026-11-15 16:00)

Goal was to isolate the `paplay` wedge (open problems #1/#2 from Round 1). Outcome: **the 768 KiB ceiling is real and now fully explained, but it is not what breaks playback.** Raw log: `/tmp/opencode/dsp-experiment-log.md`.

**VERIFIED — where the 1 MiB request comes from**

WirePlumber's ALSA monitor opens every calliope PCM with ALSA *defaults*; `dmesg` shows it verbatim:

```
abox-wdma 14a53100: ion_buffer abox_wdma_hw_params bytes(524288) size(524288)
abox-wdma 14a53300: Capture:Total=1048576 PrdSz=1024(32768) #Prds=32 rate=48000, width=32, channels=8
abox-vdma .103    : Playback:Total=1048576 PrdSz=1024(32768) #Prds=32 rate=48000, width=32, channels=8
```

`1048576` is exactly the number in the firmware refusal `Not enough memory: 1048576 of 786432`. The mismatch is in the source, not the DSP:

- `BUFFER_BYTES_MAX = SZ_1M` (`drivers/abox/abox.h:41`; applied at `abox_dma.c:43`, `abox_rdma.c:1199`, `abox_wdma.c:59`) — ALSA will accept a 1 MiB buffer.
- Firmware buffer heap = 768 KiB (`786432`). A legal request is therefore rejected *inside* the DSP instead of failing in `hw_params`.
- `CONFIG_SND_SOC_SAMSUNG_ABOX_DRAM_SIZE=0x1200000` (18 MiB, `Kbuild:3` → `DRAM_FIRMWARE_SIZE`) is the **host-side DRAM window**, **not** the 768 KiB heap. Do not conflate the two.

**VERIFIED — probing is not the cause of the playback failure**

A/B test: masked the whole user audio stack (`/home/gamer/.config/systemd/user/{pipewire,pipewire-pulse}.{service,socket}` and `wireplumber.service` → `/dev/null`), rebooted → **zero** `Total=` probe lines at boot, no `pipewire`/`wireplumber` processes, `calliope_version=nEB0`, 0 boot retries. Playback still failed:

```
aplay -D plughw:1,100 --period-size=960 --buffer-size=3840 /root/s16-48k.wav
aplay: pcm_write:2191: write error: Input/output error
```

with exactly the documented known-good params (`Total=15360 PrdSz=960(3840) #Prds=4 width=16 channels=2`, `bclk=1529174`), then ~1 s after `abox_rdma_trigger[3](1)`:

```
data abort(0x1, 0xab0cab0c, 0x8000bd24, 0x805) is reported from calliope
A-Box core register dump (nEB0)
CORE: 0   r00: ab0cab0c  r01: dead0002  ...  pc : 0000b454
```

After that abort the DSP stops serving PCM IPC: a second attempt with `ASRC3=0` + `playback_init` returns `EIO` with **no** new abort line while `calliope_version` still reads `nEB0`. Re-confirms: **Calliope does not self-recover from a runtime data abort.**

**VERIFIED — boot-window IPC timeouts plus a script bug**

Before any playback on that boot: `abox_msg_send: timeout: 1, 193, 43788, 262/268/275/284, 0, …` (4×). Mixer IPC was healthy afterwards (`amixer -c 1 cset name='ABOX SPUS ASRC3' 0` → exit 0), so it is a boot-window race, not a dead mailbox.

`scripts/abox_speaker.sh` runs under `set -e` and `audio-up.sh` swallows its output (`>/dev/null 2>&1`): one timed-out `cset` at boot silently skips the remainder of the routing. It also sets `ABOX SPUS ASRC3 1`, while the documented working recipe (`docs/a21s-audio-abox-plan.md` §0) uses `ASRC3 0`. **Unresolved contradiction.**

**Boot reliability got worse.** 3 good boots in a row (0 retries) at 16:03/16:06/16:12; afterwards every boot failed `BOOT_DONE` (8/8 retries). A software `reboot` left the host unreachable on `usb0` **and** `wlan0` twice in a row — recovery needs a **manual power cycle**, not a reboot.

**Not yet tested** (device went offline before these ran): `ASRC3=0` + `playback_init` applied *before the first* playback on a clean boot; S32-vs-S16 width isolation; a deliberate 1 MiB (`--buffer-size=262144` frames S16) request. Test WAVs are on the device: `/root/s16-48k.wav`, `/root/s32-48k.wav`.

### Device state right now

Last reboot: **host unreachable on both `usb0` and `wlan0`** after a software reboot — needs a manual power cycle. Before that, `Calliope` failed `BOOT_DONE` on every boot since the 16:12 DSP data abort (`calliope_version` empty, 8/8 retries).

**⚠️ Left behind by Round 2 — must be undone for Steam audio:** the user audio stack is masked in
`/home/gamer/.config/systemd/user/` (`pipewire.service`, `pipewire.socket`, `pipewire-pulse.service`, `pipewire-pulse.socket`, `wireplumber.service` → `/dev/null`). Remove those five symlinks (or reboot after deleting them) to restore PipeWire/WirePlumber. `abox-audio.service` was left untouched and enabled.

### Resume recipe

```bash
# wait for a good Calliope boot
ssh root@172.16.42.1 'tr -d "\0" < /sys/devices/platform/soc@0/14a50000.abox/calliope_version'   # want: nEB0
# isolate the breaking param BEFORE starting PW streams
aplay -D plughw:1,100 --period-size=960 --buffer-size=3840 song.wav          # known-good (S16)
aplay -D hw:1,100 s32.wav                                                   # suspect
# PW session (gamer)
runuser -u gamer -- env XDG_RUNTIME_DIR=/run/user/1001 \
  DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1001/bus wpctl status
# force S16 in PW if S32 is the breaker — ~/.config/wireplumber/wireplumber.conf.d/
#   monitor.alsa.rules match on node.name ~alsa_output.platform-sound* :
#   update-props { audio.format=S16LE audio.rate=48000 api.alsa.period-size=960 api.alsa.period-num=4 }
```

Then M5 (`schedutil` + `cpuidle` → `s2mu106` gauge/charger + `upower` → TMU/backlight/suspend per `docs/a21s-power-battery-modem-plan.md`), then M6.

---

## 1. Current Verified Device State

- **Kernel:** Mainline Linux `7.3.0-g14a425b3a7dc-dirty` (`LLVM=1` Clang) with `exynos850-a217f.dtb` (`patches/linux/0001`..`0004` applied).
  - `CONFIG_EXYNOS_IOMMU=y` **must** be in the flashed kernel — a stale `src/uniLoader/blob/Image` without it silently breaks the whole ABOX path (`iommu_paging_domain_alloc` → `Unable to alloc iommu domain`). Always refresh `blob/Image` + `blob/dtb` before repacking `boot-avb.img`.
- **Boot & Display:** `uniLoader` (`06a1bba`) + `decon850` + `dsi` (`td4150`) + `panfrost`/`panvk` + `steamos-session.service`.
- **Networking:** `usb0` (`172.16.42.1/24`, systemd-networkd, unmanaged in NM) + `wlan0` (`scsc_wlan`, NetworkManager, `192.168.1.142`).
- **Bluetooth & Gamepads:** `scsc_bt.ko` (`hci0`) + BlueZ 5.88 + PipeWire + HID/UHID.
- **Audio (`abox` — ✅ INTERNAL SPEAKER PLAYS MUSIC, verified on device):**
  - Main speaker `UAIF1 → SMA1303` plays clean music; earpiece `UAIF0 → aud3004x → EP` also verified. `calliope_version=nEB0`.
  - **Playback recipe** (after `/root/audio-up.sh`): `amixer -c 1 cset name='ABOX SPUS ASRC3' 0` → `echo 1536000 > /sys/bus/i2c/devices/0-001e/playback_init` → `aplay -D plughw:1,100 --period-size=960 --buffer-size=3840 song.wav` (small buffers required — DSP RDMA buffer is tiny).
  - Card 1 `Exynos3830-Madera` registers only after **`aud3004x-codec` module is loaded** (machine driver links the codec into UAIF links).
  - Full bug table in `docs/a21s-audio-abox-plan.md` §0 — headline fix: `abox_ion.c` sg_table needed `dma_get_sgtable()` (was mapping random RAM → cicada noise).
  - **PMIC `s2mpu12-regulator` DONE** — `drivers/abox/s2mpu12-regulator.c` registers 29 rails via ACPM regmap; `vdd_aldo1`=1.8 V, `vdd_aldo2`=2.9 V enabled (audio codec/amp rails).
  - **`sma1303` smart amp POWERED + responding** — `0-001e` reads `chip version 0x11`, `playback_init` sysfs configures I2S-slave + PLL-from-BCLK + power-on + mono + unmute out-of-band (no codec_dai link needed — avoids bind crash).
  - **Still missing: real audio out the speaker.** DSP→UAIF1→sma1303 chain is configured but produces no sound — either (a) DSP isn't emitting real audio on UAIF1 (its render graph needs the codec/effects path), (b) the speaker actually routes through `aud3004x` codec (DAC→SPKDRV→analog) not sma1303-direct, or (c) amp I2S format/PLL mismatch. **`aud3004x` codec still not in card 1's ASoC path** (dummy links; ACPM/SPEEDY bus node `acpm_mfd_bus@11A50000`/`aud3004x@07` not yet added to DT).

### ⚠️ Module deploy paths (cost a full debug cycle — DO NOT get wrong)

`modprobe`/`modinfo` resolve modules from **subdirectories**, not the top level of `/usr/lib/modules/$(uname -r)/`:

| Module | Correct deploy path |
|---|---|
| `snd_soc_core` | `…/kernel/sound/soc/snd-soc-core.ko.zst` (**zstd-compressed**) |
| `exynos3830_aud3004x` | `…/extra/exynos3830-aud3004x.ko` (uncompressed ELF) |
| `abox_snd` | `…/extra/abox-snd.ko` (+ `.ko.zst`) |
| `aud3004x_codec` | `…/extra/aud3004x-codec.ko` |

Deploying a rebuilt `.ko` to `…/KVER/` top-level is **silently ignored** — the running module stays the old build. Always `modinfo <mod>` to confirm the real `filename:` before assuming a deploy took effect. To replace `snd-soc-core.ko.zst`: `zstd -q -f -o snd-soc-core.ko.zst snd-soc-core.ko` then scp to `kernel/sound/soc/`.

### DSP memory ceiling

Calliope DRAM workspace is ~**768 KiB** (`786432` B). A 1 MiB WDMA capture buffer request → `CALLIOPE2:ERROR Not enough memory: 1048576 of 786432` → `ipc_send_debug_info(0xdead0001, 0x8000b538)` data abort. Keep PipeWire/WirePlumber **masked** during bring-up; they open capture streams that exhaust DSP memory and crash it.

---

## 2. The Calliope March — Root Cause Story

### Symptom

`modprobe exynos3830-aud3004x` → Calliope Core 0 crashes before the `ABOX_BOOT_DONE` IPC: `Invalid calliope state: 2` (stuck `CALLIOPE_ENABLING`), no playback/capture PCMs from topology, sometimes an ASoC NULL-deref oops.

### What the DSP was actually doing

Firmware-instrumented debugging (patched exception vectors + milestone markers + direct SRAM/DRAM probing via `0.abox-debug` debugfs and a custom `abox_diag` module):

1. Core 0 executes SRAM init (mode stacks, MMU build, `cpsie if`, init-table runner at `0x58`), writes `"init"` (`0x696e6974`) at SRAM `0x23000` **on good runs only**.
2. On failing runs it takes a **prefetch abort in Thumb state** marching through zeros to `0x81200000` (the first byte past the mapped `0x1200000` DRAM window). The march entry point varied between runs (`bl #0x114` trampoline → DRAM memset, decompressor `0x800005ca`, `bl #0xaaa0` GIC init) — always the same signature: a call into `calliope_dram.bin` code that fetched zeros instead of instructions.
3. Host-side debugfs showed the DRAM buffer **full of valid firmware** (`CLP8` magic + ARM code) — while a `memremap` probe of the SysMMU's target **physical address `0xfe000000` showed all zeros**.

### Root cause #1 — DMA IOVA mapped as physical address

`dmam_alloc_coherent()` returns a `dma_addr_t` in the **device's DMA domain** — for `14a50000.abox` (which has `iommus = <&sysmmu_aud>`), that is an **IOVA `0xfe000000`**, *not* a physical address. The driver stored it in `data->dram_base_phys` and mapped it into the **ABOX paging domain** (`IOVA 0x80000000 → "PA" 0xfe000000`). The Calliope SysMMU then emitted real PA `0xfe000000` onto the interconnect — an empty RAM region — so the DSP fetched zeros → Thumb march → PABT.

The CPU-visible `virt_to_phys(dram_base)` gives the real PA (observed `0x8aad00000` / `0xb8400000` / `0xb9600000` across boots — dynamic each time).

**Fix** (`drivers/abox/abox.c`, `samsung_abox_probe`): compute `dram_pa = virt_to_phys(dram_base)` (with `vmalloc_to_page` fallback) and map *that*:

```c
if (is_vmalloc_addr(data->dram_base))
    dram_pa = page_to_phys(vmalloc_to_page(data->dram_base));
else
    dram_pa = virt_to_phys(data->dram_base);
abox_iommu_map(dev, IOVA_DRAM_FIRMWARE, dram_pa,
        DRAM_FIRMWARE_SIZE, data->dram_base);
```

### Root cause #2 — cores not held in reset during download

The boot chain leaves the Calliope cores **powered and running**. `abox_cpu_power(true)` is a PMU RMW no-op when the power bit is already set — so the DSP kept executing a half-written image and stamped exception frames over the low SRAM (48 KB of identical 32-byte frames — destroyed vector table + trampolines).

**Fix** (`abox_enable`, `has_reset` path): `abox_cpu_enable(false); abox_cpu_power(false); udelay(100);` **before** `abox_download_firmware()`.

### Root cause #3 — stale debug firmware in `/lib/firmware`

A marker-instrumented `calliope_sram.bin` (48 304 B vs stock 47 964 B) was left deployed. Its exception stubs **halt the core on any fault** — stock firmware hits recoverable faults post-`init` and continues, but the stub parked Core 0 → DSP booted (`"init"` written, cores running real code) yet never sent `ABOX_BOOT_DONE` → driver timeout. Restoring the pristine image (saved as `calliope_sram.bin.orig`, md5 `5a4586cb…`) made the handshake reliable: **`Calliope is ready to sing` + 38 IPC interrupts on the next boot.**

### Root cause #4 — missing `samsung,aux` → topology never loaded

The `sound` node in `patches/linux/0003-a21s-abox-audio-dts.patch` lacked `samsung,aux = <&abox_tplg>` (the vendor dts binds it via overlay). With zero aux_devs the `0.abox-tplg` ASoC component registered but its `.probe` never ran → `abox_tplg.bin` never loaded → no FE PCMs (RDMA DAIs are BE-only). Also note the machine-driver `Failed to parse cpu DAI for rdma0: -517` defer is *normal* — it retries after `14a51xxx.abox-rdma` probes.

**Fix:** label the node `abox_tplg: abox-tplg@0` and add `samsung,aux = <&abox_tplg>;` to `sound`. Result: `loaded abox_tplg.bin` + `calliope_0..9` playback + `calliope_10..29` capture PCMs.

### Root cause #5 — dump-card NULL deref

`abox_dump_register_card_work_func` built dummy `dai_link`s with `cpus`/`codecs` but no `platforms`; mainline `snd_soc_add_pcm_runtimes` dereferenced it → oops. **Fix:** allocate a `snd-soc-dummy-platform` platform component (`drivers/abox/abox_dump.c`).

### Root cause #6 — NULL drvdata in `abox_dma_hw_params_set`

Topology links whose cpu DAI resolves to `snd-soc-dummy` call `abox_dma_hw_params_set(dai->dev, …)` → `dev_get_drvdata` NULL → deref at `+0xd4`. **Fix:** NULL guard with an identifying `dev_err` (`drivers/abox/abox_dma.c`). Benign `no dma data on snd-soc-dummy` warnings remain — cosmetic for now.

### Root cause #7 — kernel regression via stale blob Image

`src/uniLoader/blob/Image` was a **23:10 build predating `CONFIG_EXYNOS_IOMMU=y`**. Reflashing it downgraded the kernel → all sysmmu devices driverless → `Unable to alloc iommu domain`. Detected by diffing `build/linux/.config` vs `/proc/config.gz`. **Fix:** always `cp build/linux/arch/arm64/boot/Image src/uniLoader/blob/Image` (and the dtb) before repacking.

### Root cause #8 — `BUG_ON` kills the DSP IPC workqueue

`abox_process_ipc` (`drivers/abox/abox.c`) counted consecutive `__abox_process_ipc` failures and hit `BUG_ON(error++ > 128)` → oops → `abox_ipc` workqueue dead → **every subsequent mixer/PCM IPC hangs in `D` state forever** (device "alive" but audio wedged). Triggered whenever the DSP stopped answering IPC (e.g. after a stream error). **Fix:** replaced `BUG_ON` with `dev_err_ratelimited` — workqueue survives error storms; `abox_failsafe_report` still runs.

### Root cause #9 — `samsung-abox-dump` re-binds the card → `snd_soc_dapm_free` NULL deref

Every DSP buffer registers a `samsung-abox-dump` platform device; its probe called `devm_snd_soc_register_component` + `abox_dump_add_dai_link`, which re-triggers `snd_soc_bind_card` on the **already-instantiated** card → `soc_cleanup_card_resources` → `snd_soc_dapm_free` NULL deref (`pc : snd_soc_dapm_free+0x24`) → kworker dies holding a corrupted DAPM mutex → **all `amixer`/`aplay` hang in `D`**. This is why the whole card wedged after module load. **Fix:** in `samsung_abox_dump_probe` (`drivers/abox/abox_dump.c`), skip `devm_snd_soc_register_component`/`abox_dump_add_dai_link` for `id>=0` — dump debugfs files are created in `abox_dump_register` anyway; the ASoC component/dai_link is optional debug plumbing. Result: **zero oops, card stable**.

### Root cause #10 — sma1303 as codec_dai destabilizes card bind

Adding `sma1303` as `.codecs` on the `UAIF1` BE link made its DAI ops fire (good) but its late component registration re-triggered card bind → contributed to the cleanup crash path. **Decision:** keep machine codec links `snd-soc-dummy`; configure the amp **out-of-band** via the new `playback_init` sysfs attr (`echo <BCLK_Hz> > /sys/bus/i2c/devices/0-001e/playback_init`) which does I2S-slave + PLL + power + mono + unmute directly over regmap.

### Root cause #11 — `s2mpu12` regulator had no driver → amp unpowered

`sma1303` I2C returned `-ENXIO`/version `0x00` because the PMIC audio rails were off. `sec-pmic-acpm` enumerated `S2MPU12` but had no regulator child. **Fix:** `drivers/abox/s2mpu12-regulator.c` registers 29 LDO/buck rails over the ACPM regmap; DT `regulators/ldo33`(vdd_aldo1)/`ldo34`(vdd_aldo2) apply via `.of_match`+`.regulators_node="regulators"`+`device_set_of_node_from_dev`. Result: `vdd_aldo1`=1.8 V, `vdd_aldo2`=2.9 V enabled → **sma1303 reads `chip version 0x11`**.

---

## 3. Files Changed

- `drivers/abox/abox.c` — real-PA DRAM mapping + hold-cores-in-reset + DIAG seeds + **`BUG_ON`→ratelimit in `abox_process_ipc` (RC#8)** + **`boot_retry_work` DSP boot retry (RC#10/boot flakiness)**.
- `drivers/abox/abox.h` — `boot_retry_work` + `boot_retry_count` fields.
- `drivers/abox/abox_dma.c` — NULL guard in `abox_dma_hw_params_set`.
- `drivers/abox/abox_dump.c` — dummy platform for dump-card links + **skip dump ASoC component/dai_link (RC#9)**.
- `drivers/abox/s2mpu12-regulator.c` — **NEW**: 29-rail PMIC regulator via ACPM regmap (RC#11).
- `drivers/abox/sma1303.c` — vendor 4.19→7.3 port + `playback_init` sysfs + sysfs_remove_group fix.
- `drivers/abox/abox_ion.c` — **`dma_get_sgtable()` for PCM sg_table (RC#12 — the cicada-noise fix)**.
- `drivers/abox/abox_if.c` — `abox_if_select_pinctrl()` → `aud-i2s1-pins` during playback.
- `drivers/abox/aud3004x.c` — codec probe/register (+217 lines).
- `drivers/abox/exynos3830_aud3004x.c` — cpu_dai-by-name BE binding (`d8315e1`); `.dai_fmt` on UAIF0..2 + aud3004x codec link on UAIF0.
- `patches/linux/0003-a21s-abox-audio-dts.patch` — `abox_tplg:` label + `samsung,aux`.
- `arch/arm64/boot/dts/exynos/exynos850-a217f.dts` — `i2c_3`/`usi_hsi2c_2` + `sma1303@1e` node.

---

## 4. Debugging Toolkit (reusable)

- `0.abox-debug` debugfs: `calliope_sram`, `calliope_dram`, `gpr` (per-core regs), `gicd`, `calliope_state`.
- SRAM landmarks: `0x23000` = `"init"` (`0x696e6974`), `0xc170` = crash-dump pointer var, `0xd000` = captured abort frame, `0xbf00` = marker scratch, `0x23e00` = fw bootargs, `0x23800` = handshake area.
- `abox_diag.ko` (in `/tmp/abox_diag`): `probe`/`sram`/`dram`/`cycle`/`all` over `/sys/kernel/reload` — memremap physical probes + PMU power cycle. **Warning: power-cycling the DSP while the system is live wedges the device** (ping alive, userspace dead) — only use right after boot failures, not after a good boot.
- `speaker-test` device is `plughw:1,<100+n>` — calliope PCMs start at device **100**, not 0.

---

## 5. Immediate Next Steps

**Audio hardware path: DONE** — main speaker + earpiece verified. Remaining audio work is userspace/integration, not bring-up.

1. ~~All kernel bring-up~~ — DONE (see RC#1–#12 + `a21s-audio-abox-plan.md` §0 bug table).
2. **Userspace audio integration:** unmask PipeWire/WirePlumber now that the card is stable; verify `wpctl status` sees card 1; set default sink to `Exynos3830-Madera` speaker path; test `paplay`/Steam audio. **Caveat:** PipeWire opens capture streams that can exhaust the ~768 KiB DSP workspace → keep an eye on `Not enough memory` aborts; may need to disable capture PCMs or reduce buffer sizes.
3. **Robustness caveats (from Claude's verified run):**
   - Some boots Calliope misses `BOOT_DONE` → `calliope boot retry 1/8..8/8` → needs another reboot. `boot_retry_work` helps but isn't 100%.
   - **Never `rmmod exynos3830_aud3004x`** — `aud3004x_jack_remove` oopses on unload. Reboot instead.
   - `s2mpu12-regulator.ko` caused a ~31 s black screen after boot (boot-on rail handling) — see `a21s-mainline-boot.md`. Kept-`always_on` fix is in; verify on next flash.
   - After a DSP runtime `data abort`, Calliope does NOT self-recover — reboot (or re-trigger enable) before retrying playback.
   - RDMA close prints `close timeout` — cosmetic; stream still works.
4. **Finalize:** remove `/etc/modprobe.d/blacklist-abox.conf`; ensure `audio-up.sh` loads `aud3004x-codec` too; sync `configs/a21s_linux_defconfig`; consider an audio-up systemd unit so `aplay`/PipeWire work without manual module loads.

---

## 6. Build / Deploy / Flash Reference

```bash
# Kernel + DTBs
make -C src/linux O=../../build/linux ARCH=arm64 LLVM=1 -j$(nproc) Image dtbs
# ABOX modules
make -C src/linux O=../../build/linux ARCH=arm64 LLVM=1 M=$PWD/drivers/abox modules
# Deploy module (never overwrite /lib!)
scp drivers/abox/abox-snd.ko root@172.16.42.1:/usr/lib/modules/$(uname -r)/extra/

# Repack + flash boot-avb.img (ALWAYS refresh blob Image+dtb first)
cp build/linux/arch/arm64/boot/Image src/uniLoader/blob/Image
cp build/linux/arch/arm64/boot/dts/exynos/exynos850-a217f.dtb src/uniLoader/blob/dtb
make -C src/uniLoader ARCH=aarch64 LLVM=1 -j$(nproc)
mkbootimg --kernel src/uniLoader/uniLoader --ramdisk src/uniLoader/blob/ramdisk \
  --dtb src/tools/AIK-Linux/split_img/boot.img-dtb --dtb_offset 0x00000000 \
  --header_version 2 --pagesize 2048 --base 0x10000000 \
  --kernel_offset 0x00008000 --ramdisk_offset 0x01000000 --tags_offset 0x00000100 \
  --cmdline "$(cat src/tools/AIK-Linux/split_img/boot.img-cmdline)" \
  --os_version "$(cat src/tools/AIK-Linux/split_img/boot.img-os_version)" \
  --os_patch_level "$(cat src/tools/AIK-Linux/split_img/boot.img-os_patch_level)" \
  -o build/images/boot-linux.img
cp build/images/boot-linux.img build/images/boot-avb.img
python3 ./build/avbtool.py add_hash_footer --image build/images/boot-avb.img \
  --partition_size 37748736 --partition_name boot --rollback_index 12
# On-device: backup + dd
#   dd if=/dev/mmcblk0p18 of=/root/p18-backup.img bs=4M
#   dd if=/root/boot-avb-new.img of=/dev/mmcblk0p18 bs=4M && sync && reboot
```

- **kexec caveat:** `kexec -l Image --dtb=dtb --reuse-cmdline` is unreliable here (hung with no `--initrd`); prefer the flash path.
- **Device wedge signature:** ping OK, sshd dead → hard reset (`Power+VolDown` 10–15 s).
