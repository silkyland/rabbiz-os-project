# samsung/a21s (SM-A217F, Exynos 850) — agent rules

The A21s-specific ground rules (`/lib` symlink, `clk_ignore_unused`, `wifibt_if`, S2MPU12
`always_on`, ABOX/Calliope 768 KiB, I2C probing, charger, DE render rules, ...) currently live in
the repository root [AGENTS.md](../../../AGENTS.md) and are being split out into this file as
each rule gets a `verified` finding. Until then, treat every rule in the root file marked with an
A21s path or doc as applying to this device only.

- Boot/recovery: [docs/a21s-boot-recovery.md](../../../docs/a21s-boot-recovery.md)
- Device queue (shared hardware): [docs/DEVICE-QUEUE.md](../../../docs/DEVICE-QUEUE.md)
- Hardware status: [device.toml](device.toml)
