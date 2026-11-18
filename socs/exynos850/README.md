# SoC: Exynos 850 (S5E3830)

Drivers shared by every Exynos 850 device live in `drivers/` (ABOX audio, SCSC WLBT Wi-Fi/Bluetooth,
cpif modem, S2MU106 charger, TMU, sensorhub, vibrator, fingerprint, vendor includes). They are
GPL-2.0 / upstream-licensed — see [drivers/LICENSING.md](drivers/LICENSING.md).

Devices using this SoC: [samsung/a21s](../../devices/samsung/a21s/).

Rules for this directory:
- Code here must not assume one device. Anything specific to a board (DTS nodes, regulator
  wiring, amp/codec choice) belongs in `devices/<vendor>/<codename>/kernel/`.
- Some drivers still carry A21s assumptions (`a21s_*` file names). Generalising them is a welcome
  `kernel`-lane contribution, with a finding for each assumption removed.
- Build: `make -C src/linux O=../../build/linux ARCH=arm64 LLVM=1 M=$PWD/socs/exynos850/drivers/scsc modules`
  (see [docs/BUILD.md](../../docs/BUILD.md)).
