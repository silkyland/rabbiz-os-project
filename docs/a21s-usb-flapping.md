# A21s USB flapping investigation — 2026-11-16

Status: **full-speed workaround verified; high-speed root cause unverified**. Acceptance: zero macOS
`hardware connection lost` events for five minutes with the cable attached.

## Current evidence

Device identity: SSH ED25519
`SHA256:QcOfF+LZQcizFlUafNNCN3TOIvS+NZiWaxcc2PVDjhY`,
Wi-Fi `192.168.50.6`. Running kernel reports
`7.3.0-g14a425b3a7dc-dirty`, built `Mon Nov 16 09:18:46 UTC 2026`.
Boot ID read at device-local 17:09:08:
`7fcce380-a01f-472c-8c96-a68aae6e6fbb` (uptime nine minutes).

| Observation window (Mac clock, +0700) | Lost connections | Conditions |
| --- | ---: | --- |
| 17:06:55–17:07:55 (60 s query) | 73 | No device mutation by this investigation; prior handoff cleared PHY bits |
| 17:08:55–17:09:55 (60 s query) | 67 | Register read at 17:09:08 confirms `GUSB2PHYCFG=0x40102400` |
| 17:09:56–17:10:56 (60 s query) | 72 | PHY restored to `0x40102540` before window |

At 17:09:08 the UDC reported `configured`; networkd had recorded 260
`usb0: Lost carrier` events this boot. The physical register at
`0x1360c200` had both SUSPHY (bit 6) and ENBLSLPM (bit 8) clear.
Thus clearing those two bits alone **does not prevent flapping** in this
boot. Restoring the original PHY value gave 72 events/minute, within the same observed range.
Raw local captures: `build/usb-investigation/{baseline-cleared-phy,second-cleared-phy}.log`.

The handoff reports download mode stable on the same cable/port and device
traces cycling Suspend/U3 → Reset → Connection Done; these were not
independently repeated in this session.

## Reproduction

Leave the Linux gadget attached to the same Mac port for at least 60 seconds.
Then run on macOS:

```sh
/usr/bin/log show --last 60s --style compact --predicate \
  'process == "kernel" AND eventMessage CONTAINS "Samsung Galaxy A21s" AND eventMessage CONTAINS "hardware connection lost"'
```

Count only event lines (exclude the header). Example failing output:

```text
2026-11-16 17:08:55.978 Df kernel[0:8c1] (IOUSBHostFamily) pcie-xhci-hs-port4@08400000: AppleUSBHostPort::terminateDevice: destroying 0x18d1/d001/0703 (Samsung Galaxy A21s): hardware connection lost
```

This is a rate-based reproduction. Cable, gadget, and host are required;
a host-only unit test cannot execute the PHY path. No traffic generator,
touch injection, or shell deployment is needed to observe the failures.

## Code path verified in Docker volume a21s-kbuild

- `arch/arm64/boot/dts/exynos/exynos850.dtsi:955–972`: Exynos glue
  `usb@13600000` contains `usbdrd_dwc3: usb@0`.
- Board DTS `exynos850-a217f.dts:534–537`: OTG, maximum high-speed.
- `drivers/usb/dwc3/core.c:755–761,1833–1836`:
  `snps,dis_u2_susphy_quirk` controls SUSPHY;
  **`snps,dis_enblslpm_quirk` controls ENBLSLPM**.
- `drivers/usb/dwc3/gadget.c:4339–4374,4898`:
  `snps,usb2-gadget-lpm-disable` separately disables advertised USB2 LPM
  and DCFG.LPM_CAP. It does not clear GUSB2PHYCFG.ENBLSLPM.

## Remaining hypotheses

1. **UNVERIFIED:** advertised USB2 LPM / controller suspend-resume behavior.
   Clearing PHY bits is insufficient; inspect DCFG and runtime PM next.
2. **UNVERIFIED:** host USB probing influences the reset cycle. Mac logs
   show Chrome and ChatGPT competing for exclusive access. Those messages
   alone do not prove either app causes the hardware connection loss.
3. Queue-lock explanation is unsupported: flapping persists while this
   investigation takes no device lock. Other pre-existing SSH processes
   were present, so this is not a controlled all-clients-idle test.

SSH is intermittent even over Wi-Fi. Use bounded retries and verify host
identity. Do not build/flash a supposed fix solely from the PHY hypothesis.
Do not terminate another task's SSH holder. The owner explicitly requested
restoring queue instructions if the USB experiment still flapped; see below.

## USB2 LPM image experiment

The failed patch (archived as `build/usb-investigation/failed-lpm.patch`) added all three
properties to the board DWC3 child, testing the remaining LPM hypothesis.
The first two retain the already-tested PHY behavior; the third adds the
previously untested LPM capability change. Runtime PM was `on/active` and
configfs WebUSB `use=0` before flashing.

Build: restored tracked kernel source in the existing Docker volume after
saving its diff to `/work/pre-usb-experiment.diff`; applied all numbered
patches afresh, then built `Image dtbs` and uncompressed uniLoader.
The original config and stock ramdisk/dt_table were retained.
`avbtool verify_image` passed (NONE, boot sha256, rollback 12), and `fdtget`
confirmed all three properties in the embedded board DTB.

