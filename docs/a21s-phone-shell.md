# a21s-phone-shell — session bring-up (labwc + Quickshell on Galaxy A21s)

Android-like launcher DE running **bare-metal on mainline Linux** — replaces
the SteamOS session as the boot UI while leaving `steamos-session.service`
fully intact as a parallel, launchable-anytime unit. Architecture contract:
[shell/README.md](../shell/README.md). Stack: **labwc** (wlroots compositor,
layer-shell + XWayland) + **Quickshell 0.3.2** (shell surfaces) + standalone
**quickshell**-hosted QML apps + **gamescope**-nested Steam.

## How it is wired

```
systemd
 └─ a21s-phone-session.service        (User=gamer, PAMName=login,
    │                                 Conflicts=steamos-session.service,
    │                                 after seatd/dbus/NM/bluetooth)
    └─ /usr/local/bin/a21s-phone-session     (env setup, exec labwc)
         ├─ labwc  ── ~/.config/labwc/rc.xml
         │    • windowRule "*" → Maximize + serverDecoration=no
         │      → every app toplevel is fullscreen, Android single-tasking
         │    • touch: mapToOutput DSI-1 (720x1600 portrait), no naturalScroll
         │    • ~/.config/labwc/environment → session vars incl.
         │      QML_IMPORT_PATH=/usr/share/a21s-shell
         │    • ~/.config/labwc/autostart:
         │       ├─ dbus-update-activation-environment → user services
         │       ├─ quickshell -p /usr/share/a21s-shell/shell.qml   (shell)
         │       └─ squeekboard (OSK, only if installed)
         ├─ apps: /usr/local/bin/a21s-<app>
         │    └─ export QML_IMPORT_PATH + exec quickshell -p .../apps/<app>/<App>.qml
         └─ a21s-steam → gamescope -W 720 -H 1600 -f --steam -- steam ...
              (a normal maximized labwc window, not a separate session)
```

- `/usr/share/a21s-shell/` ← `shell/` (QML modules `A21s.Theme`, `A21s.Services`, `apps/`)
- `.desktop` files in `/usr/share/applications/` feed Quickshell's `DesktopEntries` → home-screen grid
- Seat/device access: `seatd` (`LIBSEAT_BACKEND=seatd`), `gamer` must be in `seat` group — the unit also sets `SupplementaryGroups=seat input video render audio` as a safety net
- `XDG_RUNTIME_DIR` is provided by `pam_systemd` via `PAMName=login`; the unit also sets it explicitly as a fallback

## Packages to install on the device

```sh
pacman -Sy --needed labwc quickshell qt6-declarative qt6-svg qt6-wayland \
  xorg-xwayland gamescope squeekboard noto-fonts ttf-nerd-fonts-symbols \
  upower brightnessctl
```

| Package | Status | Why |
|---|---|---|
| `labwc` | ALARM `extra` | compositor |
| `qt6-declarative` | OK | Qt/QML runtime pulled in by quickshell |
| `qt6-svg`, `qt6-wayland` | OK | icons + wayland QPA plugin |
| `xorg-xwayland` | OK | XWayland for gamescope/Steam |
| `gamescope` | OK in Arch `extra`; **verify aarch64 build** | nested Steam |
| `squeekboard` | OK in Arch; **verify ALARM** | Wayland OSK (input-method-v2, labwc supports it) |
| `noto-fonts` | OK | UI text |
| `ttf-nerd-fonts-symbols` | **UNVERIFIED on ALARM** | shell icon glyphs; fallback: drop any Nerd Font into `/usr/share/fonts/TTF/` |
| `quickshell` | OK — `extra/quickshell 0.3.2-1` aarch64 | verified on archlinuxarm.org |
| `upower` | OK | battery backend for `A21s.Services` |
| `brightnessctl` | OK | brightness keybinds/shade slider |
| `steam` | **not a package** | native ARM64 client already in `/home/gamer/.local/share/Steam` (see `docs/a21s-game-results.md`); `a21s-steam` assumes `steam` is on `PATH` |

## Deploy + switch

```sh
./scripts/deploy_shell.sh                 # → root@172.16.42.1
./scripts/deploy_shell.sh --packages      # just print the pacman line

# on device, one-time:
usermod -aG seat,input,video,render gamer

# switch to phone session (steamos-session stays installed):
systemctl disable steamos-session.service
systemctl enable --now a21s-phone-session.service

# switch back to SteamOS session any time:
systemctl disable a21s-phone-session.service
systemctl enable --now steamos-session.service
```

`Conflicts=steamos-session.service` guarantees only one session owns the
seat at a time — `enable --now`-ing one stops the other.

## Debugging

```sh
journalctl -u a21s-phone-session.service -b -f   # session/compositor log
loginctl                                          # gamer session + seat
swaymsg / labwc --version; ls /usr/share/a21s-shell
```

## Known caveats / open items

- **Steam orientation**: Big Picture was validated at 1600x720 *landscape*
  under Weston (`docs/a21s-game-results.md`). `a21s-steam` uses portrait
  `-W 720 -H 1600` to match the panel's native mode; if gamepadui renders
  badly, rotate the labwc output or flip gamescope to `-W 1600 -H 720`.
- **Modem/telephony**: no modem driver yet (M6) — Dialer/SMS run against
  `A21s.Services.Telephony` demo mode.
- **labwc config keys** (`<touch naturalScroll>`, `skipWindowSwitcher`,
  `OnScreenDisplay` font place) target current labwc (≥0.8); unknown keys
  are warned-and-ignored, not fatal.
- `brightnessctl`/backlight and battery may report unavailable until the
  M5 power drivers land — shell degrades gracefully by contract.
