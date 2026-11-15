# Engineering Handoff — Galaxy A21s (`SM-A217F` / Exynos 850)

Last updated: 2026-11-15 (post Calliope breakthrough)

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
