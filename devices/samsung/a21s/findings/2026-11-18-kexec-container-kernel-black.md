---
status: reproduced
lane: kernel
device: samsung/a21s
kernel: 7.3.0-g14a425b3a7dc-dirty
author: devin-kernel-agent
ai_assisted: true
date: 2026-11-18
evidence:
  - cmd: "kexec -l /tmp/Image --initrd=/tmp/ramdisk --dtb=/tmp/exynos850-a217f.dtb --reuse-cmdline && systemctl kexec"
supersedes: []
---

# kexec of a container-built kernel reaches black screen, needs hard power cycle

## Observed

Two `systemctl kexec` attempts on kernel #12 (built on host `dit`,
clang 23.1.5) loading an Image built in `rabbizos-kbuild:usb-investigation`
(clang 19.1.7, from Docker volume `a21s-kbuild`):

1. `kexec -l Image --dtb=… --reuse-cmdline` (no `--initrd`): the new
   kernel never came up; device fell back to the flashed #12 kernel on
   its own. Root is `/dev/loop0p2`, set up by the pmos ramdisk in
   boot-avb.img — a missing `--initrd` cannot mount root.
2. `kexec -l Image --initrd=/tmp/ramdisk --dtb=… --reuse-cmdline` with
   the ramdisk extracted from `/dev/mmcblk0p18` (verified gzip, 1,785,250 B):
   screen went black and stayed black — no display, no usb0, no Wi-Fi,
   ssh unreachable. Recovered only by long-press power cycle → flashed
   #12 kernel booted fine.

## Interpretation

The kexec path itself works (systemd reached "Reboot via kexec"; the
first attempt's early panic correctly fell back). With the correct
ramdisk the second kernel got far enough to kill the display — then
either hung or booted without panel/gadget/Wi-Fi. Whether the cause is
the clang-19-vs-23 build environment, a DTB delta between `/work/out`
and the flashed kernel's dtb, or missing modules for usb0/wlan is
**unknown**. Not proven: that kexec works at all on this device for a
foreign-built kernel.

## Reproduce

Follow `docs/BUILD.md` §B (initrd steps added after this incident) with
an Image built in the kbuild container. The user must be present for a
possible long-press recovery — do not run unattended.
