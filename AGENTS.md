# AGENTS.md

Native Mainline Linux (`7.3.0`) + Arch Linux ARM64 + SteamOS (`steam -gamepadui`) port for Samsung Galaxy A21s (`SM-A217F`, Exynos 850) and planned Phase 2 Xiaomi Pad 7 Pro (`muyu`). This is **bare-metal Native Linux**, NOT Android/Termux/proot (`docs/PLAN.md`).

## Ground rules

- **Never overwrite `/lib` on the A21s rootfs** — `/lib` is a symlink to `usr/lib` on Arch Linux ARM; `scp -r` or `tar` extracting a `/lib/` directory breaks boot. Always deploy modules to `/usr/lib/modules/$(uname -r)/`.
- **Keep `usb0` (`172.16.42.1/24`) unmanaged in NetworkManager** — `systemd-networkd` owns `usb0` (`docs/a21s-wifi-bluetooth.md`); NetworkManager manages `wlan0` with `wifi.scan-rand-mac-address=no`.
- **Never leak secrets** — `~/.credentail` on the host holds `WIFI_PASSWORD`; never print or commit it.
- **Keep `clk_ignore_unused` in `exynos850-a217f.dts` `chosen/bootargs`** — `clk_disable_unused()` disables a WLBT/APM clock gate, causing an immediate SoC reset on `SM_MSG_START_REQ` (`docs/a21s-wifi-bluetooth.md`).
- **Keep `wifibt_if` at `0xb0000000` (`0xc00000`)** in `exynos850-a217f.dts` — avoids colliding with kernel/initramfs memory (`docs/a21s-wifi-bluetooth.md`).
- **Uncompressed `uniLoader` + `--dtb` packing** — pack `build/images/boot-avb.img` using `mkbootimg` with `--dtb src/tools/AIK-Linux/split_img/boot.img-dtb --dtb_offset 0x00000000` and uncompressed `src/uniLoader/uniLoader`.
- **`src/` and `build/` are gitignored** (`.gitignore:2-3`) — tracked driver code lives in `drivers/scsc/`.
- **Keep boot-on S2MPU12 rails `always_on`** in `drivers/abox/s2mpu12-regulator.c` until panel supplies are wired in DTS — otherwise `regulator_late_cleanup()` disables `vdd_ldo28` (lcd_vddi) at ~31 s and the screen goes black (`docs/a21s-mainline-boot.md`).
- **Never `rmmod exynos3830_aud3004x`** — `aud3004x_jack_remove` oopses; reboot instead (`docs/a21s-audio-abox-plan.md` §0).
- **Never let a client request ≥768 KiB on a `calliope_*` PCM** — the Calliope buffer heap is 768 KiB (`786432` B) while ALSA advertises `BUFFER_BYTES_MAX = SZ_1M` (`drivers/abox/abox.h:41`). The oversized request is refused *inside* the DSP (`Not enough memory: 1048576 of 786432`) and wedges it. WirePlumber does exactly this when it probes calliope PCMs with ALSA defaults (`docs/HANDOFF.md` §0.1).

## Common wrong assumptions

- **"Use Termux / proot / Android underneath"** is wrong: `R1` pivoted to bootloader replacement via `uniLoader` (`docs/PLAN.md:51`).
- **"Use `wpa_supplicant` + `systemd-networkd` for Steam Wi-Fi"** is wrong: Steam `-gamepadui` requires `org.freedesktop.NetworkManager` over D-Bus (`docs/a21s-wifi-bluetooth.md`).
- **"Vendor `scsc_bt` compiles directly on Mainline 7.3"** is wrong: Mainline removed `hdev->dev_type = HCI_BREDR` and uses `<net/bluetooth/hci_h4.h>` `h4_recv_skb(hdev, NULL, NULL, ...)` (`drivers/scsc/bt/scsc_bluez.c`).
- **"Vendor `mxman.c` unregisters cleanly without `recovery_disabled_unreg` check"** is wrong: `mxman_stop()` must guard `if (mif->recovery_disabled_unreg)` (`drivers/scsc/misc/mxman.c:2667`).
- **"ABOX noise means wrong I2S format/clock"** can be wrong: cicada/mosquito noise on *both* speakers was a broken `sg_table` (no `struct page`) in the `drivers/abox/abox_ion.c` ION shim — build it with `dma_get_sgtable()` (`docs/a21s-audio-abox-plan.md` §0).
- **"The 18 MiB `ABOX_DRAM_SIZE=0x1200000` is the DSP's audio buffer space"** is wrong: that is the **host-side** DRAM window (`DRAM_FIRMWARE_SIZE`). The firmware's PCM buffer heap is only **768 KiB** — and hitting it does not return a clean error, it data-aborts the DSP.

## Commands

- Bootstrap `src/` + `build/` on a fresh machine:
  `./scripts/bootstrap_build_env.sh` (clones `samsungexynos850/linux` @ `14a425b3a` + `uniLoader` @ `06a1bba`, applies `patches/linux/*.patch`, prepares `build/linux` from `configs/a21s_linux_defconfig`)
- Build SCSC modules (`drivers/scsc/`):
  `make -C src/linux O=../../build/linux ARCH=arm64 LLVM=1 M=$PWD/drivers/scsc modules`
- Build kernel (`Image` + DTBs):
  `make -C src/linux O=../../build/linux ARCH=arm64 LLVM=1 -j$(nproc) Image dtbs`
- Verify device over USB or Wi-Fi:
  `ssh root@172.16.42.1 "uname -a && nmcli device status && hciconfig -a"` (or `ssh root@192.168.1.142`)

## Where knowledge lives

- Build & patch guide: [docs/BUILD.md](docs/BUILD.md)
- Architecture and data flow: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- Current milestone and what's next: [ROADMAP.md](ROADMAP.md)
- Full research and decisions record: [docs/PLAN.md](docs/PLAN.md)
- Wi-Fi & Bluetooth bring-up details: [docs/a21s-wifi-bluetooth.md](docs/a21s-wifi-bluetooth.md)
- Battery, power & modem plan: [docs/a21s-power-battery-modem-plan.md](docs/a21s-power-battery-modem-plan.md)
