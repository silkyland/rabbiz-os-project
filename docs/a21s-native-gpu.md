# Galaxy A21s (`SM-A217F`) — Native Arch Linux ARM64 & Panfrost/PanVK Verification (`M2-R1`)

- **Date:** 2026-11-14
- **Kernel:** `Linux a21s-steamos 7.3.0-g14a425b3a7dc #2 SMP PREEMPT_DYNAMIC aarch64 GNU/Linux`
- **OS:** Arch Linux ARM (`aarch64`) booted natively from eMMC `userdata` (`/dev/mmcblk0p37` -> `/dev/loop0p1` `/boot`, `/dev/loop0p2` `/`) via `uniLoader` + `pmos-ramdisk` + custom `/boot/initramfs-extra` (`init_2nd.sh`)
- **Memory & Swap:** `2826 MiB` RAM + `4 GiB` `/swapfile` (`CONFIG_ZRAM` disabled in `exynos850-a217f.config`)
- **Network:** USB-C `cdc_ncm` (`172.16.42.1/24` <-> host `172.16.42.2/24` NAT masquerade)

## DRM / KMS Devices
- `/dev/dri/card0`: `exynos-drm` (`platform:exynos-drm`, `panel-synaptics-tddi` `720x1600`)
- `/dev/dri/card1`: `simple-framebuffer`
- `/dev/dri/card2`: `panfrost` (`samsung,exynos850-mali`, `arm,mali-bifrost` v7)

## OpenGL ES (`glmark2-es2-drm` on `720x1600` fullscreen)
- **GL_VENDOR:** `Mesa`
- **GL_RENDERER:** `Mali-G52 MC1 (Panfrost)`
- **GL_VERSION:** `OpenGL ES 3.1 Mesa 26.2.6-arch1.1`
- **Result (`-b build:duration=3`):** `578 FPS` (`FrameTime: 1.732 ms`, `glmark2 Score: 577`)

## Vulkan (`PanVK` via `PAN_I_WANT_A_BROKEN_VULKAN_DRIVER=1 vulkaninfo --summary`)
- **Vulkan Instance Version:** `1.4.363`
- **apiVersion:** `1.0.354`
- **driverVersion:** `26.2.6`
- **vendorID / deviceID:** `0x13b5` / `0x72120000`
- **deviceName:** `Mali-G52 MC1`
- **driverID:** `DRIVER_ID_MESA_PANVK` (`panvk`, `Mesa 26.2.6-arch1.1`)
- **Instance Layers:** `VK_LAYER_FROG_gamescope_wsi_aarch64` (`1.3.221`), `VK_LAYER_MESA_anti_lag`, `VK_LAYER_MESA_device_select`

## Installed Compositor & Gaming Stack
- `mesa 1:26.2.6-1`
- `vulkan-panfrost 1:26.2.6-1`
- `gamescope 3.17.0-1`
- `weston 15.0.1-3`
- `seatd 0.9.3-1` (enabled)
- `glmark2 2023.01-2`, `mesa-utils 9.0.0-7`, `vulkan-tools 1.4.363.0-1`
