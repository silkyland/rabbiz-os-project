# A21s `boot-avb.img` flash procedure (rehearsal draft)

Carries kernel patches **0006-tail** (fuel-gauge rename), **0009**
(thermal zones) and **0010** (DSI underrun link recovery) in one boot.
Everything here is staged so the only irreversible step — the `dd` into
`/dev/mmcblk0p18` — happens last, with a fresh backup and a rehearsed
recovery. **Owner approval is required for phase D.**

## Phase 0 — prerequisites (all must be true before starting)

- [ ] Docker volume `a21s-kbuild` present; tree at `14a425b3a` with
      patches 0005/0006/0009/0010 applied (verified 2026-11-18: dts has
      thermal zones, dsi driver has `underrun_work`).
- [ ] **`mkbootimg` resolved** — it is NOT in `a21s-kimg` or
      `rabbizos-kbuild` and not on the host (checked 2026-11-18).
      Options: `apt-get install -y mkbootimg` inside the Debian kbuild
      container (needs net), or fetch AOSP `mkbootimg.py` pinned to the
      same major version the previous flash used, or restore the
      `a21s-build` image named in the recovery doc.
- [ ] `avbtool` → `/work/avb/avbtool.py` (present in volume).
- [ ] Heimdall binary ready BEFORE flashing: brew cask is disabled —
      build from source per
      [a21s-boot-recovery.md](a21s-boot-recovery.md) (framework flags)
      or have a Linux box with `heimdall-flash` on USB-OTG standby.
- [ ] Battery ≥ 50 %, USB cable seated, Wi-Fi SSH confirmed
      (`root@192.168.50.6`) AND usb0 SSH (`root@172.16.42.1`).
- [ ] DE lane notified — the flash + reboot kills its session and holds
      `/run/lock/a21s-device-test.lock` the whole time.

## Phase A — build (host, no device)

```sh
# 1. Kernel Image + dtbs in the kbuild container
docker run --rm -v a21s-kbuild:/work -w /work/linux \
  rabbizos-kbuild:usb-investigation \
  make -C /work/linux O=/work/out ARCH=arm64 LLVM=1 -j4 Image dtbs

# 2. Refresh the uniLoader blobs — the DTB that RUNS is embedded in
#    uniLoader, not the mkbootimg --dtb field (that is the stock table)
cp /work/out/arch/arm64/boot/Image                  -> /work/uniLoader/blob/Image
cp /work/out/arch/arm64/boot/dts/exynos/exynos850-a217f.dtb \
                                                    -> /work/uniLoader/blob/dtb
#    ramdisk blob already in place (1,785,250 B — matches recovery doc)

# 3. Build uniLoader (a21s_defconfig, UNCOMPRESSED output: uniLoader)
#    per its own README/Makefile in /work/uniLoader
```

## Phase B — backup (device lock)

```sh
./scripts/device_session.sh root@172.16.42.1 <<'EOF'
dd if=/dev/mmcblk0p18 of=/root/p18-pre-flash.img bs=4M
ls -l /root/p18-pre-flash.img   # MUST be exactly 37748736 bytes
md5sum /root/p18-pre-flash.img
EOF
scp root@172.16.42.1:/root/p18-pre-flash.img /tmp/a21s-p18-backup.img
# Keep BOTH copies. /root/ copy survives the flash itself.
```

## Phase C — repack (container)

