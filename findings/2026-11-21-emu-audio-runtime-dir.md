---
status: verified
lane: de
device: any
kernel: n/a
author: Devin
ai_assisted: true
date: 2026-11-21
evidence:
  - cmd: "XDG_RUNTIME_DIR=/tmp/rabbiz-emu-live/rt pactl info  # -> pa_context_connect() failed: Connection refused"
  - cmd: "ln -s /run/user/1000/{pipewire-0,pipewire-0-manager} /tmp/rabbiz-emu-live/rt/ && ln -s /run/user/1000/pulse/native /tmp/rabbiz-emu-live/rt/pulse/native && XDG_RUNTIME_DIR=/tmp/rabbiz-emu-live/rt paplay bell.oga  # -> exit 0, audible on host"
supersedes: []
---

# Tier 2 emu apps were silent because XDG_RUNTIME_DIR hid the host audio sockets

## Observed

`rabbiz-emu-viewer` launches `rabbiz-de --backend=virtual` with an isolated
`XDG_RUNTIME_DIR=/tmp/rabbiz-emu-live/rt` (`start_native_linux_backend` in
`tools/rabbiz-sdk/crates/rabbiz-emu-viewer/src/main.rs`). Apps spawned inside the
emu (Chrome, `rabbiz-music`, etc.) inherit it, so libpulse/libpipewire resolve
`$XDG_RUNTIME_DIR/pulse/native` and `…/pipewire-0` — which do not exist there.
The real sockets live under the host's `/run/user/$UID`. Result: guest audio
connects nowhere; `pactl info` under the emu env returns "Connection refused".

## Interpretation

The isolated runtime dir is correct for Wayland (the emu provides its own
`wayland-0`), but nothing ever bridged the audio endpoints. PipeWire's
pulse-compatible server accepts same-uid local connections without
`~/.config/pulse/cookie`, so plain socket symlinks are sufficient — no
`PULSE_SERVER`/`PIPEWIRE_REMOTE` env or cookie copy is required.

## Reproduce

1. Start `rabbiz emu start` on a Linux host, then:
   `XDG_RUNTIME_DIR=/tmp/rabbiz-emu-live/rt pactl info` → refused.
2. Symlink `pipewire-0`, `pipewire-0-manager` and `pulse/native` from
   `/run/user/$UID` into `/tmp/rabbiz-emu-live/rt` (done automatically at
   backend launch since this fix), retry → connects and `paplay` is audible.
