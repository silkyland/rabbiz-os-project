# Finding note: replacing software rendering with Panfrost (2026-11-16)

For the next agent. Facts are measured on the SM-A217F (kernel `7.3.0-g14a425b3a7dc-dirty`, Mesa 26.2.6,
Qt 6.12) unless marked **guess**. Background path: [a21s-qt-gpu-scanout.md](a21s-qt-gpu-scanout.md).

## What was software, what is hardware now

| Component | Before | Now |
|---|---|---|
| labwc compositor | GLES2 / Panfrost | unchanged (already HW) |
| Shell (`rabbiz-shell`) | GPU via `a21s-qt-gpu` | unchanged |
| In-house QML apps (calculator, clock, dialer, sms, settings, photos, music, notes, terminal) | `QT_QUICK_BACKEND=software` in each wrapper | `exec a21s-qt-render quickshell …` (new): GPU while CMA is healthy, software otherwise |
| Elisa (launched from `Media.qml`) | software | `a21s-qt-render elisa` (same guard) |
| Chrome (`rabbiz-browser`) | `--disable-gpu` | still `--disable-gpu` by default; `RABBIZ_BROWSER_GPU=1` opts into GPU (works, but see F3) |
| angelfish / qmlkonsole in `rabbiz-spotify-ctl`, Dolphin, other third-party Qt | software / wl_shm | **not changed** (F5) |
| Session default (`labwc/environment`, `rabbiz-labwc-session`, `.service`) | software | kept as the fail-closed fallback on purpose |

`a21s-qt-render` (`rootfs-overlay/usr/local/bin/`) is the single decision point: `a21s-qt-gpu --check` ok **and**
`CmaFree >= RABBIZ_CMA_FLOOR_KB` (default 96 MiB) -> `exec a21s-qt-gpu "$@"`, else software. `RABBIZ_FORCE_SOFTWARE=1` forces software.
Verified: calculator reports `Mali-G52 MC1 (Panfrost)` via `QSG_INFO`; at 32 MiB free the wrapper picks software, at 197 MiB `rhi`.

## F1 — GPU Qt apps render correctly next to the GPU shell
Calculator on RHI/OpenGL started cleanly with the shell also on GPU: no Wayland protocol error, no import error in dmesg.
Cost of one full-screen window: **~14 MiB CMA** (3 swapchain buffers of 4 780 032 B).

## F2 — RESOLVED: CMA leak = labwc (Mesa kmsro) cannot free imported buffers on the exynos render node
**Root cause (measured 2026-11-16):** `strace -f -e trace=ioctl -p $(pgrep -x labwc)` while a GPU app starts/exits shows labwc doing
`PRIME_FD_TO_HANDLE` on `/dev/dri/renderD128` (exynos) for every client buffer, then `MODE_DESTROY_DUMB` on that fd -> **`-1 EACCES`**
(the DRM core only allows `CREATE/DESTROY_DUMB` on the primary node; `GEM_CLOSE` is `DRM_RENDER_ALLOW`). The handle is never closed,
so each 4.6 MiB CMA buffer stays pinned until labwc exits (matches: no fd holder, `Attached Devices: 0`, ~3 buffers per launch).
Ruled out by `tools/cma-leak/leaktest.c` on the device: kernel dumb alloc/free, prime export, prime import into a second exynos file
and into Panfrost, with/without closing the handle — all return CMA to the pool (0 kB delta over 40 iterations).
**Fix (userspace, no kernel rebuild):** `tools/qt-scanout/dumb-destroy.c` is LD_PRELOADed into labwc only (`rabbiz-labwc-session`); on
`DESTROY_DUMB` + `EACCES` it retries as `GEM_CLOSE` on the same handle. Install: `scripts/setup_dumb_destroy_fix.sh`; active after labwc restarts.
**Verified:** after reboot, 12 × (start `rabbiz-calculator`, 5 s, kill) -> `CmaFree` 218 228 kB -> 213 472 kB (before the fix: 197 -> 54 MiB in 6 cycles),
0 `FB allocation failed`, shell still running. Not yet re-run for Chrome (F3) or with several apps open at once.
**Kernel alternative (not built):** flag `DRM_IOCTL_MODE_DESTROY_DUMB` with `DRM_RENDER_ALLOW` in `drivers/gpu/drm/drm_ioctl.c`
(it only deletes a handle the caller owns, same as `GEM_CLOSE`). No kernel tree/toolchain/boot-image tooling is in this checkout
(`src/`, `build/` are empty besides a partial clone), so it was not needed nor built; revisit only if the shim proves insufficient.
`a21s-qt-render`'s CMA floor is now only a safety net; it can stay.

