# A21s backlight interface (for the DE)

## Sysfs

The panel exposes exactly one backlight, driven by DCS `0x51`
`set_display_brightness` commands over the DSI link:

| File | Access | Semantics |
|---|---|---|
| `/sys/class/backlight/13070000.dsi.0/brightness` | `video` group r/w (udev `90-a21s-backlight.rules`) | 0–255, write to set |
| `…/max_brightness` | r | `255` |
| `…/actual_brightness` | r | last committed level |
| `…/bl_power` | `video` group r/w | `0` unblank … `4` powerdown |
| `…/type` | r | `platform` |

- Range is the panel DCS range, **linear, no curve** — the DTS has no
  `brightness_curve`, so sysfs level N maps to DCS brightness N.
- Brightness `0` = display-brightness 0 via DCS; it does **not** power
  the panel down. `a21s-power-key` already uses `brightness 0` +
  save/restore in `/run/user/1001/a21s-saved-brightness` for
  screen-off; `SystemControl.qml` owns save/restore when the shell is
  up.
- Writes are synchronous DSI command transfers — they **block** if the
  DSI link is wedged (see
  `../findings/2026-11-18-dsi-underrun-link-recovery.md`). A hung
  brightness write is a symptom of the underrun wedge, not a backlight
  bug; the DE should treat a write that takes seconds as "link dead",
  not retry-loop.

## Not yet in the hardware description

- `backlight-gpios` (gpg2-1) and the LM36274 bias/backlight LED driver
  are TODO in `exynos850-a217f.dts` — the panel keeps the bootloader's
  bias configuration. When wired, `bl_power` will gain real power
  meaning.
- Perceived-brightness linearity is unverified — no photometric check
  done; a `brightness_curve` may be needed if mid-range steps look
  non-uniform.

## Ownership

Session user `rabbiz` (group `video`) writes `brightness` directly —
no daemon needed. `a21s-power-key` handles screen-off/on; the DE
brightness slider writes the same file. Nothing else should own this
path.

## Unverified

- Flicker under sustained slider changes — needs eyes on the panel;
  each write is one DCS command so rapid writes queue on
  `mipi_dsi_host_transfer`'s mutex.
- Write latency (~one DSI command round-trip, expected single-digit ms)
  — measured sweep pending a free device-lock window.
