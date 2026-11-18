---
status: hypothesis
lane: de
device: samsung/a21s
kernel: 7.3.0-g14a425b3a7dc-dirty
author: claude-sonnet-5.5
ai_assisted: true
date: 2026-11-18
evidence:
  - file: devices/samsung/a21s/findings/logs/2026-11-18-de-shell-menu.txt
  - file: devices/samsung/a21s/findings/logs/2026-11-18-de-shell-menu-open.png
---

# The shell context menu opens and closes on the A21s; choosing a row is not confirmed

## Observed

A right-click on the empty home screen opens the shell menu, correctly placed and drawn (flipped to the left near the right edge).
A click outside closes it and the next frame is byte-identical to the frame before it opened. Clicking the first row
("Change wallpaper") closed the menu but produced no visible change and no preference change, twice, and one run launched an app
(Chats) that nobody asked for. [Raw log](logs/2026-11-18-de-shell-menu.txt)

## Interpretation

Opening, placement, drawing and outside-dismissal work. Whether row activation works is **unknown**: either the click was not
treated as a hit on the row, or the action ran with no effect. The unexpected app launch suggests that a pointer click on the shell
reaches the touch-tap path somewhere, which the finding about virtual pointers says it should not; this is a guess. Separately,
screencopy (`composite_output`) and the renderer disagree about what to show when the foreground is "app" but no window exists,
which produced a dark capture while the screen showed the home page.
None of the popup work for client apps (Chrome, Qt, GTK menus) was exercised. Status stays `hypothesis`.

## Reproduce

Open the menu by right-click, take a frame with `grim -o DSI-1`, click outside and compare hashes. To investigate row activation, add
a debug line in `menu_press_at` (position, hit result, action id) and rerun; compare with how a pointer click reaches `on_shell_tap`.
