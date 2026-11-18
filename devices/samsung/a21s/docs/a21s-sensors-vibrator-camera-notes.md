# Galaxy A21s (`SM-A217F`) — Sensors, Vibration, Fingerprint & Camera Notes

Last updated: 2026-11-18

## Summary Status

| Peripheral | Driver / Patch | Current State | What Remains |
|---|---|---|---|
| **Vibration** (`COINDC` ERM) | [`socs/exynos850/drivers/vibrator/a21s_vibrator.c`](../../../../socs/exynos850/drivers/vibrator/a21s_vibrator.c) | Built, deployed to `/usr/lib/modules/$(uname -r)/extra/`, `modprobe` succeeds (`gpg0-0`, `EV_FF` + `/sys/class/leds/vibrator` + `/sys/kernel/a21s_vib/duration_ms`) | Physical feel test on device (`echo 150 > /sys/kernel/a21s_vib/duration_ms`); verify if `vdd_motor_3p3` (`ldo31`) is needed or if `gpg0-0` alone drives the motor |
| **Gyroscope / accelerometer** | [CHUB IIO driver](../../../../socs/exynos850/drivers/sensorhub/a21s_chub.c) | Vendor sources identify ST LSM6DSL. The hub boots and IIO nodes register, but the latest device log lacks gyroscope and accelerometer bits despite the enabled sensor rail. | Confirm CHUB firmware boot and sensor discovery, then verify changing XYZ samples. [Device evidence](../findings/2026-11-18-als-zero-chub-scan.md) |
| **Fingerprint** (EgisTech ET520) | [GPIO driver](../../../../socs/exynos850/drivers/fingerprint/a21s_et5xx.c) | GPIO/IRQ module exposes `/dev/esfp0`. Its `chipid` sysfs entry returns a fixed string. | Verify the powered SPI link with a real chip-ID read before attempting image capture and authentication. |
| **Display Brightness** | `lm36274` (`/sys/class/backlight/`) | Working (`devices/samsung/a21s/docs/a21s-display-brightness.md`) | Auto-brightness integration via `a21s-light` (`iio:device2`) once `vdd_sensor_1p8` is powered |
| **Camera** | Vendor CSI-2 / Pablo ISP, with S5KGM2 rear main, HI1336 front, GC02M1 depth and macro, and S5K4HA or SR846 in the ultrawide slot | The r06 overlay lists both ultrawide candidates with the same logical ID and I²C channel. The fitted variant is unverified. | Bring up one sensor and CSI-2 capture first. Verify the actual I²C address from the vendor control path before adding the sensor node. |
| **Microphone** | [AUD3004X machine driver](../../../../socs/exynos850/drivers/abox/exynos3830_aud3004x.c), ABOX capture | Capture DAIs exist. The [HiFi UCM profile](../../../../rootfs-overlay/usr/share/alsa/ucm2/Exynos3830-Madera/HiFi.conf) declares only Speaker. Physical microphone part numbers and the working capture route remain unverified. | Verify the stock MIC1 route and a short capture before adding a PipeWire source. [Capture probe](a21s-main-mic-capture.md). |
| **GPS / GNSS** | Vendor Kepler `gnssif` | The r06 overlay specifies mailbox MMIO `0x11980000` and `gnss_ipc`. The base DT reserves 6 MiB at `0xee000000`. | Port power/reset, mailbox and shared-memory IPC, then identify the data format and verify the first satellite fix. |

## Vendor source references

- The [A21s r06 overlay](https://github.com/samsungexynos850/android_kernel_samsung_exynos850/blob/b8c71b7aecece9c67f0e840a6b074573e8041d20/arch/arm64/boot/dts/samsung/a21s/a21s_eur_open_w00_r06.dts) and [Exynos3830 base DT](https://github.com/samsungexynos850/android_kernel_samsung_exynos850/blob/b8c71b7aecece9c67f0e840a6b074573e8041d20/arch/arm64/boot/dts/exynos/exynos3830.dts) describe the camera and GNSS wiring.
- Samsung specifies four rear modules at 48, 8, 2 and 2 MP, plus a 13 MP front camera. The two GC02M1 nodes specify 1600×1200 active pixels. The r06 camera node suffixes and `sensor_id` fields should not be used as Linux I²C addresses. [Samsung camera specifications](https://news.samsung.com/uk/samsung-adds-the-new-galaxy-a21s-to-the-galaxy-a-family)
- The Android kernel implements the GNSS endpoint in [`drivers/soc/samsung/gnssif/`](https://github.com/samsungexynos850/android_kernel_samsung_exynos850/tree/b8c71b7aecece9c67f0e840a6b074573e8041d20/drivers/soc/samsung/gnssif). An NMEA stream from that endpoint has not been verified on RabbizOS.
- The [LineageOS A21s common device tree](https://github.com/LineageOS/android_device_samsung_a21s-common/blob/lineage-23.2/proprietary-files.txt#L99-L113) lists an Android vendor `gpsd`, GNSS HAL 2.1 and GNSS configuration. The vendor binary name alone does not establish compatibility with Linux gpsd.

## Next checks on the A21s

Use the shared device queue after the DE test run. Confirm the live DT and
regulator state for `vdd_sensor_1p8` and `vdd_btp_3p3` before planning
another flash. Find the IIO devices by name before testing motion or
illuminance. Check for changing readings before wiring SensorProxy
auto-rotate or auto-brightness.
