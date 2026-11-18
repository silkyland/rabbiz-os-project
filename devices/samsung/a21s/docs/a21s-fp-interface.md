# A21s fingerprint interface (for the DE lock screen)

**Status: provisional** — driver probes and the sysfs ABI exists, but
the chip is not yet verified (the ID probe in
`tools/a21s-et520-id-probe/` has not run on device). Do not build
enrollment/unlock on this until the probe finding lands.

## Driver

`socs/exynos850/drivers/fingerprint/a21s_et5xx.c` (EgisTech ET520),
GPIO/IRQ-based: exposes `/dev/esfp0` (poll/read/write/ioctl) and a
debug sysfs group.

## ABI the lock screen may read

| Path | Contents | Notes |
|---|---|---|
| `/sys/kernel/a21s_fp/chipid` | chip ID string | currently returns a **fixed string** — do not trust as hardware proof until the probe verifies a real register read |
| `/sys/kernel/a21s_fp/status` | driver state | bring-up diagnostic |
| `/sys/kernel/a21s_fp/irq_count` | interrupt count | proof the IRQ line is wired |
| `/dev/esfp0` | vendor chardev | poll = finger event; read/write/ioctl = vendor protocol |

The `/sys/kernel/a21s_fp/` names are a **stable contract** — the kernel
lane will not rename them; if the ABI grows (enrollment state, capture
readiness) it grows by adding files, not renaming.

## Open before the ABI is real

- ET520 ID probe under the device lock (`a21s-et520-id-probe`): does
  register `0x00` return the expected ID, and what power/reset sequence
  is needed — [probe doc](a21s-et520-id-probe.md).
- SPI controller/DT node for the sensor is not yet bound.
- No image capture, enrollment, or match path exists — `/dev/esfp0`
  protocol work is vendor-ABI territory, unstarted.
