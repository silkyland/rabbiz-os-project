# A21s ET520 SPI identification

The r06 vendor overlay names ET520 and uses `VDD_BTP_3P3` (S2MPU12
LDO26), reset/sleep `gpg1-1`, DRDY `gpa0-5` and chip select `gpp2-1`.
The vendor base DT maps its SPI0 controller to `0x13940000`. The vendor
non-secure driver uses SPI mode 0, 8 bits and 12.5 MHz. It enables power
and pinctrl, waits about 1.6 ms, drives reset high and waits about 12 ms,
then waits another 20 ms before checking status. A standalone reset drives
the line low for about 1.05 ms before releasing it.
[Vendor board overlay](https://github.com/samsungexynos850/android_kernel_samsung_exynos850/blob/b8c71b7aecece9c67f0e840a6b074573e8041d20/arch/arm64/boot/dts/samsung/a21s/a21s_eur_open_w00_r06.dts)
· [Vendor SPI and power code](https://github.com/samsungexynos850/android_kernel_samsung_exynos850/blob/b8c71b7aecece9c67f0e840a6b074573e8041d20/drivers/fingerprint/et5xx-spi.c)

Each single-register read clocks out `20 <address> 00` in one transfer
and consumes RX byte 2. Register `00` must return `aa` before checking
`fd fe ff`. The expected ET520 reply is `03 14 05`. No interrupt is
required for this ID check. The A217FXXU8DVF6 stock kernel config enables
secure fingerprint support, so the direct SPI transaction comes from
the vendor driver's non-secure path. Access from the native Linux boot
still needs testing.
[Register read implementation](https://github.com/samsungexynos850/android_kernel_samsung_exynos850/blob/b8c71b7aecece9c67f0e840a6b074573e8041d20/drivers/fingerprint/et5xx-spi_data_transfer.c)
· [Stock kernel config](https://github.com/leddaz-dump-stash/samsung_a21s_dump/blob/a21snsxx-user-12-SP1A.210812.016-A217FXXU8DVF6-release-keys/ikconfig#L5110-L5120)

## Hardware checkpoint

The current A21s boot (`7.3.0-g14a425b3a7dc-dirty`) exposes only
`spi0.0`, bound to the TD4150 panel at controller `0x11d00000`.
The ET520 controller was not enumerated. Do not use that bus number for
ET520. Confirm the controller path and chip select in sysfs after
enabling the correct DT node and arranging the vendor-equivalent
power/reset sequence. Use the shared device lock.

Build [the ID utility](../../../../tools/a21s-et520-id-probe/et520-id-probe.c)
with the target Linux C compiler. Run it only on the verified ET520 spidev
node with the sensor powered and out of reset. It checks SPI mode, reads
status, reads the three ID registers and exits zero only for ET520. It
does not change power, pinctrl or SPI mode and has no biometric capture code.
