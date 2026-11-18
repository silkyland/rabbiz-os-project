---
status: reproduced
lane: kernel
device: samsung/a21s
kernel: 7.3.0-g14a425b3a7dc-dirty
author: Codex
ai_assisted: true
date: 2026-11-18
evidence:
  - cmd: 'journalctl -k -b -o cat --no-pager | rg "^a21s-chub:"'
  - file: devices/samsung/a21s/findings/logs/2026-11-18-als-zero-chub-scan.txt
supersedes: []
---

# The A21s sensor hub scan omits the ambient-light sensor

## Observed

IIO illuminance, IIO raw and SensorProxy `LightLevel` remained zero in a 35-sample
baseline and a 22-sample follow-up started after the owner confirmed they were shining
a flashlight on the sensor area. The probe held `ClaimLight` during both intervals.
The accelerometer Z axis also stayed zero. The regulator framework reported
`vdd_sensor_1p8 state=enabled microvolts=1800000`. The backlight read 153/255.
[Device samples](logs/2026-11-18-als-zero-chub-scan.txt)

CHUB reported `ALIVE` and returned a 24-byte `SENSOR_SCAN_RESULT`. The first eight
bytes decode to `0x00000018024e0000`, with bits 17, 18, 19, 22, 25, 35 and 36 set.
The vendor driver interprets that first little-endian `u64` as the legacy sensor bitmap.
Accelerometer (1), gyroscope (4), and ambient light (5) are absent from the bitmap.
[Vendor bitmap decoding](https://github.com/samsungexynos850/android_kernel_samsung_exynos850/blob/b8c71b7aecece9c67f0e840a6b074573e8041d20/drivers/sensorhub/ssp_data.c#L360-L385)
[Vendor sensor IDs](https://github.com/samsungexynos850/android_kernel_samsung_exynos850/blob/b8c71b7aecece9c67f0e840a6b074573e8041d20/drivers/sensorhub/ssp_type_define.h#L1-L8)

The stock Android 12 dump for SM-A217F build `A217FXXU8DVF6` shows LSM6DSL
enabled for both accelerometer and gyroscope in the extracted kernel config.
This establishes the stock driver configuration for that build.
[Stock firmware dump](https://github.com/leddaz-dump-stash/samsung_a21s_dump)
· [Extracted kernel config](https://github.com/leddaz-dump-stash/samsung_a21s_dump/blob/a21snsxx-user-12-SP1A.210812.016-A217FXXU8DVF6-release-keys/ikconfig#L5160-L5169)

## Interpretation

SensorProxy is passing through the zero already present in Linux IIO. The port allocates
its cache with `kzalloc`, registers an IIO light node regardless of the scan result, and
returns the cached lux field even before `light_valid` is set by a parsed sample. That
explains why a present IIO node and `HasAmbientLight=true` can still report 0 lux. The
missing scan bits point to hardware detection or CHUB initialization. The exact cause of
the failed detection remains unverified. The regulator status establishes what Linux
reports for one rail. Automatic brightness should wait for a changing lux reading.
[A21s CHUB driver](../../../../socs/exynos850/drivers/sensorhub/a21s_chub.c)

## Reproduce

Run the probe under the device lock while illuminating the front sensor. A responding
sensor should produce a positive reading during the light step.

```sh
./scripts/device_session.sh <<'REMOTE'
set -eu
for dev in /sys/bus/iio/devices/iio:device*; do
  if [ "$(cat "$dev/name")" = a21s-light ]; then
    sensor="$dev/in_illuminance_input"
    break
  fi
done
test -n "${sensor:-}"
for step in $(seq 1 10); do
  lux=$(cat "$sensor")
  printf '%s\n' "$lux"
  [ "$lux" -eq 0 ] || exit 0
  sleep 1
done
exit 1
REMOTE
```

## Update — honest no-data reads (2026-11-18, commit 3283e41)

`read_raw` now returns `-ENODATA` for channels that have not received a
real SSP sample, instead of reporting a fake `0`. The IIO nodes stay
registered, so they start answering the moment CHUB actually reports the
sensor. Consumers (the DE auto-brightness path) must treat a failed read
as "no sensor data", not as 0 lux. Status unchanged: the missing scan
bits are still unexplained; this change only removes the misleading
zero.
