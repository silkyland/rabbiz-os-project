# Bringing up a new device

Read this before you flash anything. RabbizOS runs on real phones and tablets; a wrong flash can
brick one. The first device is the Samsung Galaxy A21s ([devices/samsung/a21s/](../devices/samsung/a21s/));
Xiaomi Pad 7 Pro ([devices/xiaomi/muyu/](../devices/xiaomi/muyu/)) is planned. Use them as worked examples.

## 0. Report your device first

Open an issue with the **Device report** template: brand, marketing name, model number and
codename, SoC, bootloader status. Do this *before* flashing so we can tell you whether the SoC
has any mainline Linux support and which existing work you can reuse (for example another phone
on the same SoC can share `socs/<soc>/`). It also stops two people bringing up the same device
without knowing.

No device? You can still contribute in the `de` and `docs` lanes — see [CONTRIBUTING.md](../CONTRIBUTING.md).

## 1. Is it a good candidate?

Check these yourself; none is guaranteed by this project:

| Question | Why it matters |
|---|---|
| Can the bootloader be unlocked, and what does unlocking erase or void? | Without a way to boot your own kernel there is nothing to port. Unlocking commonly wipes data and may affect warranty or features. |
| Is there a documented download/recovery mode and a way to restore stock firmware? | This is your only recovery path. `device.toml [boot].recovery` must point at a written procedure. |
| Does the SoC boot a mainline Linux kernel on any board? | Decides whether you port drivers (kernel lane) or reuse existing ones. |
| Can you reach it without a screen — USB networking, serial, or a framebuffer log? | You will debug blind for a while. |
| Do you have a second device, or can you afford to lose this one? | Bricking is a real possibility. |

## 2. Collect device information safely

On a **stock** Android phone with USB debugging enabled you can use `scripts/probe_device.sh`
(needs `adb`). It reads model, hardware/SoC, Android version, CPU ABIs, RAM, kernel version,
SELinux state, GPU driver info and bootloader lock state into a text file.

- It does **not** read your serial number, IMEI or MAC address itself, but its last probe
  (`dumpsys deviceidle whitelist`) lists installed app package names. Delete that section before
  you post the file.
- It only works while Android runs; on a device already running something else, collect
  `uname -a`, `/proc/cpuinfo` and `lscpu` by hand.
- Never post serial numbers, IMEI, real MAC addresses, Wi-Fi names or IP addresses. Replace them
  with placeholders (`<serial>`, `<mac>`, `<wifi-ip>`).

## 3. Create the device directory

```bash
mkdir -p devices/<vendor>
cp -r devices/_template devices/<vendor>/<codename>      # lowercase; model numbers go in device.toml
```

Fill `device.toml` honestly: unknown stays `unknown`. A recovery procedure is **required** before
anyone flashes. Put your findings in `devices/<vendor>/<codename>/findings/` ([docs/FINDINGS.md](FINDINGS.md)).

## 4. Safety gates (do not skip)

1. **Back up stock** partitions before writing anything. The muyu plan shows the approach:
   [devices/xiaomi/muyu/docs/muyu-stock-backup-and-plan.md](../devices/xiaomi/muyu/docs/muyu-stock-backup-and-plan.md).
2. Write down the recovery procedure and, if you can, rehearse it once on a harmless partition.
3. Do not write bootloader, modem or vendor partitions unless the recovery path is documented and
   you cite it ([CONTRIBUTING.md](../CONTRIBUTING.md) §4).
4. Boot from a test image or memory first when the device allows it; flash to storage last.
5. Do not copy rules from the A21s `AGENTS.md` into your device as if they were true for yours;
   they are hypotheses until you have your own `verified` findings.

## 5. Bring-up order

Each step produces a finding (with the log) and updates `[status]` in `device.toml`. Stop and
write it up when you are stuck; a documented dead end helps the next person.

| Step | Goal | Typical evidence |
|---|---|---|
| 1 | A kernel boots and you can see output (framebuffer, serial or USB network) | boot log, `uname -r` |
| 2 | USB gadget networking to the host (SSH in) | `ip a`, SSH session |
| 3 | Display and backlight | photo or `dmesg` lines |
| 4 | Touch input | `evtest` output |
| 5 | Storage and a real root filesystem | `lsblk`, boot log |
| 6 | Wi-Fi, then Bluetooth | `nmcli device`, `hciconfig` |
| 7 | GPU, then audio, then battery/charging | tool output for each |
| 8 | Modem, sensors, camera | usually last; often vendor-specific |

The A21s history in [devices/samsung/a21s/docs/](../devices/samsung/a21s/docs/) is a long worked example, including
mistakes, such as the display wedge and the audio DSP buffer limit.

## 6. Send your work back

- Fork the repository and work on a branch of your fork. Keep device work in
  `devices/<vendor>/<codename>/` and, for drivers shared by a SoC, in `socs/<soc>/`.
- Open a PR using the template: lane (`kernel`/`userland`/`docs`), your device id, and the highest
  evidence level you reached (L2 needs the log). Say "Not tested on device" when that is the case.
- You become the listed maintainer of your device directory; ask for review from someone with the
  same SoC if one exists.
- You may keep a fork-only device tree if you prefer; the layout is designed so the device
  directory can live in its own fork without touching `core` code.
