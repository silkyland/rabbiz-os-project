# samsung/a21s (SM-A217F, Exynos 850) — agent rules

Device-specific ground rules live here; OS-wide rules (branding, lanes, evidence
levels, secrets, shell seams) stay in the repository root
[AGENTS.md](../../../AGENTS.md). Each rule below cites the finding or doc it was
learned from; treat uncited additions as hypotheses until they earn a finding.

- Boot/recovery: [devices/samsung/a21s/docs/a21s-boot-recovery.md](docs/a21s-boot-recovery.md)
- Device queue (shared hardware): [docs/DEVICE-QUEUE.md](../../../docs/DEVICE-QUEUE.md)
- Hardware status: [device.toml](device.toml)

## Ground rules

- **Never overwrite `/lib` on the rootfs** — `/lib` is a symlink to `usr/lib` on
  Arch Linux ARM; `scp -r`/`tar` extracting a `/lib/` directory breaks boot.
  Deploy modules to `/usr/lib/modules/$(uname -r)/`.
- **Keep `usb0` (`172.16.42.1/24`) unmanaged in NetworkManager** —
  `systemd-networkd` owns `usb0`; NetworkManager manages `wlan0` with
  `wifi.scan-rand-mac-address=no`
  ([a21s-wifi-bluetooth.md](docs/a21s-wifi-bluetooth.md)).
- **Keep `clk_ignore_unused` in `exynos850-a217f.dts` `chosen/bootargs`** —
  `clk_disable_unused()` disables a WLBT/APM clock gate, causing an immediate SoC
  reset on `SM_MSG_START_REQ` ([a21s-wifi-bluetooth.md](docs/a21s-wifi-bluetooth.md)).
- **Keep `wifibt_if` at `0xb0000000` (`0xc00000`)** in `exynos850-a217f.dts` —
  avoids colliding with kernel/initramfs memory
  ([a21s-wifi-bluetooth.md](docs/a21s-wifi-bluetooth.md)).
- **Uncompressed `uniLoader` + `--dtb` packing** — pack `build/images/boot-avb.img`
  using `mkbootimg` with `--dtb src/tools/AIK-Linux/split_img/boot.img-dtb
  --dtb_offset 0x00000000` and uncompressed `src/uniLoader/uniLoader`.
- **Keep boot-on S2MPU12 rails `always_on`** in
  `socs/exynos850/drivers/abox/s2mpu12-regulator.c` until panel supplies are wired
  in DTS — otherwise `regulator_late_cleanup()` disables `vdd_ldo28` (lcd_vddi)
  at ~31 s and the screen goes black ([a21s-mainline-boot.md](docs/a21s-mainline-boot.md)).
- **Never `rmmod exynos3830_aud3004x`** — `aud3004x_jack_remove` oopses; reboot
  instead ([a21s-audio-abox-plan.md](docs/a21s-audio-abox-plan.md) §0).
- **Probe `i2c-0`/`0x3d` (S2MU106) with plain `write+read`, not userspace SMBus
  ioctl** — `I2C_SMBUS` byte reads return unstable garbage; kernel `i2c_smbus_*`
  is fine ([HANDOFF.md](docs/HANDOFF.md) §0.7).
- **Charge path lives in `s2mu106-charger.ko`, not DT** — instantiated via
  `i2c-0/new_device` by `s2mu106-charger.service`/`scripts/charger-up.sh`; the
  chip boots in BUCK mode so `CTRL0` must be set to `CHG` every boot. A real
  MFD/MUIC/PDIC port is still open ([HANDOFF.md](docs/HANDOFF.md) §0.7).