## F3 — Chrome on Panfrost: works, no longer leaks (retested 2026-11-16 after the F2 fix)
`RABBIZ_BROWSER_GPU=1 rabbiz-browser` -> `a21s-qt-gpu chrome --use-gl=angle --use-angle=gles`; the gpu-process runs with `use-gl=angle use-angle=gles`
and a WebGL probe page reports an `ANGLE (Mesa …, OpenGL ES 3.1)` renderer (readable faintly through the lock-screen fade in a grim capture;
exact device string not captured — read it with `WEBGL_debug_renderer_info` on a screen that stays awake).
CMA, three start/kill cycles of 20 s each: free 123 -> 16 (cold start) -> closed 113 | 95 -> 116 | 71 -> 118 MiB, i.e. returns to the pool and does
not accumulate; later runs 140 -> 93 -> 123 MiB. 0 `FB allocation failed`, 0 Panfrost faults, shell alive. A software-Chrome control run
(140 -> 90 MiB running) used about as much CMA, so the cost is not specific to the GPU path and was **not isolated** (candidates: Chrome's own
window buffers through labwc, wake/unlock redraws during the test).
Still unmeasured: frame rate / scroll smoothness / thermals vs `--disable-gpu`, video decode, several tabs. Default stays `--disable-gpu`
(opt-in via `RABBIZ_BROWSER_GPU=1`; its CMA floor is 128 MiB). Flip the default only after a smoothness comparison shows a gain.
Note: the main Chrome process shows no `LD_PRELOAD` of the shim in `/proc/<pid>/environ` (Chrome re-execs); rendering still works.

## F4 — Steam: removed
Steam and its support (nested Weston, gamescope, session units, shell card) were uninstalled on 2026-11-16 to free resources; revisit after F2 is fixed. See ROADMAP "Deferred".

## F5 — Not converted
Third-party Qt Quick apps launched through `Appearance.appEnvironment` still get `QT_QUICK_BACKEND=software`; that map would have to drop the
override and the `.desktop` `Exec` would need `a21s-qt-render` (only worth it after F2). angelfish (QtWebEngine, `--disable-gpu`) has the Chrome problem.
QtWidgets apps (Dolphin) are wl_shm either way; the GPU only matters for Qt Quick/WebEngine.

## Not deployed / not verified
Only the changed wrappers + `Media.qml` were pushed with `scp` (not `deploy_shell.sh`). `apps/clock`, `notes`, `photos`, `music`, `terminal` QML
is not on the device (`/usr/share/rabbiz-apps` has only `RabbizKit` and `calculator`), so only the calculator was exercised. No FPS/thermal measurement.
`scripts/cma_stat.sh` prints CMA free + dma-buf census (`ssh root@172.16.42.1 sh < scripts/cma_stat.sh`).

## Boot splash (2026-11-16)
`rabbiz-splash.service` (sysinit) paints `assets/branding/rabbizos-wordmark-on-cream.png` (pre-scaled to `rootfs-overlay/usr/share/rabbiz/rabbizos-splash.bmp`,
640x225 top-down BGRA) centred on `/dev/fb0` and silences the console (`dmesg -n 1`, `kernel.printk`, `ShowStatus=no`, `getty@tty1` masked).
Verified from a `/dev/fb0` dump 10 s after boot: logo on cream, no text. Limit: it starts after the rootfs mounts, so the kernel/initramfs lines
before that are still visible; hiding those needs `quiet loglevel=3` in the boot image cmdline (kernel/boot-image rebuild, not done).
