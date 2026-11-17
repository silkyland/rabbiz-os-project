# Persistent display brightness

Verified on A21s, 2026-11-16. `Brightness.setLevel()` is the user setting;
`setTransientLevel()` applies temporary screen-torch changes. Sleep/wake's
hardware blanking does not replace the saved preference.

The old service read hardware brightness but did not save a selected level.
Native systemd-backlight stores raw brightness and cannot distinguish a
user choice from the shell's temporary zero when the screen is off. The
reported near-zero startup was not independently captured before this fix;
the missing shell preference was verified in code and on disk.

`ShellPrefs.brightnessLevel` now persists the selected percentage in the
existing prefs.json. Brightness restores it once both preferences and the
backlight probe are ready. Slider writes are debounced for 250 ms. On an
old installation without the key, a readable current level is migrated;
if the hardware is at an off/near-zero level, migration uses 60% instead.
The screen-torch path uses the transient setter and cannot save 100% over
the normal preference.

## Hardware checks

- The shell and `shell/tests/brightness-integration.qml` compiled and ran
  against the installed Quickshell runtime.
- The integration probe persisted 64%, applied temporary zero, verified
  the preference remained 64%, then restored the display to 64%.
- On-disk preference was 64, hardware raw brightness 163 of 255.
- Forced hardware brightness to zero, then rebooted. New boot ID:
  `aa605b13-49d9-4b86-8dff-b99f647c64a1`.
- After boot: saved preference **64**, hardware brightness **163/255**.
- Restored the pre-test user level **76%**: disk preference 76 and raw
  hardware **194/255**. The test value was not left as the user's setting.

Device integration command (intentionally changes the saved value to 64%):

```sh
runuser -u gamer -- env XDG_RUNTIME_DIR=/run/user/1001 \
  WAYLAND_DISPLAY=wayland-0 QML_IMPORT_PATH=/usr/share/a21s-shell \
  QT_QUICK_BACKEND=software quickshell -p /tmp/brightness-integration.qml
```

Copy the fixture from `shell/tests/brightness-integration.qml` first and
serialize hardware access with `scripts/device_session.sh`. Restore the
user's original level afterward. Host `scripts/check.sh` also passed.
