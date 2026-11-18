Findings for samsung/a21s. Format: docs/FINDINGS.md

## Index

### Kernel / hardware

| Finding | Status | Topic |
|---|---|---|
| [tmu-efuse-temperature](2026-11-18-tmu-efuse-temperature.md) | verified | TMU reads three live sensors with eFuse trim |
| [tmu-clocked-register-probe](2026-11-18-tmu-clocked-register-probe.md) | verified | Clocked TMU register probe binds |
| [panfrost-thermal-run](2026-11-18-panfrost-thermal-run.md) | verified | 90 s GPU load with live TMU monitoring |
| [fuel-gauge-s2mu106-monout-bogus](2026-11-18-fuel-gauge-s2mu106-monout-bogus.md) | verified | FG instantaneous reads real, MONOUT averages bogus |
| [thermal-cpuidle-cpufreq-state](2026-11-18-thermal-cpuidle-cpufreq-state.md) | reproduced | kernel #12: no thermal zones, no idle states, `performance` governor |
| [als-zero-chub-scan](2026-11-18-als-zero-chub-scan.md) | reproduced | CHUB scan omits accel/gyro/ALS; reads now `-ENODATA` |
| [kexec-container-kernel-black](2026-11-18-kexec-container-kernel-black.md) | reproduced | container-built kernel black-screens on kexec |
| [soc-drivers-a21s-assumptions](2026-11-18-soc-drivers-a21s-assumptions.md) | reproduced | ledger of board data inside shared exynos850 drivers |
| [suspend-blocked-abox](2026-11-18-suspend-blocked-abox.md) | reproduced | `echo mem` aborts: abox runtime suspend -EAGAIN, usage_count 1 |
| [abox-calliope-root-causes](2026-11-18-abox-calliope-root-causes.md) | verified | the eleven ABOX/Calliope bring-up root causes + methods |
| [dsi-underrun-link-recovery](2026-11-18-dsi-underrun-link-recovery.md) | hypothesis | DSI link-restart patch 0010 (built, not boot-tested) |

### DE / compositor

| Finding | Status | Topic |
|---|---|---|
| [de-wayvnc-unauthenticated-localhost](2026-11-18-de-wayvnc-unauthenticated-localhost.md) | reproduced | mirror VNC has no auth on usb0/loopback |
| [de-wayvnc-needs-virtual-pointer](2026-11-18-de-wayvnc-needs-virtual-pointer.md) | reproduced | a21s-wayvnc fails/restarts under rabbiz-de |
| [de-clipboard-focus-fallback](2026-11-18-de-clipboard-focus-fallback.md) | reproduced | any app can read the clipboard by taking focus |
| [de-osk-typing](2026-11-18-de-osk-typing.md) | reproduced | rabbiz-osk typing reaches a client |
| [de-first-flip-ebusy](2026-11-18-de-first-flip-ebusy.md) | hypothesis | first flip EBUSY → labwc fallback after session swap |
| [de-session-orphans](2026-11-18-de-session-orphans.md) | hypothesis | stopping the session can orphan the compositor |
| [de-launch-morph-frame-timing](2026-11-18-de-launch-morph-frame-timing.md) | hypothesis | morph frames queued during DRM timeouts |
| [de-morph-page-flips](2026-11-18-de-morph-page-flips.md) | hypothesis | launch/return gestures flipped cleanly post-reboot |
| [de-shell-menu-device](2026-11-18-de-shell-menu-device.md) | hypothesis | shell context menu opens; row select unconfirmed |
| [de-privileged-policy-device-check](2026-11-18-de-privileged-policy-device-check.md) | hypothesis | default-deny privileged-global policy works |

Raw logs live in `logs/`. One file per topic; keep refuted findings —
say what refuted them instead of deleting.
