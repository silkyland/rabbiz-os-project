---
status: reproduced
lane: de
device: samsung/a21s
kernel: 7.3.0-g14a425b3a7dc-dirty
author: claude-sonnet-5.5 (continuing Codex's uncommitted work)
ai_assisted: true
date: 2026-11-18
evidence:
  - cmd: 'zenity --entry (as rabbiz on wayland-0), then touch_swipe.py taps on h, i and return'
  - file: devices/samsung/a21s/findings/logs/2026-11-18-de-osk-typing.txt
  - file: devices/samsung/a21s/findings/logs/2026-11-18-de-osk-acceptance/
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
Return key reach the focused client. The earlier gaps are now closed:

- **Missing `enter` isolated (same-boot A/B).** With `a21s-osk` swapped in for one run and
  `rabbiz-osk` restored for the next, **both** delivered `enter` and typed `Hi` — the first run's
  missing `enter` was the degraded boot (the compositor had lost DRM to an orphan and the session
  fell back to labwc), not the keyboard binary. [Evidence](logs/2026-11-18-de-osk-acceptance/evidence.txt)
- **The keyboard is verifiable on screen.** `zwlr_screencopy_manager_v1` is now implemented
  (shm path) and the frame dump composites layer surfaces, so `grim` captures the real frame:
  [OSK visible](logs/2026-11-18-de-osk-acceptance/osk-visible.png),
  [Thai layout](logs/2026-11-18-de-osk-acceptance/osk-thai-on.png).
- **Untested cases covered:** Thai (`ไพ` committed after the globe switch), a Qt field
  (`pinentry-qt`: `enter` + `commit_string`, [screenshot](logs/2026-11-18-de-osk-acceptance/osk-qt.png)),
  Chrome under ozone-wayland ([typed chars](logs/2026-11-18-de-osk-acceptance/osk-chrome-c.png)),
  focus moving between two clients (`enter`/`leave`/re-`enter` all delivered), and the keyboard
  hiding when the last text client exits ([home screen, no OSK](logs/2026-11-18-de-osk-acceptance/osk-hidden.png)).
- **App lifecycle checked along the way:** dead clients' surfaces do unmap
  (`toplevel_destroyed … matched=true`), but `de-apps::launch` leaked zombies (Child never
  `wait()`ed) — fixed by a detached waitpid thread.
- Still unproven: dmabuf/EGL app content is invisible to the CPU screencopy path (documented
  limitation — those surfaces show the clear colour in `grim` output), and emoji panel typing.

## Reproduce

Build `de-bin` and `osk-wayland` with `--release` in `rabbiz-de-build:arm64`, install them as
`/usr/local/bin/rabbiz-de` and `/usr/local/bin/rabbiz-osk` under `scripts/device_session.sh`, reboot,
then as `rabbiz` start `zenity --entry` and, as root, run `touch_swipe.py` taps at (431,1431),
(538,1364) and (670,1564). Expect `Hi` on the client's stdout. Remove the test binaries afterwards or
keep the previous `rabbiz-de` at `/usr/local/bin/rabbiz-de.prev`.
