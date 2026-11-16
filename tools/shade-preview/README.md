# shade-preview — render the pull-down shade without the phone

`preview.py` loads the **real** `A21s.Panels.QuickSettings` and
`A21s.Panels.NotificationCenter` QML in QtQuick's software backend and saves
720×1600 PNGs, so layout, theme and grouping changes can be checked on a host
instead of by deploying to the A21s. It also prints `content height` next to
`page height`: content taller than the page means the sheet will scroll on the
device (`Flickable.interactive` switches on).

```sh
pip install pyside6-essentials          # Qt 6.12 verified
python3 tools/shade-preview/preview.py  # -> /tmp/shade-preview/*.png
python3 tools/shade-preview/preview.py --light   # + light theme variants
```

## What is stubbed, and why

`Quickshell` (Wayland layer-shell, D-Bus services, IPC) cannot run on the host,
so the script writes inert QML stand-ins into a temp import path:
`Quickshell.Io.{Process,FileView,StdioCollector}`, `SystemClock`, `Singleton`,
`Wayland.ToplevelManager`, `Services.Notifications.NotificationServer` (fed from
`NotificationDemo.js` fixtures), `Services.Mpris`, `Services.UPower`. The stub
tree is generated on every run from the `STUBS` dict in `preview.py` — it is not
tracked as QML files on purpose, so it cannot drift into a second source of
truth for shell behaviour.

## Caveats

- The status bar is its own `PanelWindow` and is not part of these pages.
- macOS window decoration shaves ~21 px off the frame, so the printed page
  height is ~1471 instead of 1600; content fits at ~1396.
- The host has no "Noto Sans", so text metrics are close to the device but not
  identical, and `Brightness.available` / telemetry are false in the preview
  (tiles will show their unavailable states).
- This verifies layout and QML validity, **not** touch behaviour, blur or
  compositor integration — those still need the phone.

`qmllint` from the same PySide6 install is the cheaper first check:
`qmllint -I shell shell/A21s/Panels/*.qml` (expect only `unresolved-type`
noise for the missing `Quickshell` imports).
