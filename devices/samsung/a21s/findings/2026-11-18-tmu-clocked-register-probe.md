---
status: verified
lane: kernel
device: samsung/a21s
kernel: 7.3.0-g14a425b3a7dc-dirty
author: Codex
ai_assisted: true
date: 2026-11-18
evidence:
  - cmd: "make -C /work/out ARCH=arm64 LLVM=1 M=/tmp/tmu modules"
  - cmd: "insmod <temporary-module>; readlink /sys/bus/platform/devices/10070000.tmu/driver"
supersedes: []
---

# The clocked TMU register probe binds to the A21s

## Observed

On the A21s, `10070000.tmu` existed with no bound driver and the thermal class
had no thermal zones. The ARM64 module built against the cached kernel with
matching vermagic. Under the shared device lock, it produced the following output.

```text
7.3.0-g14a425b3a7dc-dirty
TMU bound to exynos850-tmu
TMU register dump headers=1
Temporary TMU module unloaded
```

## Interpretation

The existing device tree supplies a clock that the diagnostic driver can
enable, and the register dump completes on this kernel. The probe does
not expose a calibrated temperature or register thermal zones.

## Reproduce

Build the out-of-tree module against the kernel running on the phone.
Acquire the shared device lock and check that the TMU node is unbound.
Load the temporary module, check the driver symlink and the TMU register
dump in `dmesg`, then unload the module and verify the node is unbound.
