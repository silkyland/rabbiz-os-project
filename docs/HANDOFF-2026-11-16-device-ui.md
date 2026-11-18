# Device/UI handoff — 2026-11-16, 18:10 +0700

User is travelling home and explicitly deferred the physical charging-cable
notification test. Preserve this work and resume that test when convenient.
Do not treat charging UX as hardware-verified.

## Completed and verified

- **USB:** high-speed still loses the link, 67–78 times/minute. PHY/LPM
  quirks failed. The retained 0008 patch limits USB to **full-speed 12 Mbps**.
  Two complete five-minute windows passed, including one after reboot with
  no LPM quirks. USB transferred a 37.7 MB image with matching SHA256.
  Root cause of high-speed failure remains unresolved; occasional SSH
  connection attempts still need retry. See `a21s-usb-flapping.md`.
- **Hold menu:** Vol Down + Power held three seconds opens Power menu.
  Labwc Power now runs on release, preventing repeat-driven flicker. Combos
  require keys held together; released-key history no longer makes lone
  Vol Up take screenshots. Physical tests passed, confirmed by the owner.
  Four actual-loop regression tests and six HoldMenu tests pass.
- **Battery:** moved FG from wrong hsi2c_0 (138a0000) to vendor-verified
  i2c_0 (13830000). Kernel and UPower report 43%; installed Quickshell
  reports 0.43. BatteryMeter was deployed and visually verified at 43%.
  Average voltage/current channels and calibration remain unvalidated.
- **Brightness:** saves the user's level separately from temporary zero
  (screen off) and screen-torch brightness. Hardware reboot test passed:
  forced raw zero, rebooted, restored saved 64% (raw 163/255). Restored the
  pre-test choice **76%**, saved and applied as raw **194/255**.
- **Queue:** AGENTS.md and DEVICE-QUEUE.md lock suspension was reverted
  per the original handoff. Use `scripts/device_session.sh` for device work.

## Charging feedback: implementation present, physical test pending

`Services/Power.qml` polls fresh gauge/UDC state every two seconds.
`ChargingEvents.js` debounces transitions and suppresses boot/reload spam.
The native notification uses the existing status-bar capsule (4.2 s) and
notification history, and is closed on disconnection. It reports Charging,
Fully charged, or Power connected, with percentage; it invents no fast-charge
or remaining-time estimate. Notifications may show over an active app and
retain the existing DND behavior.

These files were deployed and the shell loaded without QML errors. Host
charging-event tests passed. The physical unplug/replug test was requested
but the owner deferred it while travelling; it has **not passed yet**.

Next test:

1. Observe UDC state and battery status before/during/after cable removal.
2. Unplug for at least six seconds, reconnect and wait five seconds.
3. Verify one capsule, one Devices notification, correct percentage,
   dismissal after unplug, and no duplicates while steadily connected.
4. Check `/tmp/a21s-power-shell.log` for `Power notification posted` or
   notification errors. D-Bus session address for gamer is
   `unix:path=/run/user/1001/bus`.

Important uncertainty: the PHY forces VBUS valid without a role switch.
Verify whether the UDC actually leaves configured state on physical unplug;
if it stays configured, the current detector needs correction before claiming
that cable notification works. Gauge status alone flips Charging/Discharging
under load even with USB attached, so it must not be the sole USB trigger.
A bounded observer was saved at `/tmp/a21s-power-trace.py` (90 s, no grab);
its log is `/tmp/a21s-power-trace.log`. It may have expired before the test.

## Added user requirement — charging icon and animation (not implemented)

When a charging cable is connected:

- Show a **plug icon in the centre of BatteryMeter**.
- Animate the bars **`| | | |` in a continuous loop** inside the battery.
- Stop the animation and restore the normal level indicator after unplug.
- Keep the charging notification as well; the animation is local to the
  battery icon, not a whole-screen flash.
- Use the stable cable/charging state verified above, existing Theme tokens,
  and the existing BatteryMeter component. Do not enable GPU globally.

The existing BatteryMeter currently has a static charging bolt and level
fill; the requested plug + looping bars are the next UI change.

## Current image and access

Latest validated boot image: `build/images/boot-avb.img` (also
`boot-battery-bus.img`), SHA256:
`6bd0a485c8d69beeadcd771bc7ecf358383bbb8f284609f3889d6c0f1296d1a0`.
This was flashed with matching readback. The canonical boot-avb.img was
updated to this validated image at the end of the session.

Prior image backup: `build/images/boot-before-usb-87d6a511.img` SHA256
`87d6a5113d9f5316f48113e0768496f496ba3709cda8bdb16198c6404c074712`.
Stock backup remains `build/images/stock/boot-current.img` SHA256
`d222439485ab83a887c31ff43c01930dc035b562ff4c76f2ff157eb97ba6c7d0`.
An additional old boot backup is `/var/tmp/a21s-boot-before-usb.img` on device.
Never write /lib; only mmcblk0p18 is the boot partition.

Last boot ID after brightness verification:
`aa605b13-49d9-4b86-8dff-b99f647c64a1`.
USB SSH: root@172.16.42.1. Office Wi-Fi was <wifi-ip>; it will change at
home. Resolve a21s-steamos.local and verify ED25519 fingerprint
`SHA256:QcOfF+LZQcizFlUafNNCN3TOIvS+NZiWaxcc2PVDjhY`; use
`-o HostKeyAlias=172.16.42.1` for Wi-Fi. Never print ~/.credentail.

## Commits from this work

- 71ca807 — 0003 DTS patch hunk count.
- 6f42762 — verified USB full-speed workaround and measurements.
- 0ef0122 — hold menu, repeat/flicker fix, false screenshot fix and tests.
- 28a545e — fuel-gauge bus correction and hardware findings.
- 3bd07eb — proportional BatteryMeter.
- 4646caf — persistent brightness, device probe and reboot evidence.

Other chats are editing this same checkout/device. Do not stage or deploy
unrelated changes. In particular, the current StatusBar worktree also has
another task's DEMO-badge removal, and scripts/check.sh has another task's
appcatalog test and executable-bit change. Preserve those when staging the
charging feature. The HTTP image-transfer servers from this session were
stopped. Build logs/probes/screenshots are under build/usb-investigation/.


## Repository snapshot requested before travel

The owner requested committing and pushing all remaining source, assets and
findings, including unfinished work. Included in the snapshot:

- RabbizOS/Rabbiz brand assets and notes; desktop icons, greeting card and
  preferred-app/deduplication policy with its host tests.
- Native Rabbiz Mirror Swift sources, icons, build/setup/verification scripts
  and the existing mirror findings. `swift build -c release --package-path
  tools/a21s-mirror` passed in this session; no new hardware mirror test was
  run while the owner was travelling.
- `docs/PLAN-keyboard.md`: retained as **plan only**, with its verified,
  unverified and decision tags. `tools/a21s-osk/spikes/s2-render/Cargo.toml`
  is only a dependency manifest: no Rust target/source exists yet, so the
  spike is not runnable and is not represented as implemented.
- Browser benchmark HTML and shell performance comparison tooling; their
  presence is not evidence of new benchmark results.

Host `scripts/check.sh` passed before this snapshot. Candidate text files
were scanned for secret patterns with no matches. Swift build products,
device package caches and Python bytecode are excluded. The stray root
`-I` file was a generated qmllint JSON report, preserved locally at
`build/usb-investigation/qmllint-report.json` rather than shipped as source.

Still pending: physical charging notification verification, centre plug icon
and looping charging bars, high-speed USB root cause, fuel-gauge average
channels/calibration, and the keyboard plan's spikes/implementation.
