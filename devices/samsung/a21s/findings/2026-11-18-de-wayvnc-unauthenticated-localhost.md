---
status: reproduced
lane: de
device: samsung/a21s
kernel: 7.3.0-g14a425b3a7dc-dirty
author: claude-sonnet-5.5
ai_assisted: true
date: 2026-11-18
evidence:
  - cmd: "ss -ltnp | grep 5900 (on the device) and an RFB handshake from the Mac, both read-only"
  - file: devices/samsung/a21s/findings/logs/2026-11-18-de-wayvnc-unauthenticated-localhost.txt
---

# The mirror's VNC server offers no authentication on usb0 and on loopback

## Observed

`a21s-wayvnc` listens on `172.16.42.1:5900` (usb0) and `127.0.0.1:5900`, and its RFB handshake offers
only security type 1, **None**. There is no wayvnc config file and the device has no firewall rules.
It is not bound to the Wi-Fi address. [Raw log](logs/2026-11-18-de-wayvnc-unauthenticated-localhost.txt)

## Interpretation

This matches what `docs/rabbiz-mirror.md` already says (no authentication, USB and loopback only), so the
USB side is a known, documented choice. What the documentation does not discuss is **loopback**: any process
on the phone, including any installed app, can connect to `127.0.0.1:5900`, see the whole screen (the lock
screen too) and send pointer and keyboard events. That path is outside the compositor, so an allow-list for
screencopy and virtual-input globals inside rabbiz-de would not stop it. wayvnc runs as the session user and
asks the compositor with its own identity, so an allow-list that trusts wayvnc trusts everyone who can reach
its port. Whether the loopback listener is needed at all (for example for an SSH-tunnelled connection) was
not checked; this is a guess about intent, not a verified requirement.

Options for the owner, not decided here: drop the loopback listener; require a password (RFB type 2) with
the secret in a root-owned file; bind only to usb0 and require that the USB host is the peer; or make the
mirror an on-demand service started and stopped by the user with a visible indicator.
