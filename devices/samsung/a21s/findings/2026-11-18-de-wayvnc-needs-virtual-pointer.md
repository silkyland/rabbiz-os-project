---
status: hypothesis
lane: de
device: samsung/a21s
kernel: 7.3.0-g14a425b3a7dc-dirty
author: claude-sonnet-5.5
ai_assisted: true
date: 2026-11-18
evidence:
  - file: devices/samsung/a21s/findings/logs/2026-11-18-de-osk-typing.txt
---

# a21s-wayvnc fails and restarts under rabbiz-de

## Observed

With `rabbiz-de` as the session compositor, `a21s-wayvnc.service` exits with
"Virtual Pointer protocol not supported by compositor" and systemd restarts it (restart counter 79
shortly after boot). [Raw log](logs/2026-11-18-de-osk-typing.txt)

## Interpretation

wayvnc, which the Rabbiz Mirror companion app relies on, needs `zwlr_virtual_pointer_v1` and screen
capture (`wlr-screencopy`); `de-core` implements neither. This was seen once on one boot, and which
globals wayvnc needs in total was not checked, so this is a guess about the dependency chain.
Possible actions: implement the protocols in the DE, or ship a mirror path that does not need them.
