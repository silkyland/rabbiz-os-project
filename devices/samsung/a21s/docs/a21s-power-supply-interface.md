# A21s battery/charging interface for the DE status bar

Kernel lane contract. Verified on `7.3.0-g14a425b3a7dc-dirty` (#12).
upowerd is installed and running (`org.freedesktop.UPower` on the system
bus).

## Preferred: read upower (D-Bus)

| What | Object | Property |
|---|---|---|
| Charger attached | `/org/freedesktop/UPower/devices/line_power_s2mu106_charger` | `Online` (bool) |
| Battery | `/org/freedesktop/UPower/devices/battery_s2mu005_fuel_gauge`* | `Percentage`, `State` |
| Combined (what a status bar should show) | `/org/freedesktop/UPower/devices/DisplayDevice` | `Percentage`, `State`, `IconName` |

\* Renames to `battery_s2mu106_fuel_gauge` once a kernel carrying the
S2MU106 variant descriptor (patch 0006, upstream backport) is flashed —
resolve the object via `upower -e` / `EnumerateDevices`, never hardcode
the path.

## Sysfs fallback

| File | Contents |
|---|---|
| `/sys/class/power_supply/s2mu106-charger/online` | `1` = VBUS good |
| `…/s2mu106-charger/status` | `Charging`/`Discharging`/`Full` |
| `…/s2mu106-charger/charge_type` | `Fast`/`Unknown` (state machine, not cable class) |
| `…/s2mu005-fuel-gauge/capacity` | SOC % |
| `…/s2mu005-fuel-gauge/voltage_now` | µV |
| `…/s2mu005-fuel-gauge/current_now` | µA, positive = charging |

## Caveats (kernel #12 and the pending backport)

- **Do not read `voltage_avg`/`current_avg`** on kernel #12 — fixed bogus
  values (MONOUT path absent on S2MU106). They disappear after the next
  kernel flash; see `findings/2026-11-18-fuel-gauge-s2mu106-monout-bogus.md`.
- **`status` on the fuel gauge flickers** near zero current (±5 mA noise
  at DONE). For the status bar, trust charger `online` + `capacity`;
  treat FG `status` as advisory.
- **No battery temperature property exists** — treat as unavailable.
- `online` transitions reach `power_supply_changed` within ~1 s (1 s
  driver poll); upower follows on the next uevent.
