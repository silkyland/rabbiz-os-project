# A21s hardware key combinations

Verified on the lab device on 2026-11-16:

- Volume Up/Down alone changes volume, including after a Power press.
- Power + Volume Down: release before three seconds takes one screenshot;
  hold both for three seconds opens the power menu once, without a screenshot.
- Power + Volume Up, or both volume keys held together, takes a screenshot.
- Labwc Power binding uses `onRelease="yes"`; holding Power must not repeat
  screen toggles. The release refreshes the combo marker so the compositor's
  delayed toggle is suppressed after a long combination.

## Regression evidence

Initial physical test failed: raw evdev showed Power and Down held for
2.7 seconds, while `a21s-power.cmd` changed to `toggle` about ten times per
second. The compositor repeated its press binding; the user saw flickering.
Installed labwc 0.20.2 documents `onRelease`; changing that binding and
refreshing the marker removed the repeated toggles. A subsequent physical
trace showed `menu` after three seconds, then clean key releases.
An automatic four-second evdev injection produced exactly one `menu` and
no new screenshot. The owner confirmed the menu opens without flickering.

A second report found lone Volume Up taking screenshots after other key
presses. Replaying the actual key loop demonstrated that the 400 ms
recent-press fallback counted released keys as active. Three regression
tests failed before the fix: released Power then Volume Up, separate volume
taps, and volume autorepeat during a held combo. Combos now require held
keys; repeat events cannot leak volume actions from a combo. All four loop
tests and six HoldMenu tests pass. The owner confirmed lone Volume Up now
changes volume without a screenshot.

Run host checks:

```sh
python3 -B tests/test_holdmenu.py
python3 -B tests/test_volume_keys.py
```

Deployment copied only `volume-keys.py` and `a21s_holdmenu.py` as root:root,
restarted volume-keys.service, and patched only the Power binding in the
live labwc config, followed by SIGHUP. The whole shell deployment script
was not run. Both installed files' checksums matched the host; the service
reported active and watched gpio-keys and s2mpu12-pwrkey without import errors.

Diagnostic captures are local under `build/usb-investigation/keytrace*.log`.
The 90-second observer did not grab input and was stopped after testing.
