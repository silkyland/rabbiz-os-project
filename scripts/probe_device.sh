#!/usr/bin/env bash
set -euo pipefail

SERIAL=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    -s)
      SERIAL="${2:-}"
      shift 2
      ;;
    -h|--help)
      echo "Usage: $0 [-s <serial>] <output_path>"
      exit 0
      ;;
    *)
      break
      ;;
  esac
done

OUT_PATH="${1:-}"
if [[ -z "$OUT_PATH" ]]; then
  echo "Error: missing <output_path>" >&2
  echo "Usage: $0 [-s <serial>] <output_path>" >&2
  exit 1
fi

ADB_CMD=(adb)
if [[ -n "$SERIAL" ]]; then
  ADB_CMD+=(-s "$SERIAL")
fi

mkdir -p "$(dirname "$OUT_PATH")"

probe() {
  local label="$1"
  local cmd="$2"
  echo "=== ${label} ===" >> "$OUT_PATH"
  "${ADB_CMD[@]}" shell "$cmd" >> "$OUT_PATH" 2>&1 || echo "[probe failed or restricted]" >> "$OUT_PATH"
  echo "" >> "$OUT_PATH"
}

: > "$OUT_PATH"
echo "# Device Profile ($(date -Iseconds))" >> "$OUT_PATH"
echo "" >> "$OUT_PATH"

probe "ro.product.model" "getprop ro.product.model"
probe "ro.hardware" "getprop ro.hardware"
probe "ro.soc.model / ro.board.platform" "getprop ro.soc.model; getprop ro.board.platform"
probe "ro.build.version.release" "getprop ro.build.version.release"
probe "ro.build.version.sdk" "getprop ro.build.version.sdk"
probe "ro.product.cpu.abilist" "getprop ro.product.cpu.abilist"
probe "MemTotal / MemAvailable" "grep -E '^(MemTotal|MemAvailable):' /proc/meminfo"
probe "uname -a" "uname -a"
probe "getenforce" "getenforce"
probe "ls -lZ /dev/mali0" "ls -lZ /dev/mali0"
probe "mali_kbase version" "cat /sys/module/mali_kbase/version 2>/dev/null || echo 'no /sys/module/mali_kbase/version'"
probe "ro.boot.flash.locked" "getprop ro.boot.flash.locked"
probe "ro.boot.verifiedbootstate" "getprop ro.boot.verifiedbootstate"
probe "dumpsys deviceidle whitelist" "dumpsys deviceidle whitelist"

echo "Saved device profile to $OUT_PATH"