- Prior boot/readback: `87d6a5113d9f5316f48113e0768496f496ba3709cda8bdb16198c6404c074712`.
- Experimental image: `build/images/boot-usb-lpm-experiment.img`,
  SHA256 `36b130a226bb786266ec050487d079c786975c7ab696d5a1f9b203235274b5cc`.
- Size: 37,748,736 bytes. Original `build/images/boot-avb.img` retained.
- Evidence: `build/usb-investigation/{build,pack,verify}.log`.
  The initial verification command expected a sibling `boot.img`; verifying
  a temporary copy named `boot.img` succeeded. This was a tool filename
  requirement, not an image-integrity failure.

## Host identity control

At 17:17:54, temporarily unbound the gadget, changed only idVendor from
`18d1` to `0525`, then rebound the same NCM configuration. A full 60 s
window (17:18:08–17:19:08) still recorded **78 hardware connection losses**,
all with `0525/d001`. Thus avoiding the original Android vendor identity
did not prevent flapping. This does not rule out all host software behavior.
Capture: `build/usb-investigation/vendor-id-0525-full.log`.

## Transport issue

Wi-Fi SSH/SFTP connections frequently stall; a separate HTTP download was
used for the image. `ss -ti` shows TCP retransmissions. Temporary wlan0 MTU
1280 (from 1500) allowed transfer progress; causal improvement is not proven.
The MTU change is runtime-only and disappears on reboot. Never flash a
partial transfer: gunzip and verify the full image SHA256 first.

Host `scripts/check.sh` passed, including six hold-menu tests. Commit
`71ca807` contains only the corrected 0003 patch hunk count; the full kernel
patch series applied and built successfully before that commit.

Vendor ID was restored to `18d1` at 17:19:31. HTTP resume delivered the
complete compressed image; gunzip passed and on-device SHA256 matched
`36b130a2...b5cc`. Before flashing, copied the current boot partition to
`/var/tmp/a21s-boot-before-usb.img` and verified its known SHA256.
Flashed only `/dev/mmcblk0p18`; readback matched the experimental image.
`/var/tmp/a21s-usb-flash.log` ends with `FLASH_VERIFIED`.
A reboot was then requested for the experiment.

## LPM experiment result: failed

New kernel: build #2, timestamp `Mon Nov 16 10:12:08 UTC 2026`.
Boot ID `4fac678f-111e-4eb6-8709-f550845802dc`.
All three DT properties were present in the live device tree;
GUSB2PHYCFG read `0x40102400`, DCFG `0x00a00b28` (LPM_CAP bit 22 clear).
A fresh 60 s window 17:25:33–17:26:33 recorded **75 losses**.
Thus disabling PHY suspend and USB2 LPM **does not fix this failure**.
Acceptance failed; do not promote the experimental patch as a fix.
Capture: `build/usb-investigation/lpm-disabled-60s.log`.

The shell booted normally, but office Wi-Fi required the owner to reconnect
from Settings after reboot. SSH returned at `192.168.50.6` afterward.

The lock suspension was reverted per the owner's handoff after the USB
experiment still flapped. `AGENTS.md` and `docs/DEVICE-QUEUE.md` again
require the shared device lock. No lock script itself was changed.

## Full-speed control (in progress)

At 17:26:46, unbound configfs g1, set `max_speed=full-speed`, and rebound.
At 17:27:37 the device reported `current_speed=full-speed`, `configured`,
and usb0 carrier 1; the Mac acquired `172.16.42.2` on en18. USB SSH worked.
The initial five-minute observation passed; the boot test is recorded below.

The replacement `0008-a21s-usb-full-speed.patch` changes only the board's
`maximum-speed` from high-speed to full-speed. No LPM quirks remain.
Built DTB + uniLoader, packed and AVB-verified
`build/images/boot-usb-full-speed.img`, SHA256
`e807ed1932df870f7257c4752cd71ae1d14634ecbd86829b9854cd8ba2abc1ae`.
This limits theoretical USB signaling to 12 Mbps; high-speed remains an
unresolved issue even if the connectivity workaround passes.

First full-speed acceptance window **passed**: 17:26:50–17:31:50,
zero macOS hardware connection losses, including a successful 37,748,736-byte
USB SSH transfer with matching SHA256. Captured in
`build/usb-investigation/full-speed-five-min.log` (header only, no events).
The full-speed-only image was flashed and readback SHA256 matched; a second
reboot was requested to test persistence without any LPM quirks.

Full-speed-only boot **passed**: boot ID
`3cb3b270-6a59-4fae-814b-ba4f44fa9d9c`, live DT maximum-speed `full-speed`,
no LPM quirk properties, UDC full-speed, and USB SSH reachable after startup.
A second complete window **17:33:00–17:38:00** had zero hardware connection
losses (`full-speed-only-five-min.log`). This establishes that the LPM
quirks are unnecessary for this workaround. The board DTS speed limit is
the only retained USB patch. Do not claim that high-speed is repaired.
