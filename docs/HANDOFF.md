# Engineering Handoff — Galaxy A21s (`SM-A217F` / Exynos 850)

Last updated: 2026-11-15 (post Calliope breakthrough)

---

## 1. Current Verified Device State

- **Kernel:** Mainline Linux `7.3.0-g14a425b3a7dc-dirty` (`LLVM=1` Clang) with `exynos850-a217f.dtb` (`patches/linux/0001`..`0004` applied).
  - `CONFIG_EXYNOS_IOMMU=y` **must** be in the flashed kernel — a stale `src/uniLoader/blob/Image` without it silently breaks the whole ABOX path (`iommu_paging_domain_alloc` → `Unable to alloc iommu domain`). Always refresh `blob/Image` + `blob/dtb` before repacking `boot-avb.img`.
- **Boot & Display:** `uniLoader` (`06a1bba`) + `decon850` + `dsi` (`td4150`) + `panfrost`/`panvk` + `steamos-session.service`.
- **Networking:** `usb0` (`172.16.42.1/24`, systemd-networkd, unmanaged in NM) + `wlan0` (`scsc_wlan`, NetworkManager, `192.168.1.142`).
- **Bluetooth & Gamepads:** `scsc_bt.ko` (`hci0`) + BlueZ 5.88 + PipeWire + HID/UHID.
- **Audio (`abox` — Phase A DONE, Phase B next):**
  - `Calliope is ready to sing (version:nEB0)` — DSP boots + handshake completes reliably on clean boots with pristine firmware.
  - `loaded abox_tplg.bin` — topology loads; card 1 `Exynos3830-Madera` registers **10 playback PCMs (`calliope_0`–`calliope_9`, `hw:1,100`–`hw:1,109`)** + ~29 capture PCMs + 9 WDMA capture.
  - Still missing: physical speaker output (AUD3004X codec path — Phase B); `speaker-test -D plughw:1,100` currently returns `-EINVAL` on open (FE PCM open path under investigation).
  - `/etc/modprobe.d/blacklist-abox.conf` may still blacklist the modules — load manually with `modprobe exynos3830-aud3004x`.

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

---

## 3. Files Changed

- `drivers/abox/abox.c` — real-PA DRAM mapping + hold-cores-in-reset-before-download + DIAG seeds (`sram+0xc170` dump ptr, `sram+0x23e00` bootargs `slog=false vss=false`).
- `drivers/abox/abox_dma.c` — NULL guard in `abox_dma_hw_params_set`.
- `drivers/abox/abox_dump.c` — dummy platform component for dump-card links.
- `patches/linux/0003-a21s-abox-audio-dts.patch` — `abox_tplg:` label + `samsung,aux = <&abox_tplg>;` in `sound` node.

---

## 4. Debugging Toolkit (reusable)

- `0.abox-debug` debugfs: `calliope_sram`, `calliope_dram`, `gpr` (per-core regs), `gicd`, `calliope_state`.
- SRAM landmarks: `0x23000` = `"init"` (`0x696e6974`), `0xc170` = crash-dump pointer var, `0xd000` = captured abort frame, `0xbf00` = marker scratch, `0x23e00` = fw bootargs, `0x23800` = handshake area.
- `abox_diag.ko` (in `/tmp/abox_diag`): `probe`/`sram`/`dram`/`cycle`/`all` over `/sys/kernel/reload` — memremap physical probes + PMU power cycle. **Warning: power-cycling the DSP while the system is live wedges the device** (ping alive, userspace dead) — only use right after boot failures, not after a good boot.
- `speaker-test` device is `plughw:1,<100+n>` — calliope PCMs start at device **100**, not 0.

---

## 5. Immediate Next Steps

1. **Fix FE PCM open `-EINVAL`** — calliope_0..9 exist but `hw:1,100` open fails; likely FE link cpu=snd-soc-dummy needing a real platform/CPU or a missing `dynamic`/`no_pcm` route to the RDMA BE links. Trace `soc_pcm_open` on `hw:1,100`.
2. **Phase B — AUD3004X codec:** `aud3004x-codec.ko` + ACPM (`devm_acpm_get_by_node`) + SPEEDY + `uaif`/`spdy` codec link in the sound node → physical speaker output (`speaker-test -D plughw:1,100`).
3. **Finalize:** remove `/etc/modprobe.d/blacklist-abox.conf`; sync `configs/a21s_linux_defconfig` (already has `CONFIG_EXYNOS_IOMMU=y`); keep `clk_ignore_unused` + `wifibt_if@b0000000`; evaluate abox-slog (`0xe6c00000/0x2800000`) + abox-dbg reserved-mem if DSP logs are needed.

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
