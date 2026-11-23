---
status: reproduced
lane: userland
device: samsung/a21s
kernel: n/a
author: agent
ai_assisted: true
date: 2026-11-23
evidence:
  - file: rootfs-overlay/usr/local/bin/rabbiz-session
  - file: rootfs-overlay/usr/local/bin/rabbiz-labwc-session
supersedes: []
---

# `rabbiz-session` refuses `labwc` fallback when a non-swipe keyguard lock is configured

## Observed
`rootfs-overlay/usr/local/bin/rabbiz-session` supervises `rabbiz-de` and falls back to `rabbiz-labwc-session` (`exec /usr/bin/labwc`) on exit 75 or after 3 crashes within 60 s. `rabbiz-labwc-session` launches `labwc` and the legacy QML shell without keyguard PIN/passphrase authentication.

## Interpretation
Falling back to an unlocked `labwc` session after repeated `rabbiz-de` crashes would allow a crash loop (or an induced `SIGSEGV`) to bypass the lock screen. Because `labwc` cannot enforce `rabbiz-de`'s `keyguard.json` Argon2id lock at startup, `to_labwc()` in `rabbiz-session` now checks `$XDG_CONFIG_HOME/rabbiz-shell/prefs.json` (`lock_method`) and `keyguard.json` and exits with status `1` (fail-closed) rather than starting `labwc` when any non-swipe lock is configured or `prefs.json` is unreadable.

## Reproduce
1. Configure `"lock_method": "pin"` in `$XDG_CONFIG_HOME/rabbiz-shell/prefs.json`.
2. Trigger `to_labwc` in `rootfs-overlay/usr/local/bin/rabbiz-session` (e.g. `rabbiz-de --force-fail`).
3. Observe `rabbiz-session` logs `refusing labwc fallback because keyguard lock is configured (fail-closed)` and exits `1` instead of launching `labwc`.