- **Never hardcode `i2c-N` adapter paths** — enabling/disabling any I2C bus in
  DTS renumbers adapters (fuel-gauge `i2c@13830000` moved SMA1303 from `0-001e`
  to `1-001e` on kernel #12 → silent amp). Glob `/sys/bus/i2c/drivers/<drv>/*/`
  instead; card presence does NOT prove `playback_init` ran
  ([HANDOFF.md](docs/HANDOFF.md) §0.6).
- **Keep `BUFFER_BYTES_MAX = SZ_256K` and WirePlumber's Pro Audio probe disabled
  for ABOX** — Calliope's shared PCM heap is 768 KiB (`786432` B). WirePlumber
  previously requested 1 MiB during probing, which wedged the DSP
  ([HANDOFF.md](docs/HANDOFF.md) §0.3).
- **Never extract archives `-C /` without `--no-same-owner`** — `tar`/`deb`
  extraction re-applies archive uids to `/` and `/home`, transiently breaking
  `runuser`/`su` for `rabbiz`. `deploy_shell.sh` re-chowns `/`, `/home`,
  `/home/rabbiz` after every push.
- **Every device mutation goes through `scripts/device_session.sh`** (remote
  `/run/lock/a21s-device-test.lock`; [DEVICE-QUEUE.md](../../../docs/DEVICE-QUEUE.md)).
- **Test new kernels with `kexec` before flashing** — `--initrd` with the
  ramdisk extracted from `boot-avb.img` is mandatory (loop-mounted rootfs);
  recipe in [docs/BUILD.md](../../../docs/BUILD.md). Persistent `boot-avb.img`
  flashes are owner-approved only; foreign-built kernels have black-screened on
  kexec ([findings/2026-11-18-kexec-container-kernel-black.md](findings/2026-11-18-kexec-container-kernel-black.md)).
- **Session user is `rabbiz` (uid 1001)** — `gamer` is gone. Hardware-facing
  files are `video`-group writable by udev where the DE needs them
  (`90-a21s-backlight.rules`: backlight + cpufreq governor).

## Power/display state (2026-11-18, kernel #12)

- Governor is `performance`, cpuidle has zero registered states, and
  `/sys/class/thermal/` is **empty** — no thermal protection at all until
  patch 0009 boots ([finding](findings/2026-11-18-thermal-cpuidle-cpufreq-state.md)).
- IIO sensor nodes (`a21s-accel`/`a21s-gyro`/`a21s-light`) return `-ENODATA`
  until a real sample arrives — a failed read means "no data", never 0 lux
  ([finding](findings/2026-11-18-als-zero-chub-scan.md)).
- Fuel-gauge `*_avg` properties are bogus on S2MU106 (MONOUT path); only
  `voltage_now`/`current_now`/`capacity` are real
  ([finding](findings/2026-11-18-fuel-gauge-s2mu106-monout-bogus.md)).

## Session/render rules (all lanes touching the DE)

- **Shell-launched Qt Quick apps stay software via `Appearance.appEnvironment`
  (`RABBIZ_FORCE_SOFTWARE=1`); keep `QT_QUICK_BACKEND=software` as the session
  default** — direct GPU diagnostics may use `a21s-qt-render` (GPU while
  CMA ≥ 96 MiB, else software) — labwc must run with `dumb-destroy.so`
  preloaded (fixes the CMA leak, [FINDINGS-hw-render.md](docs/FINDINGS-hw-render.md)
  F2) — without it every GPU app leaks ~14 MiB and exhausting CMA segfaults the
  shell. Original reason: stock Mesa buffers fail exynos-drm import. The shell
  entry point `rabbiz-shell` uses `a21s-qt-gpu` with a software fallback; app
  launches must use `Appearance.appEnvironment` to strip GPU overrides.
  Explicit `a21s-qt-gpu <command>` is a verified experimental exception:
  private, checksum-pinned Mesa SCANOUT allocation + primary-node shim enables
  Panfrost/MultiEffect ([a21s-qt-gpu-scanout.md](docs/a21s-qt-gpu-scanout.md)).
  Do not enable GPU globally or remove its version guard. Chrome still needs
  `--disable-gpu`; QtWidgets (Dolphin) is unaffected.
- **Dolphin's phone UI needs `PLASMA_PLATFORM=phone:handset` AND a first-run
  state wipe** — the phone layout only applies when `dolphinrc [General]
  Version < 200` and no `~/.local/state/dolphinstaterc` exists (KDE bug
  505864). `rabbiz-files` does the one-time wipe; `QT_QPA_PLATFORMTHEME=kde` +
  `plasma-integration` + seeded `kdeglobals` give Breeze dark/single-click.
- **Only one on-screen keyboard per seat** — `rabbiz-osk` (Rust,
  `tools/rabbiz-osk/`) is the session keyboard; squeekboard is the installed
  fallback. Never run both. `rabbiz-osk-switch start|stop|status` flips
  between them; autostart prefers `rabbiz-osk`, falls back to squeekboard.
- **Keep `QT_IM_MODULE`/`GTK_IM_MODULE` unset** — Qt6 ≥6.7/GTK apps auto-use
  `zwp_text_input_v3` under labwc; squeekboard (`zwlr_input_method_v2`) then
  auto-shows on text focus once `org.gnome.desktop.a11y.applications
  screen-keyboard-enabled=true`. Thai layout `th` ships built-in;
  maliit/plasma-keyboard/ibus are `input-method-v1` → dead on labwc.

## Common wrong assumptions (device)

- **"Vendor `scsc_bt` compiles directly on Mainline 7.3"** is wrong: Mainline
  removed `hdev->dev_type = HCI_BREDR` and uses
  `<net/bluetooth/hci_h4.h>` `h4_recv_skb(hdev, NULL, NULL, ...)`
  (`socs/exynos850/drivers/scsc/bt/scsc_bluez.c`).
- **"Vendor `mxman.c` unregisters cleanly without `recovery_disabled_unreg`
  check"** is wrong: `mxman_stop()` must guard
  `if (mif->recovery_disabled_unreg)`
  (`socs/exynos850/drivers/scsc/misc/mxman.c`).
- **"ABOX noise means wrong I2S format/clock"** can be wrong: cicada/mosquito
  noise on *both* speakers was a broken `sg_table` (no `struct page`) in
  `socs/exynos850/drivers/abox/abox_ion.c` — build it with `dma_get_sgtable()`
  ([a21s-audio-abox-plan.md](docs/a21s-audio-abox-plan.md) §0).
- **"The 18 MiB `ABOX_DRAM_SIZE=0x1200000` is the DSP's audio buffer space"** is
  wrong: that is the **host-side** DRAM window (`DRAM_FIRMWARE_SIZE`). The
  firmware's PCM buffer heap is only **768 KiB** — and hitting it does not
  return a clean error, it data-aborts the DSP.
- **"Use `wpa_supplicant` + `systemd-networkd` for Steam Wi-Fi"** is wrong:
  `steam -gamepadui` requires `org.freedesktop.NetworkManager` over D-Bus
  ([a21s-wifi-bluetooth.md](docs/a21s-wifi-bluetooth.md)).
