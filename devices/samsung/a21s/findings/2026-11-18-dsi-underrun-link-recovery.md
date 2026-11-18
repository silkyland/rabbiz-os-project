---
status: hypothesis
lane: kernel
device: samsung/a21s
kernel: 7.3.0-g14a425b3a7dc-dirty
author: devin-kernel-agent
ai_assisted: true
date: 2026-11-18
evidence:
  - cmd: "make O=/work/out ARCH=arm64 LLVM=1 drivers/gpu/drm/exynos/exynos850_drm_dsi.o Image"
related:
  - devices/samsung/a21s/findings/2026-11-18-de-first-flip-ebusy.md
  - devices/samsung/a21s/docs/HANDOFF.md
supersedes: []
---

# DSI underrun link-recovery patch (built, not boot-tested)

## Implemented

`devices/samsung/a21s/kernel/patches/0010-dsi-underrun-link-recovery.patch`
implements HANDOFF §0 proposed fix #2 in
`drivers/gpu/drm/exynos/exynos850_drm_dsi.c`:

- `DSIM_INT_UNDER_RUN` in `dsim850_irq_handler` now schedules
  `dsim->underrun_work` (only when `dsim->enabled`) in addition to the
  existing ratelimited log.
- The worker `dsim850_underrun_recover` takes `dsim->lock` (the same
  mutex that serialises `mipi_dsi_host_transfer` and the
  `enabled = false` write in `atomic_post_disable`), re-checks
  `enabled`, then runs `dsim850_stop -> dsim850_init_link ->
  dsim850_start` to resynchronise the video-mode link in place.
- `INIT_WORK` in probe; `cancel_work_sync` in remove.

Compiles clean (`exynos850_drm_dsi.o`, full `Image` linked). The patch
reverse-applies cleanly against the pinned tree, so it is well-formed
for `bootstrap_build_env.sh`.

## Why the lock is held across the restart

`atomic_post_disable` clears `enabled` under `dsim->lock` before calling
`dsim850_stop` outside it. Holding the lock for the whole worker body
means either the worker completes first (post_disable then stops the
restarted link — still ends disabled, correct) or post_disable wins and
the worker sees `enabled == false` and returns. A slow in-flight panel
transfer (e.g. a wedged RX-DATA-DONE poll) delays the restart by at most
its own timeout; that is safe, not a deadlock.

## Not verified on device — blocked boot path

The driver is built-in (`CONFIG_DRM_EXYNOS850_DSI=y`), so testing needs a
new kernel boot. kexec of the container-built Image black-screens (see
`2026-11-18-kexec-container-kernel-black.md`); a persistent flash needs
owner approval. Until then this is a compile-verified hypothesis, not a
fix. Expected behaviour when booted: teardown underrun logs `underrun:
restarting video link`, the link resyncs, and the next commit's
`vblank wait timed out`/`flip_done timed out`/`EBUSY` chain (and the
`de-first-flip-ebusy` fallback to labwc) should stop occurring — the
acceptance test is ten compositor stop/start cycles.

## Open questions for the boot test

- Does the wedge's `VIDEO_MODE_ACTIVE` poll in `dsim850_stop` time out
  when decon keeps streaming? It only warns and continues, so recovery
  should proceed — but watch the log.
- Repeated underruns re-arm the work item each IRQ; no retry budget is
  implemented. If the link thrashes, add a backoff.
- `atomic_pre_enable`'s boot_on path skips `pm_runtime_resume_and_get`
  while `atomic_post_disable` always `pm_runtime_put_sync` — a
  pre-existing imbalance noticed in passing; the boot_on link survives
  anyway, but it is worth a line in the DTB/boot test notes.
