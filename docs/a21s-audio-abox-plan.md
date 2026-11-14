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
