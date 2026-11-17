#!/bin/sh
# cma_stat.sh — CMA headroom + dma-buf census on the device (run on the A21s as root).
# CMA (256 MiB) is the budget every exynos-drm scanout/GPU window buffer is carved from; when it is
# exhausted the compositor/Qt clients crash (docs/FINDINGS-hw-render.md).
printf 'CmaFree: %s kB\n' "$(awk '/CmaFree/{print $2}' /proc/meminfo)"
awk '/^[0-9]+\t/{n++; s+=$1; c[$1]++} END{printf "dma-bufs: %d, %d MiB\n", n, s/1048576; for(k in c) printf "  %d B x %d\n", k, c[k]}' /sys/kernel/debug/dma_buf/bufinfo
