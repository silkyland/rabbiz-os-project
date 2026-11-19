---
status: reproduced
lane: kernel
device: samsung/a21s
kernel: 7.3.0-g14a425b3a7dc-dirty
author: Antigravity
ai_assisted: true
date: 2026-11-19
evidence:
  - cmd: "./scripts/abox_main_mic_capture.sh 1 110 /tmp/mic_test_110.wav"
  - file: /sys/fs/pstore/console-ramoops-0
supersedes: []
---

# Main mic capture on `calliope_10` triggers Calliope data abort

## Observed

On kernel `#6` (`CONFIG_EXYNOS_IOMMU=y`, `Exynos3830Mader` `card1` live),
running `scripts/abox_main_mic_capture.sh 1 110 /tmp/mic_test_110.wav`
dropped the SSH/USB session and rebooted the device.

`/sys/fs/pstore/console-ramoops-0` captured the Calliope DSP fault:

```text
[  164.331125] samsung-abox 14a50000.abox: data abort(0x1, 0xdead0001, 0x8000b538, 0x805) is reported from calliope
[  165.257644] samsung-abox-wdma 14a53400.abox-wdma: close timeout
[  165.258027] samsung-abox-vdma samsung-abox-vdma.110: abox_vdma_close[c]
```

## Interpretation

Just as speaker playback required `ASRC3=0` to avoid a Calliope data
abort, the `calliope_10` capture path (`ABOX NSRC4=UAIF0`,
`ABOX SIFM4=WDMA`, `ABOX VPCMIN_DAI0_A=WDMA4`, `ABOX VPCMIN_DAI2_A=WDMA4`)
causes Calliope to data-abort (`0xdead0001` at `pc=0x0000b454` /
`r14=0x8000b538`) and wedge `abox-wdma` (`close timeout`).

## Next

- Check whether `calliope_12` (`pcm 112`) or disabling an ASRC/SIFM/VPCMIN
  route avoids the Calliope data abort on capture.
