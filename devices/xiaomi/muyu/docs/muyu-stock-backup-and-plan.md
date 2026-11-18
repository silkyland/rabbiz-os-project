# Xiaomi Pad 7 Pro (`muyu` / `SM8635`) — Stock Backup Notes & Phase 2 Plan

Last updated: 2026-11-15

---

## 1. Verified Device State (Captured via ADB `ac9c8ddc`)

- **Model:** `24091RPADG`
- **Codename / Variant:** `muyu` (`muyu_global`)
- **SoC / Platform:** `SM8635` (Snapdragon 8s Gen 3, `ro.board.platform = pineapple`, `ro.boot.hardware = qcom`)
- **OS / Build:** Android `16` — HyperOS `OS3.0.303.0.WOYMIXM`
- **Build Fingerprint:** `Xiaomi/muyu_global/muyu:16/BP2A.250605.031.A3/OS3.0.303.0.WOYMIXM:user/release-keys`
- **Bootloader & AVB State:** `ro.boot.flash.locked = 1`, `ro.boot.vbmeta.device_state = locked`
- **Partition Layout:** A/B (`ro.boot.slot_suffix = _b`), Dynamic Partitions (`ro.boot.dynamic_partitions = true`), `ro.crypto.state = encrypted`

---

## 2. Stock ROM Backup (Excluded from Git)

Large binary images are stored locally under `backups/xiaomi-pad-7-pro/` (gitignored via `.gitignore`).

### Official Fastboot ROM (Verified Local Backup)
- **Filename:** `muyu_global_images_OS3.0.303.0.WOYMIXM_20260625.0000.00_16.0_global_59ef5e03ea.tgz`
- **Size:** `8234507296` bytes (~7.67 GiB)
- **MD5:** `59ef5e03ea18505083c3da1aa57c4752` (verified `OK`)
- **Local Path:** `backups/xiaomi-pad-7-pro/stock-rom/`
- **Re-download URLs (Official Xiaomi Mirrors):**
  - `https://bkt-sgp-miui-ota-update-alisgp.oss-ap-southeast-1.aliyuncs.com/OS3.0.303.0.WOYMIXM/muyu_global_images_OS3.0.303.0.WOYMIXM_20260625.0000.00_16.0_global_59ef5e03ea.tgz`
  - `https://cdnorg.d.miui.com/OS3.0.303.0.WOYMIXM/muyu_global_images_OS3.0.303.0.WOYMIXM_20260625.0000.00_16.0_global_59ef5e03ea.tgz`

### Device-Specific Partitions (Pending Unlock + Root)
Because the bootloader is currently locked (`ro.boot.flash.locked = 1`), `adb` cannot dump `/dev/block/by-name/*` directly. Once unlocked and temporarily rooted, re-run:
```bash
./scripts/backup_muyu_stock.sh
```
This will dump device-unique calibration/EFS partitions (`persist`, `modemst1`, `modemst2`, `fsg`, `fsc`) plus slot `boot`/`init_boot`/`vendor_boot`/`dtbo`/`vbmeta` into `backups/xiaomi-pad-7-pro/stock-rom/partitions/` with `SHA256SUMS`.

> **Important:** `persist`, `modemst1`, `modemst2`, `fsg`, and `fsc` are **not** included in the Fastboot ROM `.tgz`. Back them up immediately after unlocking the bootloader and before flashing any custom kernel/Linux rootfs.

---

## 3. Phase 2 Porting Plan (`M4 (R1)`)

