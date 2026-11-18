---
status: reproduced
lane: de
device: samsung/a21s
kernel: 7.3.0-g14a425b3a7dc-dirty
author: claude-sonnet-5.5
ai_assisted: true
date: 2026-11-18
evidence:
  - file: devices/samsung/a21s/findings/logs/2026-11-18-de-privileged-policy.txt
---

# Any app can read the clipboard by taking focus, with or without the data-control global

## Observed

A renamed copy of `wl-paste`, refused the data-control global by the policy, still printed the text that `wl-copy`
had just put on the clipboard (two runs; the first had an empty clipboard, the second read `policy-test`). The journal
shows the global was hidden from it. [Raw log](logs/2026-11-18-de-privileged-policy.txt)

## Interpretation

`wl-paste` has a fallback for compositors without data-control: it opens a tiny window, receives keyboard focus and
reads the selection as an ordinary focused client. `rabbiz-de` gives the foreground, and therefore keyboard focus, to
every newly mapped toplevel (`first_app_frame`). So any app can bring itself to the foreground without a user action and read
what is on the clipboard. Hiding data-control does not close that path, and the same weakness would let an app swallow
keyboard input. This is the usual cost of a compositor without focus-stealing prevention; on a phone it deserves a
fix, for example giving the foreground only to windows the shell launched or that present a valid `xdg-activation`
token, and keeping other new windows in the background. Which rule is right is a product decision and was not made here.

## Reproduce

`printf x | wl-copy`, then run a renamed copy of `wl-paste` as the session user and check that it prints `x`.
Clear the clipboard and remove the copy afterwards.
