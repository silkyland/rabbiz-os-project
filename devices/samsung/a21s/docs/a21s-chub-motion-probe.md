# A21s CHUB motion probe

The failing scan reply contains legacy bitmap `0x00000018024e0000`.
Bits 1 (accelerometer) and 4 (gyroscope) are clear. CHUB ALIVE and the scan
reply confirm that the AP and MCU can exchange messages. The port registers
its IIO devices independently of the scan, so zero-valued attributes can
come from the initial cache. [Captured device log](../findings/logs/2026-11-18-als-zero-chub-scan.txt)
· [Vendor scan parser](https://github.com/samsungexynos850/android_kernel_samsung_exynos850/blob/b8c71b7aecece9c67f0e840a6b074573e8041d20/drivers/sensorhub/ssp_data.c#L360-L397)

The A217FXXU8DVF6 stock kernel configuration selects **LSM6DSL** for both
gyroscope and accelerometer and enables SSP A21s. It lists
`sensorhub/shub_nacho_bl.bin` as built-in firmware. The r06 overlay selects
`sensorhub/shub_nacho_a21s.bin` for the hub OS, matching the port's requested
filename. Compare the actual firmware hashes and reported revisions before
assuming the images match. The vendor kernel can select a newer signed SPU
image. [Stock dump kernel config](https://github.com/leddaz-dump-stash/samsung_a21s_dump/blob/a21snsxx-user-12-SP1A.210812.016-A217FXXU8DVF6-release-keys/ikconfig)
· [Vendor firmware loader](https://github.com/samsungexynos850/android_kernel_samsung_exynos850/blob/b8c71b7aecece9c67f0e840a6b074573e8041d20/drivers/sensorhub/ssp_firmware.c)

## Isolate the cause

1. **Boot clock contract.** The vendor Exynos3830 driver requests a 360 MHz
   CHUB clock, reads the achieved rate, and writes it into `bootargs.chubclk`
   before releasing reset. The RabbizOS port leaves that field as supplied
   by the bootloader image and does not request the vendor clock. Measure the
   effective clock and bootargs first. A candidate A/B test should supply
   the measured rate to the field while keeping all other settings fixed.
   [Vendor power-on and clock setup](https://github.com/samsungexynos850/android_kernel_samsung_exynos850/blob/b8c71b7aecece9c67f0e840a6b074573e8041d20/drivers/staging/nanohub/chub.c)
   · [Current port](../../../../socs/exynos850/drivers/sensorhub/a21s_chub.c)
2. **Boot timing and command order.** Vendor initializes with a synchronous
   scan before firmware version, sensor axis configuration, AP status, and
   stream restoration. The port sends AP resume and LCD on ahead of its scan
   and immediately queues ADD. Run a scan-only attempt, then compare an
   early and a delayed scan on the same boot. If both are missing the bits,
   try vendor command order on a separate boot. Axis setup and stream
   parsing come later because vendor scans before either is needed.
   [Vendor SSP refresh](https://github.com/samsungexynos850/android_kernel_samsung_exynos850/blob/b8c71b7aecece9c67f0e840a6b074573e8041d20/drivers/sensorhub/ssp_dev.c#L260-L304)
3. **Power, sensor bus and firmware.** The prior failing log reports the
   Linux `vdd_sensor_1p8` regulator enabled at 1.8 V. Confirm the physical
   sensor supply and bus state if the first two comparisons still fail.
   The stock base DT names a separate `vdd_sensor_en` resource. Its use
   and relevance to the fitted IMU remain unverified. Keep firmware and
   power comparisons separate. [Stock base DT](https://github.com/leddaz-dump-stash/samsung_a21s_dump/blob/a21snsxx-user-12-SP1A.210812.016-A217FXXU8DVF6-release-keys/bootdts/01.dtsdump_samsung%2Carmv8.dts)

## Run the regression check

From the repository root on the host, replay the saved failure.

```sh
python3 scripts/check_a21s_chub_motion.py \
  devices/samsung/a21s/findings/logs/2026-11-18-als-zero-chub-scan.txt
```

Expected exit 1 with `FAIL: missing from discovery: accelerometer, gyroscope`.
Exit 2 means the scan reply or required observations were unavailable.

After the DE test session releases the device, capture a boot-specific kernel
log and run the same check against it. Once both bits appear, run the script
on the A21s with `--motion` under the device lock, feeding it the current
boot's log. The script asks for a stationary interval and then a slow rotation
and samples the three raw axes by IIO device name. Review the readings against
the intended movement before relying on them for auto-rotate. No hardware
changes are needed to replay the historical log.

## Clock-only A/B module

Use the same module, kernel, DTB and firmware for both arms. Once the
device is free, collect each arm on a separate cold boot with the normal
module autoload. Set the module option `use_ccf_chubclk=0` for A and
`use_ccf_chubclk=1` for B. Keep the power and firmware setup identical.
Save the kernel log for each boot and run the scan regression check.

The module logs `ccf_hz`, `bootargs_original_hz`, `lookup_rc` and
`bootargs_effective_hz` immediately before continuing the existing boot
sequence. CCF reports the configured APM CHUB bus divider rate, which
can depend on nominal parent-clock rates. It is not a physical frequency
measurement or a read of an internal CHUB peripheral clock. Neither arm
programs the clock controller. B writes only `bootargs.chubclk` and
refuses to release reset if rate lookup fails.

Treat the experiment as inconclusive if A and B report different CCF
rates, if B cannot obtain a rate, or if the original field already equals
the reported rate. If only B advertises both motion sensors, repeat A
and B and then confirm live XYZ motion. If both arms still omit them,
move to the scan timing and command-order comparison.
