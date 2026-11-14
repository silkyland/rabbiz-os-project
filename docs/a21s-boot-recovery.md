# A21s Boot Image Anatomy & Download-Mode Recovery

Hard-won findings from the 2025-11-14 boot-image failure → recovery cycle.
Read this before packing or flashing `boot` on SM-A217F again.

## Boot image layout (verified against known-good `/tmp/boot-current.img`)

`mkbootimg` header v2, page 2048:

| Section | Content |
|---|---|
| header | `ANDROID!`, kernel_addr `0x10008000`, ramdisk_addr `0x11000000`, tags `0x10000100`, os_version `0x18000185`, cmdline `quiet splash plymouth... pmos_root_uuid=2c51b76a-...` |
| kernel | **uniLoader flat binary** (raw ARM64 code, NOT ELF/gzip). Contains embedded DTBs at `~0x1236fa0` (131-byte stub) and `~0x1c70000` (real board DTB, ~37.6 KB) |
| ramdisk | postmarketOS initramfs (~1.78 MB) |
| dtb section | **`android_dt_table` (magic `0xd7b7ab1e`, ~28 KB)** — extracted to `/tmp/boot-stock-dtb.bin`; REQUIRED: `mkbootimg --header_version 2` refuses to build without `--dtb` |
| tail | AVB: `AVB0` vbmeta (512 B) + `AVBf` footer. Hash descriptor: partition `boot`, sha256, `Algorithm: NONE`, rollback index 12. `partition_size = 37748736` (= `/dev/mmcblk0p18` = 73728 sectors) |

Key facts:

- The DTB Linux actually runs is embedded **inside uniLoader** (built from `src/uniLoader/blob/dtb`), NOT the mkbootimg `--dtb` field — that field holds the stock Samsung dt_table.
- Without vbmeta/footer the device drops straight to **Download Mode** (verified: first bad flash had no AVB → instant download mode).
- kexec is NOT usable on this device (hangs even with the running DTB). `CONFIG_OF_CONFIGFS`/runtime overlay not available.

## Repack recipe (exact, verified byte-comparable to original)

```bash
# inside a21s-build container; blobs in src/uniLoader/blob/
mkbootimg --kernel src/uniLoader/uniLoader --ramdisk src/uniLoader/blob/ramdisk \
  --cmdline "quiet splash plymouth.ignore-serial-consoles plymouth.prefer-fbcon \
    pmos_boot_uuid=a0d6a667-e3a0-43f4-9c73-5eaa0005aa19 \
    pmos_root_uuid=2c51b76a-9803-456e-961d-e05ff8403bd2 pmos_rootfsopts=defaults" \
  --base 0x10000000 --kernel_offset 0x8000 --ramdisk_offset 0x01000000 \
  --tags_offset 0x100 --pagesize 2048 --header_version 2 \
  --dtb <stock-dt_table> --dtb_offset 0x0 -o /tmp/boot.img
# patch os_version byte to match original: offset 44 = 85 01 00 18
python3 -c "d=bytearray(open('/tmp/boot.img','rb').read()); d[44:48]=bytes.fromhex('85010018'); open('/tmp/boot.img','wb').write(d)"
avbtool add_hash_footer --image /tmp/boot.img --partition_name boot \
  --partition_size 37748736 --rollback_index 12
```

## Download-Mode recovery

Homebrew `heimdall-suite` cask is disabled; build from source works with framework flags:

```bash
git clone https://gitlab.com/Heimdall/heimdall /tmp/heimdall && cd /tmp/heimdall && mkdir build && cd build
cmake .. -DCMAKE_EXE_LINKER_FLAGS="-framework CoreFoundation -framework IOKit -framework Security -lobjc"
make -j4   # binary: bin/heimdall
```

On Linux: `sudo apt install heimdall-flash`.

```bash
heimdall detect                                   # device in download mode
heimdall flash --BOOT /tmp/boot-current.img --no-reboot
# heimdall close-pc-screen fails on this device — user presses Power+VolDown ~10 s to boot
```

Known-good image backup: `/tmp/boot-current.img` (37,748,736 B) on the Mac; rebuild by `dd if=/dev/mmcblk0p18` when online.

## Current experiment state (handoff → Linux)

Flashed `boot-stockdtb.img` = **my rebuilt uniLoader + STOCK dtb** to isolate whether:

- boots → rebuild toolchain OK, **ABOX DTS is the boot killer** → bisect DTS: candidates `sysmmu_aud` (AUD power domain off at boot, `CONFIG_EXYNOS_IOMMU=n` anyway), `abox-gic`, `cmu_aud` clock refs. Plan: `status="disabled"` all ABOX nodes → re-enable in groups.
- download mode again → rebuilt uniLoader binary itself is bad → byte-compare vs original payload.

Images in `/tmp/` on Mac: `boot-current.img` (good), `boot-fixed.img` (abox DTB + AVB, FAILED), `boot-stockdtb.img` (test, flashed).

## Post-boot SSH quirk

After restore, sshd rejected `id_ed25519` — `authorized_keys` had only an old key. Root password login works; re-append `~/.ssh/id_ed25519.pub` to `/root/.ssh/authorized_keys`.

## Never forget

- `clk_ignore_unused` in bootargs — removing it resets the SoC on WLBT start.
- Flash only `/dev/mmcblk0p18` (verified size 37,748,736 B).
- Never write `/lib` on the rootfs (symlink → `usr/lib`).
