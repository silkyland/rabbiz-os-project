# A21s BLE Gamepad Findings (2026-11-14)

Why `steam -gamepadui` could not pair Bluetooth devices, what was fixed in userspace, why an interim `uinput` bridge exists, and the kernel config set needed to remove it.

Related: [a21s-wifi-bluetooth.md](a21s-wifi-bluetooth.md) §6–7, `scripts/xbox-ble-uinput.{py,service}`.

---

## 1. Root causes found (in order)

| # | Finding | Fix |
|---|---|---|
| 1 | `usb0` static `Gateway=172.16.42.2` (metric 0) shadowed `wlan0` DHCP route (600) whenever the RNDIS host was absent → no internet → `pacman` dead | `10-usb0.network` now uses `[Route] Metric=700`; `wlan0` wins, USB NAT = fallback |
| 2 | No BlueZ userspace at all — `hci0` existed (`rfkill` unblocked) but no `bluetoothd`, no `bluetooth.service`, no `bluetoothctl`. Steam's pairing UI talks to `org.bluez` on the **system** bus | Installed `bluez` `5.88`, `bluez-utils`, `bluez-deprecated-tools`; enabled `bluetooth.service`; `power on` / `discoverable on` / `pairable on` |
| 3 | `bluetoothctl pair` → `org.bluez.Error.AuthenticationFailed` when no agent is registered | Pair with `agent KeyboardDisplay` + `default-agent` in the same session → `Paired/Bonded/Trusted: yes` |
| 4 | **`input-hog profile accept failed`** — kernel lacks `CONFIG_UHID` (`/dev/uhid` absent) and `CONFIG_HIDRAW`; BLE gamepads bond but get **no evdev node** | Stopgap `xbox-ble-uinput.py` (below); real fix = kernel rebuild (§4) |

## 2. Xbox Wireless Controller (`<controller-mac>`) — verified

- `modalias usb:v045Ep0B13d0523`, `Icon: input-gaming`, `Appearance: 0x03c4`, `LegacyPairing: no`, Battery service `0x180f` (50%).
- HID service `0x1812` = `service0016`; chars: `char001b` Report Map `0x2a4b`, `char001d` Report `0x2a4d` + CCCD + Report Ref `[0x01,0x01]` (**input, ID 1**), `char0021` Report Ref `[0x03,0x02]` (**output, ID 3** = rumble/PID, unused so far), `char0019` HID Control Point `0x2a4c`, `char0017` HID Info `0x2a4a`. No `0x2a4e` Protocol Mode.
- **Input report = 16-byte payload, no report-ID prefix** on the notification:
  ```
  off 0-3   LX, LY      u16 LE each (idle ≈ 0x8300)
  off 4-7   RX, RY      u16 LE each
  off 8-9   LT          10-bit + 6 pad   off 10-11 RT  10-bit + 6 pad
  off 12    lo nibble   hat: 0=null, 1-8 = N,NE,E,SE,S,SW,W,NW
  off 13-14 buttons     15-bit: A B X Y LB RB View Menu Xbox LS RS (+4 unused)
  off 15    bit0        consumer = Share
  ```
  Live sample (idle): `67 83 65 82 8a 83 dc 7f 00…00` → LX=0x8367 LY=0x8265 RX=0x838a RY=0x7fdc.

## 3. Stopgap: `scripts/xbox-ble-uinput.py` (running as `xbox-ble-uinput.service`)

- `input-hog` needs `CONFIG_UHID`, which is absent; but `CONFIG_INPUT_UINPUT=y` provides `/dev/uinput`. Bridge reads Report-ID-1 notifications via BlueZ D-Bus (`StartNotify` on `char001d`) and re-emits through uinput as `BUS_BLUETOOTH 045E:0B13 "Xbox Wireless Controller"` → `/dev/input/event3`. Live axis values confirmed end-to-end; 15 s `Connect()` tick auto-reconnects the trusted pad.
- Limitations: one hardcoded MAC; Xbox layout only; no rumble/LED/battery node; per-report D-Bus+Python overhead. **Must be removed** once `input-hog` works, or the pad registers twice. Cleanup commands in [a21s-wifi-bluetooth.md §7](a21s-wifi-bluetooth.md).
- Note: `fuzz=16` on stick axes + input-core duplicate suppression → an idle pad produces zero evdev events (normal; verify with `evdev absinfo`, not an event listener).

## 4. Kernel fix — pending on the **other build machine** (`src/`+`build/` are empty on this Mac)

```
CONFIG_UHID=y            CONFIG_HIDRAW=y          CONFIG_INPUT_JOYDEV=y
CONFIG_BT_HIDP=m         (already)                 CONFIG_HID_MICROSOFT=m
CONFIG_HID_PLAYSTATION=m CONFIG_HID_NINTENDO=m     CONFIG_HID_STEAM=m
CONFIG_XPAD=m            (optional, USB-OTG pads)
```

- `CONFIG_KEXEC=y` is enabled → test a new `Image` via `kexec -l/-e` before flashing `boot-avb.img`.
- With `UHID`, BlueZ `input-hog` handles **any** BLE HID pad automatically (multi-controller, rumble, battery, per-vendor drivers) — the entire goal for gamepads 2-4.

## 5. BT audio prepared alongside

`pipewire-audio` (ships `/usr/lib/spa-0.2/bluez5/`: SBC/AAC/aptX/LDAC/HFP codecs), `pipewire-pulse`, `wireplumber` installed; `wireplumber.service` + `pipewire-pulse.socket` started in the `gamer` user manager (`systemctl --user -M gamer@`). A2DP/HFP sink nodes will appear when an audio device connects.

## 6. Status / Open items

- [x] Kernel rebuilt & flashed (`7.3.0-g14a425b3a7dc-dirty` `#4`) with `CONFIG_UHID=y`, `CONFIG_HIDRAW=y`, `CONFIG_INPUT_JOYDEV=y`, `CONFIG_BT_HIDP=m`, `CONFIG_HID_{MICROSOFT,PLAYSTATION,NINTENDO,STEAM}=m`, `CONFIG_JOYSTICK_XPAD=m`; `/dev/uhid` present and `xbox-ble-uinput.service` stopgap removed.
- [ ] Verify multi-controller BLE pairing (Xbox / DualSense / Switch) natively via BlueZ `input-hog` + Steam UI.
- [ ] Classic (BR/EDR) pads need `modprobe hidp` (`CONFIG_BT_HIDP=m` present).
