# Galaxy A21s (`SM-A217F`) — Sensors, Vibration, Fingerprint & Camera Notes

Last updated: 2026-11-17

## Summary Status

| Peripheral | Driver / Patch | Current State | What Remains |
|---|---|---|---|
| **Vibration** (`COINDC` ERM) | [`socs/exynos850/drivers/vibrator/a21s_vibrator.c`](../../../../socs/exynos850/drivers/vibrator/a21s_vibrator.c) | Built, deployed to `/usr/lib/modules/$(uname -r)/extra/`, `modprobe` succeeds (`gpg0-0`, `EV_FF` + `/sys/class/leds/vibrator` + `/sys/kernel/a21s_vib/duration_ms`) | Physical feel test on device (`echo 150 > /sys/kernel/a21s_vib/duration_ms`); verify if `vdd_motor_3p3` (`ldo31`) is needed or if `gpg0-0` alone drives the motor |
| **Gyroscope / Accel / Light (ALS)** | [`socs/exynos850/drivers/sensorhub/a21s_chub.c`](../../../../socs/exynos850/drivers/sensorhub/a21s_chub.c) + [`devices/samsung/a21s/kernel/patches/0008-a21s-sensors-regulators-dts.patch`](../kernel/patches/0008-a21s-sensors-regulators-dts.patch) | CHUB firmware extracted (`shub_nacho_bl.bin`, `shub_nacho_a21s.bin`), CHUB boots (`CHUB ALIVE!`), responds to `SENSOR_SCAN_RESULT` & `0x41` sensor spec, registers `iio:device0` (`a21s-gyro`), `iio:device1` (`a21s-accel`), `iio:device2` (`a21s-light`) | Apply `0008` DTS patch and rebuild/flash `boot-avb.img` so `vdd_sensor_1p8` (`ldo30`) stays on; verify live non-zero IIO readings + hook up `iio-sensor-proxy` for auto-rotate / auto-brightness |
| **Fingerprint** (`ET520`) | [`socs/exynos850/drivers/fingerprint/a21s_et5xx.c`](../../../../socs/exynos850/drivers/fingerprint/a21s_et5xx.c) + `0008` DTS patch | Loaded on device (`/sys/kernel/a21s_fp/chipid` = `ET520`, `status` = `ready`, `/dev/esfp0`) | Rebuild/flash `boot-avb.img` with `0008` so `vdd_btp_3p3` (`ldo26`) stays on; optionally wire `spi_0` (`etspi-spi@0`) for real chip-ID probe / image capture (`libfprint`) |
| **Display Brightness** | `lm36274` (`/sys/class/backlight/`) | Working (`devices/samsung/a21s/docs/a21s-display-brightness.md`) | Auto-brightness integration via `a21s-light` (`iio:device2`) once `vdd_sensor_1p8` is powered |
| **Camera (Front & Back)** | — | Researched only (`S5KGM2` 48MP rear, `HI1336` 13MP front, `GC02M1` 5MP/macro, `S5K4HA` 8MP UW) | Full port needed (XL effort): 902 files in vendor `drivers/media/platform/exynos/camera/`, Samsung Pablo ISP (`exynos_is@14440000`), `is_lib.bin`/`is_rta.bin`, proprietary `is-sock` userspace (no V4L2/mainline/pmOS support) |

## Next Steps (When Ready to Rebuild & Flash `boot-avb.img`)

1. Apply `devices/samsung/a21s/kernel/patches/0008-a21s-sensors-regulators-dts.patch` to `src/linux`:
   ```sh
   git -C src/linux apply ../../patches/linux/0008-a21s-sensors-regulators-dts.patch
   ```
2. Rebuild `Image` + `dtbs`, repack `build/images/boot-avb.img`, and flash.
3. After boot, verify regulators and live IIO sensor values:
   ```sh
   cat /sys/class/regulator/regulator.16/state  # vdd_btp_3p3 -> enabled
   cat /sys/class/regulator/regulator.20/state  # vdd_sensor_1p8 -> enabled
   cat /sys/bus/iio/devices/iio:device{0,1,2}/in_*
   ```
4. Add/verify `iio-sensor-proxy` (`monitor-sensor`) for Wayland/DE auto-rotate and auto-brightness.
