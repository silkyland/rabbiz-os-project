# A21s front HI1336 sensor and CSI-2 checkpoints

The r06 board overlay binds the front HI1336 at **7-bit address
`0x20`**, vendor `hsi2c_0` at `0x138a0000`. The `@D3` name and
`sensor_id = 0xd3` are not the Linux I²C address. The chosen vendor
sensor setfile is `setB`. The separate front EEPROM at `0x51` does
not establish the sensor identity.
[Vendor board overlay](https://github.com/samsungexynos850/android_kernel_samsung_exynos850/blob/b8c71b7aecece9c67f0e840a6b074573e8041d20/arch/arm64/boot/dts/samsung/a21s/a21s_eur_open_w00_r06.dts)

## Power and ID

The vendor sequence holds reset low on `gpa2-4`, enables AVDD, IOVDD
and DVDD in that order, asserts MIPI select on `gpg2-5`, switches pinctrl,
enables MCLK1, releases reset and enables I²C pinctrl. The AVDD, IOVDD and
DVDD supplies are S2MPB03 LDO7 (2.8 V), LDO3 (1.8 V) and LDO2 (1.1 V).
The camera PMIC is at address `0x56` on controller `0x13880000`.
Preserve the board overlay's delays and shutdown order in the port.

After implementing power sequencing and checking that the I²C adapter's
DT node is `0x138a0000`, the isolated read uses a 16-bit register address
and a repeated-start 16-bit read. With the unclaimed sensor powered,
the equivalent of the vendor model-ID read is:

```sh
i2ctransfer -y "$camera_bus_number" w2@0x20 0x07 0x16 r2
```

The data sheet specifies `0x13 0x36` as the expected reply.
[Vendor model-ID read](https://github.com/samsungexynos850/android_kernel_samsung_exynos850/blob/b8c71b7aecece9c67f0e840a6b074573e8041d20/drivers/media/platform/exynos/camera/sensor/module_framework/cis/is-cis-hi1336.c)
· [SK hynix data sheet](https://assets.sourcengine.com/datasheets/2fc38276-12bd-4dd9-97ca-82d8f475bb89.pdf)

## CSI-2 checkpoint

Front capture selects CSIS channel 1 at `0x14410000` and D-PHY at
`0x144a0000`. The vendor mode table tags the image as VC0 RAW10
(`0x2b`). Start with one known sensor mode and match its clock and lane
configuration at both endpoints. First verify CSI frame-start/frame-end
events and CSI errors. Then verify a completed DMA buffer with the
expected geometry and image data before claiming raw capture.

The current A21s boot (`7.3.0-g14a425b3a7dc-dirty`) has no adapter for
`0x138a0000` and no enumerated video4linux device. The first hardware
checkpoint therefore needs the camera DT and power setup. The raw-frame
checkpoint also needs CSIS/DPHY/DMA support. Neither checkpoint has run.