References: [ROADMAP.md](../../../../ROADMAP.md#L34-L39), [docs/PLAN.md](../../../../docs/PLAN.md#L119-L125).

1. **Step 0 — Stock ROM & Device Metadata Backup (Done)**
   - Captured `getprop` / `by-name` metadata and verified official `OS3.0.303.0.WOYMIXM` Fastboot `.tgz` locally.
2. **Step 1 — Rootless Baseline on HyperOS (No Unlock Required)**
   - Optionally validate Adreno 735 + Mesa Turnip performance using rootless `DroidDeck` while waiting for bootloader unlock quota.
3. **Step 2 — HyperOS Bootloader Unlock (`M4-R1-T1`)**
   - Apply for HyperOS bootloader unlock permission via Xiaomi Community, bind Mi Account in Developer Options (`Mi Unlock status`), and unlock `ac9c8ddc` via `fastboot` / Mi Unlock.
   - **Note:** Unlocking wipes userdata.
   - Immediately after unlock + temporary root (e.g., Magisk-patched `init_boot` from the backed-up `.tgz`), run `./scripts/backup_muyu_stock.sh` to dump `persist` and EFS partitions.
4. **Step 3 — Native Linux / `Zaviya` Bring-up (`M4-R1-T2`)**
   - Port/adapt `hashtagbasit/Zaviya` kernel/DTB/Turnip stack for `SM8635` (`muyu`).
   - Test boot via `fastboot boot` (non-destructive) before flashing `boot_a`/`boot_b` or `vendor_boot`.
5. **Step 4 — Stock Recovery / Unbrick Path**
   - Extract `backups/xiaomi-pad-7-pro/stock-rom/muyu_global_images_OS3.0.303.0.WOYMIXM_20260625.0000.00_16.0_global_59ef5e03ea.tgz` and flash via `flash_all.sh` (or `flash_all_except_storage.sh`) in Fastboot mode (`Vol- + Power`).
   - Do **not** run `flash_all_lock.sh` while custom images are present (re-locking on non-stock images bricks the device).

---

## 4. Community Findings & Existing Shortcuts (Researched 2026-11-15)

1. **Official Kernel & LineageOS Trees for `muyu`:**
   - `MiCode/Xiaomi_Kernel_OpenSource` (branch `muyu-v-oss`) — official Xiaomi OSS kernel drop for `muyu`.
   - `Xiaomi-Pad-7-Pro-Resources` (`https://github.com/Xiaomi-Pad-7-Pro-Resources`) — maintained LineageOS `23.2` / `24.0` trees and GKI `6.1` LTS kernel manifest (`android_device_xiaomi_muyu`, `android_device_xiaomi_sm8635-common`, `android_vendor_xiaomi_muyu`, `android_vendor_xiaomi_sm8635-common`, `android_kernel_xiaomi_sm8635`, `kernel_manifest` `common-android14-6.1-lts`, `auto`).
2. **Kexec / Multi-OS Boot Menu on `SM8635`:**
   - `dimivelev/peridot-multiboot` (`https://github.com/dimivelev/peridot-multiboot`) implements a pre-init ramdisk boot menu + KEXEC-enabled GKI kernel for `peridot` (`SM8635`), allowing internal multi-OS boot (Android + Linux) without writing a custom ABL bootloader.
3. **Mainline Linux / Nura (formerly postmarketOS) Status:**
   - Nura Wiki (`nura.eco`) tracks `muyu` under the `palawan` (`SM8635` / `SM7675`) platform. Although `SM8635` shares CPU traits with `SM8650`, its PMIC (`PM7550` / `PMXR2230`), GPU, and LLCC differ — `SM8650` DTBs cannot be reused directly.
4. **`hashtagbasit/Zaviya` Coverage Gap:**
   - `Zaviya` (`https://github.com/hashtagbasit/Zaviya`) currently ships prebuilt images only for `SM8750`, `SM8650`, `SM8550`, and `SM8250` (via `OpenABL` + microSD). There is no prebuilt `SM8635` / `muyu` image yet, and Pad 7 Pro has no microSD slot — adapt `Zaviya`'s ARM64 SteamOS/FEX/Proton userspace onto an internal `SM8635` Kexec/multi-boot setup instead.
5. **HyperOS Global Bootloader Unlock Notes:**
   - `muyu_global` qualifies for official Xiaomi Community unlock (account >= 30 days, OEM Unlocking + Mi Unlock Status binding, quota resets at `00:00 GMT+8`). Unlocking wipes userdata.
