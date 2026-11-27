# DE lane handoff — Quick Settings tile completion, 2026-11-27

Lane `de`, host-only evidence (`cargo check --workspace`, de-system 67/67 tests,
goldens checked). **No device deploy performed.** The working tree already
contained another lane's WIP (lock_ui, session_lock, auth, display_stack,
composite, …) — nothing was committed, and these changes deliberately sit
on top uncommitted so the owner/lanes can sequence the merge.

## What landed

| Area | State | Where |
|---|---|---|
| Screenshot QS tile | wired — compositor-internal capture (no grim/policy dependency), deferred one power-poll tick so the close animation leaves the frame; shares `capture_and_save_screenshot` with the ctl command | `de-core/runtime/ctl_ipc.rs`, `power.rs`, `shell_ui/panel_ui.rs` |
| Hotspot backend + tile | wired — NetworkManager AP connection `RabbizOS-Hotspot` created on first use (random 12-char password persisted by NM in system-connections, not in our code), `con up/down` after that; poll reads active connections; failures surface as the tile reverting, never a fake on-state | `de-system/src/controls.rs`, `panel_ui.rs` |
| Screen record tile | wired — compositor spawns `wf-recorder` over a private `WAYLAND_SOCKET` with only the `Screencopy` grant (`DE-PRIVILEGED-PROTOCOLS` option B, never the public allow-list); SIGINT to stop so the mp4 trailer is written; the tile self-clears when the recorder exits; recording blanks on the lock screen via the existing `should_blank_capture` path | `de-core/shell_ui/record_ui.rs` (new), `panel_ui.rs` |
| Portal/standards packaging | `xdg-desktop-portal`, `xdg-desktop-portal-gtk`, `gnome-keyring`, `power-profiles-daemon`, `wf-recorder` added to the deploy package list | `scripts/deploy_shell.sh` |
| M6-T2 scripts | `de_soak.sh` (30 min RSS/CMA/thermal sampler, M6 exit criteria baked in), `de_boot_matrix.sh` (N cold reboots via a transient on-device systemd unit; state/result in `/var/lib/de-boot-matrix` because `/tmp` is volatile across reboots) | `scripts/de_soak.sh`, `scripts/de_boot_matrix.sh` |

## Honestly not done, with reasons

- **Location tile**: no GNSS hardware path exists until the modem (M6) lands and
  geoclue is packaged. The tile stays dimmed; wiring a fake backend would
  violate the "no fake tile state" rule in `controls.rs`.
- **Hotspot on the A21s is unverified hardware-wise**: the SCSC driver may not
  support AP mode (or STA+AP concurrency). If `con up` fails the tile just
  falls back off — check `nmcli dev wifi list` / driver caps on device first.
- **wf-recorder CPU cost**: default libx264 encode of 720×1600 on Exynos 850
  is heavy; expect thermal/power cost. If unusable on device, consider
  `-c libx264` + ultrafast params or a raw/iframe-lite mode. Needs an
  on-device measurement before tuning.
- Device verification of all three tiles is pending a device-queue slot.
- The pre-existing `lock_ui` PIN test failure and the golden diffs
  (lock/drawer/recents/panels/first_run/settings/morph) come from the other
  lane's WIP already in the tree — confirmed unrelated (de-sim builds its
  panel model inline; nothing here touches de-ui/de-paint/de-design).

## What remains for "AOSP-parity" after this

DE-side: portal packages must actually be installed on the rootfs image
(`deploy_shell.sh --packages` list is only a printout), `wp_security_context`
sandboxing (M5+), performance hardening measurements (M6-T1/T2) on device.
Platform-side (the real gap): modem M6, power/suspend M5, audio stability M3.6.
