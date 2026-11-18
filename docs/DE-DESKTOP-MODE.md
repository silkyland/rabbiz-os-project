# Making rabbiz-de a real desktop environment (design proposal)

Status: **design, mostly not built**. Lane: `de`. Written 2026-11-18. The owner's direction: the DE should
behave like a real **KDE or GNOME desktop**, with context menus in apps and a context menu on the empty desktop,
not only a phone launcher.

## 1. Where the code stands

Facts checked in the repository on 2026-11-18:
- Every toplevel is configured fullscreen with no decorations (`de_protocols::configure_fullscreen`). That is the
  handset policy; there are no windows to move, resize or switch between.
- A device profile carries `form_factor` (`handset`, `tablet`, `desktop`) but it is used only to scale motion timing.
- xdg popups (the surfaces apps use for context menus and drop-downs) are **tracked** by `PopupManager`, but
  `XdgShellHandler::grab` is a no-op (the code comment says the handset needs no interactive grab). Without a popup grab
  a client's menu cannot reliably take the keyboard and be dismissed by a click outside or by Escape.
- Whether popups are drawn at the right place, hit-tested for touch and pointer, and included in screenshots is **not
  verified**: the capture compositor (`composite_output`) walks toplevel windows only.
- The shell has a long-press gesture, drawer app actions and a lock/shade/power UI, but no general menu component and no
  menu on the empty home screen.
- The mouse and keyboard reach clients (libinput events are handled), but there is no visible cursor path and no
  hover feedback in the shell UI.

## 2. Principle

One compositor and one window model; the **shell layer** changes with the form factor. Handset behaviour stays exactly as
it is by default. The desktop form factor adds windows and panels; it is switched by the device profile and, later, by
docking (external display + pointer + keyboard).

## 3. Phases

### Phase A: context menus that actually work (first, useful on every form factor)
1. **Client menus:** implement xdg popup grabs (pointer and touch), keyboard focus to the popup, dismissal on outside
   click, touch outside and Escape, nested popups, positioner constraint solving against the output, correct stacking over a
   fullscreen window, drawing and hit-testing of popups, inclusion in captures.
2. **Shell menu component** in `de-ui`, data driven: items with icon, label, shortcut hint, separators, check and radio
   items, submenus, disabled items, keyboard and controller navigation, hover for the mouse, one motion style (the enter/exit
   tier from `DE-MOTION-M3.md`).
3. **Where menus appear:** right-click or long-press on an app icon (drawer, dock, desktop), on the **empty home screen**
   (the owner's "empty desktop" menu: change wallpaper, display settings, sort icons, add a widget later, desktop settings), on
   a notification, on a panel.
4. **Acceptance:** right-click opens a menu in a GTK app, a Qt app (the Files app) and Chrome; clicking outside or Escape
   closes it; submenus open; the same works from touch by long-press; a right-click on the empty home screen opens the
   shell menu. Each with a screenshot and a log as a finding.

### Phase B: windows (desktop form factor)
Server-side decorations drawn by the compositor (client-side ones respected when the client asks for them), move, resize
from edges and corners, maximize, minimize, fullscreen, close, window menu on the title bar, edge snapping, Alt+Tab
switcher, focus policy, minimum and maximum sizes, multi-window stacking. Handset keeps fullscreen windows.

### Phase C: the desktop shell
Panel with a launcher and running windows, an overview or application menu, system tray and indicators (StatusNotifier over
D-Bus), notification popups, quick settings as a popover, lock and session menu, virtual desktops, per-output wallpaper,
optional desktop icons, multi-monitor and hotplug.

### Phase D: convergence
Automatic switch between handset and desktop when an external display and a pointer are attached, per-output scale, windows
that survive the switch. Depends on the hardware: whether the A21s can output video at all is **not known**.

## 4. Decisions the owner has to make

1. **Which desktop feels like home:** GNOME-like (overview, top bar, no permanent taskbar) or KDE-like (panel with a
   launcher and a task list, many settings)? A default is needed before Phase C; Phase A and B do not depend on it.
2. Tiling or floating windows as the default in desktop mode.
3. Does desktop mode have to run on the A21s, or only on a tablet or docked setup? That decides whether Phase B and C
   are tested on real hardware soon.
4. Should the first deliverable be Phase A alone? It improves the phone too and is small compared with the rest.

## 5. Dependencies and risks
- Input: the device registry, a visible cursor, hover, scrolling and keyboard shortcuts come from the input package; menus
  need them first.
- Apps: GTK, Qt and Chrome menus must be tested, not assumed; some toolkits draw their own menus when a compositor lacks a
  protocol.
- Focus stealing (see [DE-PRIVILEGED-PROTOCOLS.md](DE-PRIVILEGED-PROTOCOLS.md)) must be decided before windows can coexist:
  with several windows, "which one gets the focus" becomes a security matter.
- The effort for Phases B and C is large; this document does not estimate it.
