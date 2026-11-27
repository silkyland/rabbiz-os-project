#!/usr/bin/env bash
set -euo pipefail

# Backup script for Xiaomi Pad 7 Pro (codename: muyu, SM8635)
# Collects device info via ADB and, if rooted, dumps critical stock partitions.
# For locked stock HyperOS devices, it identifies the exact installed firmware
# version so the matching official Fastboot ROM (.tgz) can be downloaded.

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
BACKUP_DIR="${REPO_ROOT}/backups/xiaomi-pad-7-pro"
INFO_DIR="${BACKUP_DIR}/device-info"
PART_DIR="${BACKUP_DIR}/stock-rom/partitions"

mkdir -p "${INFO_DIR}" "${PART_DIR}"

if ! command -v adb >/dev/null 2>&1; then
    echo "ERROR: 'adb' not found in PATH." >&2
    exit 1
fi

echo "[1/4] Checking connected ADB devices..."
mapfile -t DEVICES < <(adb devices | awk 'NR>1 && $2=="device" {print $1}')

if [[ ${#DEVICES[@]} -eq 0 ]]; then
    echo "ERROR: No authorized ADB device connected." >&2
    echo "Plug in Xiaomi Pad 7 Pro (muyu) with USB Debugging enabled, then re-run." >&2
    exit 1
fi

TARGET_SERIAL="${1:-}"
if [[ -z "${TARGET_SERIAL}" ]]; then
    for serial in "${DEVICES[@]}"; do
        dev_name="$(adb -s "${serial}" shell getprop ro.product.device 2>/dev/null | tr -d '\r')"
        if [[ "${dev_name}" == "muyu" ]]; then
            TARGET_SERIAL="${serial}"
            break
        fi
    done
fi

if [[ -z "${TARGET_SERIAL}" ]]; then
    echo "ERROR: Could not find a 'muyu' device among connected ADB devices: ${DEVICES[*]}" >&2
    echo "Pass serial explicitly: $0 <serial>" >&2
    exit 1
fi

echo "Found Xiaomi Pad 7 Pro (muyu) at serial: ${TARGET_SERIAL}"

echo "[2/4] Capturing device properties and partition layout..."
adb -s "${TARGET_SERIAL}" shell getprop > "${INFO_DIR}/getprop.txt"
adb -s "${TARGET_SERIAL}" shell "cat /proc/partitions" > "${INFO_DIR}/proc_partitions.txt" || true
adb -s "${TARGET_SERIAL}" shell "ls -la /dev/block/by-name" > "${INFO_DIR}/by-name.txt" 2>&1 || true

BUILD_INC="$(adb -s "${TARGET_SERIAL}" shell getprop ro.build.version.incremental | tr -d '\r')"
MODEL="$(adb -s "${TARGET_SERIAL}" shell getprop ro.product.model | tr -d '\r')"
MOD_DEVICE="$(adb -s "${TARGET_SERIAL}" shell getprop ro.product.mod_device | tr -d '\r')"
ANDROID_VER="$(adb -s "${TARGET_SERIAL}" shell getprop ro.build.version.release | tr -d '\r')"
BL_LOCKED="$(adb -s "${TARGET_SERIAL}" shell getprop ro.boot.flash.locked | tr -d '\r')"
ACTIVE_SLOT="$(adb -s "${TARGET_SERIAL}" shell getprop ro.boot.slot_suffix | tr -d '\r')"

cat > "${INFO_DIR}/summary.txt" <<EOF
Serial: ${TARGET_SERIAL}
Model: ${MODEL}
Device: muyu (${MOD_DEVICE})
Android: ${ANDROID_VER}
HyperOS / Build Incremental: ${BUILD_INC}
Bootloader Locked (ro.boot.flash.locked): ${BL_LOCKED}
Active Slot: ${ACTIVE_SLOT}
Captured At: $(date -Is)
EOF

cat "${INFO_DIR}/summary.txt"

echo "[3/4] Checking root access for direct partition dump..."
if adb -s "${TARGET_SERIAL}" shell "su -c id" >/dev/null 2>&1; then
    echo "Root detected! Dumping critical stock partitions to ${PART_DIR}..."
    CRITICAL_PARTS=(
        boot_a boot_b
        init_boot_a init_boot_b
        vendor_boot_a vendor_boot_b
        dtbo_a dtbo_b
        vbmeta_a vbmeta_b
        vbmeta_system_a vbmeta_system_b
        persist modemst1 modemst2 fsg fsc
    )
    for part in "${CRITICAL_PARTS[@]}"; do
        if adb -s "${TARGET_SERIAL}" shell "su -c 'test -e /dev/block/by-name/${part}'" 2>/dev/null; then
            echo "  -> Dumping ${part}..."
            adb -s "${TARGET_SERIAL}" exec-out "su -c 'dd if=/dev/block/by-name/${part} bs=4M 2>/dev/null'" > "${PART_DIR}/${part}.img"
        fi
    done
    (cd "${PART_DIR}" && sha256sum *.img > SHA256SUMS 2>/dev/null || true)
    echo "[4/4] Direct partition backup complete."
else
    echo "No root access (normal for locked stock HyperOS)."
    echo "[4/4] Next step: download the official Xiaomi Fastboot ROM (.tgz) matching:"
    echo "       Device: muyu | Build: ${BUILD_INC}"
    echo "       Save it into: ${BACKUP_DIR}/stock-rom/"
fi
