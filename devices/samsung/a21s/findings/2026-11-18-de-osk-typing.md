---
status: reproduced
lane: de
device: samsung/a21s
kernel: 7.3.0-g14a425b3a7dc-dirty
author: claude-sonnet-5.5 (continuing Codex's uncommitted work)
ai_assisted: true
date: 2026-11-18
evidence:
  - cmd: 'zenity --entry (as gamer on wayland-0), then touch_swipe.py taps on h, i and return'
  - file: devices/samsung/a21s/findings/logs/2026-11-18-de-osk-typing.txt
---

# Typing on the rabbiz-osk keyboard reaches a client under rabbiz-de

## Observed

With the layer-shell and input-method plumbing (`4d2545c`) in a release `rabbiz-de`, the session
spawned the keyboard over the private socket and it connected ("waiting for text-input activation").
With the **old** keyboard binary (`a21s-osk`) the compositor logged an xkbcommon parse error at
start and a GTK text-entry client received keyboard focus but **never** `zwp_text_input_v3.enter`,
so it never sent `enable` and the taps typed nothing. After rebuilding the keyboard with a
NUL-terminated virtual-keyboard keymap, the xkbcommon error was gone, the client received `enter`
and `enable`, and tapping h, i and return produced `Hi` on the client's stdout in **4 of 4** runs
(one run with debug logging, three without). The compositor and keyboard processes stayed up and
no display timeout was logged. [Raw log](logs/2026-11-18-de-osk-typing.txt)

## Interpretation

Input routing works end to end: touches on the layer surface go to the keyboard, its text and its
Return key reach the focused client. What is **not** shown:
- Nobody looked at the keyboard on the screen. Screenshots do not work under `rabbiz-de`
  (no wlr-screencopy) and the frame dump only contains the shell, so the key positions were computed
  from a host render and the successful typing is the evidence that the surface is where it should be.
- The cause of the missing `enter` in the first run is **not isolated**. The only variable changed
  between the runs was the keyboard binary (keymap fix, and the new `rabbiz-osk` name instead of the
  `a21s-osk` fallback), plus one reboot. A run with the old binary on the same boot would settle it.
- Only English letters, one text-entry app and one session. Thai, emoji, Chrome and Qt apps, and
  repeated focus changes are untested.

## Reproduce

Build `de-bin` and `osk-wayland` with `--release` in `rabbiz-de-build:arm64`, install them as
`/usr/local/bin/rabbiz-de` and `/usr/local/bin/rabbiz-osk` under `scripts/device_session.sh`, reboot,
then as `gamer` start `zenity --entry` and, as root, run `touch_swipe.py` taps at (431,1431),
(538,1364) and (670,1564). Expect `Hi` on the client's stdout. Remove the test binaries afterwards or
keep the previous `rabbiz-de` at `/usr/local/bin/rabbiz-de.prev`.
