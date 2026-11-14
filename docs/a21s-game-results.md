# Galaxy A21s (`SM-A217F`) SteamOS ARM64 Session & Benchmark Results (`M3-R1`)

- **Device:** Samsung Galaxy A21s (`SM-A217F`), Exynos 850 (8x Cortex-A55), Mali-G52 MC1, `2826 MiB` RAM + `6921 MiB` Swap (`zram0` zstd + `4 GiB` `/swapfile`)
- **Kernel:** `7.3.0-g14a425b3a7dc #3 SMP PREEMPT_DYNAMIC aarch64 GNU/Linux` (`CONFIG_BINFMT_MISC=y`, `CONFIG_ZRAM=y`, `CONFIG_ZRAM_BACKEND_ZSTD=y`)
- **Display & Compositor:** `weston 15.0.1` (`drm-backend.so`, `kiosk-shell.so`, `xwayland=true`, `DSI-1` `720x1600` rotated via `transform=rotate-90` -> **`1600x720` landscape**, `idle-time=0`) + `seatd 0.9.3`

## Steam Client Comparison (x86_64 via Box64 vs Native ARM64)

| Metric | x86_64 `steamrt64` (Box64 `v0.4.4`) | Native ARM64 `steamrtarm64` (`steam_client_publicbeta_linuxarm64`) |
| --- | --- | --- |
| Binary Arch | `x86-64` (`ELF 64-bit`) | **`ARM aarch64` (`ELF 64-bit`, native)** |
| Big Picture UI (`-gamepadui`) | Stuck on loading spinner (`206%` CPU) | **Rendered (`1600x720` landscape, verified via `xwd` + screen)** |
| RAM Used (`free -m`) | `2740 MiB` (`85 MiB` avail) | **`1698–1947 MiB` (`878–1127 MiB` avail)** |
| Swap Used (`free -m`) | `2921 MiB` | **`251 MiB`** |

- **Key fixes applied on A21s:**
  - Pulled Valve's native ARM64 client (`steam_client_publicbeta_linuxarm64`, `Nov 13 2026`) into `/home/gamer/.local/share/Steam/steamrtarm64/`.
  - Installed `libibus`, `libnm`, `openal`, `libvdpau`, `ffmpeg`, plus `arm64` `libgtk-x11-2.0.so.0` / `libgdk-x11-2.0.so.0` into `steamrtarm64/libs/`.
  - Launched `/usr/local/bin/steamos-session` with `-gamepadui -no-cef-sandbox -cef-disable-gpu -cef-disable-gpu-compositing` (avoiding black `steamwebhelper` GPU compositing under Xwayland/Panfrost).

## Live GPU & Compositor Benchmarks (with Native ARM64 Steam Big Picture running)

| Workload | Backend / Renderer | Result |
| --- | --- | --- |
| `glmark2-es2-wayland -b build:duration=3 -b texture:duration=3` | Wayland (`Mali-G52 MC1 (Panfrost)`, OpenGL ES 3.1 Mesa 26.2.6) | **`build`: 849 FPS** (`1.179 ms`), **`texture`: 1228 FPS** (`0.814 ms`), **Score: 1037** |
| `glxgears` (`DISPLAY=:0`) | Xwayland (`Panfrost`) | **`8790.829 FPS`** (`43955 frames in 5.0 seconds`) |
| `box64 -v` | `Box64 arm64 v0.4.4 2f130fa` with Dynarec (`BOX32=ON`, `binfmt_misc`) | Installed & verified for x86/x86_64 game translation |
