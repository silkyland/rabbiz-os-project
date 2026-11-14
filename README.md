# Android ARM Linux — Native Mainline Linux + SteamOS on Galaxy A21s

Porting **Native Mainline Linux (`7.3.0`) + Arch Linux ARM64 + SteamOS (`steam -gamepadui`)** directly onto Android smartphones/tablets (zero Android runtime).

- **Primary Device (Phase 1):** Samsung Galaxy A21s (`SM-A217F`, Exynos 850 / `S5E3830`, 3 GB RAM) — **Booting & Working**
- **Secondary Device (Phase 2):** Xiaomi Pad 7 Pro (`muyu`, `SM8635`) — **Planned**

---

## Hardware Status (`SM-A217F` / Exynos 850)

| Subsystem | Status | Notes |
|---|---|---|
| **Bootloader** | Working | Unlocked S-Boot → `uniLoader` (`a21s_defconfig`) + AVB boot image (`/dev/mmcblk0p18`) |
| **Kernel** | Working | Mainline Linux `7.3.0` (`exynos850-a217f.dts`) |
| **Display & Touch** | Working | `decon` + `dsi` (`syna,td4150-panel`) + `ovt,td4150` SPI touch |
| **GPU (3D / Vulkan)** | Working | Mali-G52 (`panfrost` + Mesa `panvk`, `/dev/dri/renderD128`) |
| **USB Networking** | Working | USB-C RNDIS (`usb0`, `172.16.42.1/24` via `systemd-networkd`) |
| **Wi-Fi (`wlan0`)** | Working | Out-of-tree SCSC WLBT (`scsc_platform_mif`, `scsc_mx`, `scsc_wlan`) + `NetworkManager` |
| **Bluetooth (`hci0`)** | Working | Ported `scsc_bt.ko` (`h4_recv_skb` BlueZ H4 + `rfkill`) |
| **SteamOS UI** | Working | `steamos-session` (Weston landscape + `steam -gamepadui` + Box64/FEX-Emu) |
| **Battery / Charger** | Planned | `s2mu106` Fuel Gauge & Charger ([Plan](docs/a21s-power-battery-modem-plan.md)) |
| **Thermal / Suspend** | Planned | Exynos TMU + Sleep/Wake ([Plan](docs/a21s-power-battery-modem-plan.md)) |
| **Cellular Modem (`cpif`)** | Planned | Exynos `cpif` + `pmu-cp` + `oFono` / Voice Call ([Plan](docs/a21s-power-battery-modem-plan.md)) |

---

## Repository Layout

```text
.
├── drivers/scsc/          # Out-of-tree SCSC WLBT drivers (MIF, MX, WLAN, BT) for Mainline 7.3
├── docs/                  # Bring-up logs, reverse-engineering notes, architecture & plans
│   ├── ARCHITECTURE.md    # System topology, boot/driver/network data flow, ADRs
│   ├── PLAN.md            # Original deep-plan (R1 Native Linux pivot)
│   ├── a21s-mainline-boot.md
│   ├── a21s-native-gpu.md
│   ├── a21s-game-results.md
│   ├── a21s-wifi-bluetooth.md
│   ├── a21s-wlbt-reverse-engineering.md
│   └── a21s-power-battery-modem-plan.md
├── scripts/               # Host probe/helper scripts (legacy Android scripts marked obsolete)
├── AGENTS.md              # Ground rules & verified facts for coding agents
└── ROADMAP.md             # Milestone status and next steps
```

> Note: `src/` (upstream `linux`, `uniLoader`, `exynos850-vendor-kernel`, `AIK-Linux`) and `build/` are gitignored.

---

## Quick Links

- **Architecture & System Map:** [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- **Roadmap & Next Milestones:** [ROADMAP.md](ROADMAP.md)
- **Wi-Fi & Bluetooth Bring-up Guide:** [docs/a21s-wifi-bluetooth.md](docs/a21s-wifi-bluetooth.md)
- **Battery, Power & Modem Plan:** [docs/a21s-power-battery-modem-plan.md](docs/a21s-power-battery-modem-plan.md)
- **Mainline Boot & GPU Results:** [docs/a21s-mainline-boot.md](docs/a21s-mainline-boot.md), [docs/a21s-native-gpu.md](docs/a21s-native-gpu.md)