```sh
# 4. Extract the stock android_dt_table from the BACKUP image:
#    parse mkbootimg v2 header -> dtb_size/dtb_addr; carve the dtb
#    section (see a21s-boot-recovery.md table). ~28 KB, magic 0xd7b7ab1e.

# 5. Repack — exact recipe from the recovery doc:
mkbootimg --kernel /work/uniLoader/uniLoader \
  --ramdisk /work/uniLoader/blob/ramdisk \
  --cmdline "quiet splash plymouth.ignore-serial-consoles \
    plymouth.prefer-fbcon pmos_boot_uuid=a0d6a667-e3a0-43f4-9c73-5eaa0005aa19 \
    pmos_root_uuid=2c51b76a-9803-456e-961d-e05ff8403bd2 \
    pmos_rootfsopts=defaults" \
  --base 0x10000000 --kernel_offset 0x8000 --ramdisk_offset 0x01000000 \
  --tags_offset 0x100 --pagesize 2048 --header_version 2 \
  --dtb <extracted-dt_table> --dtb_offset 0x0 -o /tmp/boot-avb-new.img
python3 -c "d=bytearray(open('/tmp/boot-avb-new.img','rb').read()); \
  d[44:48]=bytes.fromhex('85010018'); \
  open('/tmp/boot-avb-new.img','wb').write(d)"          # os_version
python3 /work/avb/avbtool.py add_hash_footer --image /tmp/boot-avb-new.img \
  --partition_name boot --partition_size 37748736 --rollback_index 12

# 6. Self-check BEFORE flashing:
python3 /work/avb/avbtool.py info_image --image /tmp/boot-avb-new.img
#    must show AVB0 vbmeta + AVBf footer, partition_size 37748736.
#    Image WITHOUT AVBf goes straight to Download Mode.
#    size(boot-avb-new.img) <= 37748736.
```

## Phase D — flash + verify (OWNER APPROVAL REQUIRED)

```sh
scp /tmp/boot-avb-new.img root@172.16.42.1:/root/
./scripts/device_session.sh root@172.16.42.1 <<'EOF'
dd if=/root/boot-avb-new.img of=/dev/mmcblk0p18 bs=4M && sync
systemctl reboot
EOF
```

Wait ~90 s, then verify on `root@172.16.42.1` (usb0) or Wi-Fi:

```sh
uname -a                                            # new build
ls /sys/class/thermal/                              # 0009: zones exist
for z in /sys/class/thermal/thermal_zone*; do
  cat $z/type $z/temp; done
cat /sys/class/power_supply/*/type                  # 0006: s2mu106-fuel-gauge
ls /sys/class/power_supply/s2mu106-fuel-gauge/ | grep -c _avg || echo "no bogus avg props"
dmesg | grep -iE "underrun|dsim"                    # 0010: recovery worker armed
```

Then the 0010 acceptance test under the lock: ten compositor stop/start
cycles, watching for `underrun: restarting video link` and a live panel.

## Phase E — recovery runbook

| Failure | Recovery |
|---|---|
| Black screen, no ssh (kernel boots dead) | `dd` the backup back: still under ssh? `dd if=/root/p18-pre-flash.img of=/dev/mmcblk0p18 && reboot`. No ssh at all → download mode (hold VolUp+VolDown, insert USB) → `heimdall flash --BOOT /tmp/a21s-p18-backup.img --no-reboot`; `close-pc-screen` fails on this device — exit download mode with Power+VolDown ~10 s. |
| Straight to Download Mode on boot | AVB/footer wrong — re-check phase C step 6, heimdall the backup. |
| sshd rejects key after restore | Expected quirk — root password login, re-append `id_ed25519.pub` to `/root/.ssh/authorized_keys`. |
| Total loss | Stock `SM-A217F` AP tar via heimdall (owner keeps it). |

## Known unknowns going in

- The same code kexec'd cleanly to a black screen once (built under
  clang 19 vs flashed clang 23.1.5). If this flash also blacks out, the
  suspect list is: clang-version-sensitive early init, or something in
  the container tree that isn't in the flashed tree — diff `/proc/config.gz`
  against `/work/out/.config` post-boot before suspecting the patches.
- Rollback index is pinned at 12 from the recovery doc's known-good;
  if Samsung anti-rollback bumped past that on the current image, the
  repack needs the newer index — read it from the backup's AVB footer
  (`avbtool info_image /tmp/a21s-p18-backup.img`) and use that value.
