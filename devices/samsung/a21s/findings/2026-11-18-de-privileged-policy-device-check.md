---
status: hypothesis
lane: de
device: samsung/a21s
kernel: 7.3.0-g14a425b3a7dc-dirty
author: claude-sonnet-5.5
ai_assisted: true
date: 2026-11-18
evidence:
  - file: devices/samsung/a21s/findings/logs/2026-11-18-de-privileged-policy.txt
---

# The default-deny policy for privileged Wayland globals works on the A21s

## Observed

With the policy build (`d118bed`) installed, `grim` took a screenshot (1.1 MB PNG), a renamed copy of `grim`
placed in `/tmp` was told "compositor doesn't support the screen capture protocol" and wrote no file, the on-screen
keyboard still typed `Hi` into a GTK client, and `a21s-wayvnc` stayed up. `wayvnc` was refused `layer_shell` and
`data_control` (optional for it) and never refused screencopy or virtual input. A renamed copy of `wl-paste` was
refused the data-control global. No display error appeared in `dmesg`. [Raw log](logs/2026-11-18-de-privileged-policy.txt)

## Interpretation

The policy decides on the resolved executable path, so a copy under another path loses the permission, as designed.
Each check was seen once in one session, so this stays a hypothesis until it is repeated.

Not verified: that screencopy returns a blank frame on the lock screen (the rule is unit-tested, but there is no way to
show the lock screen on demand yet); the opt-in policy log introduced afterwards; the policy file extension on the device;
what the Mac companion shows when wayvnc is refused the clipboard (VNC clipboard sharing is presumably gone).

The earlier per-client "denied" log was noisy: every client that lists the registry is checked against every privileged
global, so harmless programs such as `zenity` were reported as denied `virtual_pointer`. The log is now opt-in
(`RABBIZ_DE_POLICY_LOG=1`), and the first line at startup says the policy is active.

## Reproduce

Install the build under the device lock and reboot. As `rabbiz`: `grim -o DSI-1 x.png` works; `cp /usr/bin/grim
/tmp/g && /tmp/g -o DSI-1 y.png` fails and writes no file. Remove the test copies afterwards.
