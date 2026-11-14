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
      echo "Usage: $0 [-s <serial>]"
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      exit 1
      ;;
  esac
done

ADB_CMD=(adb)
if [[ -n "$SERIAL" ]]; then
  ADB_CMD+=(-s "$SERIAL")
fi

STAGING="build/panvk-staging/kbase"
mkdir -p "$STAGING"

BASE_URL="https://github.com/zenithblue-oss/panvk-kbase-android/releases/download/panvk-kbase-v26.3.1"

if [[ ! -f "$STAGING/libvulkan_panfrost-android-aarch64.so" ]]; then
  echo "Downloading libvulkan_panfrost-android-aarch64.so..."
  curl -sL -o "$STAGING/libvulkan_panfrost-android-aarch64.so" "${BASE_URL}/libvulkan_panfrost-android-aarch64.so"
fi

if [[ ! -f "$STAGING/panfrost_icd.aarch64.json" ]]; then
  echo "Downloading panfrost_icd.aarch64.json..."
  curl -sL -o "$STAGING/panfrost_icd.aarch64.json" "${BASE_URL}/panfrost_icd.aarch64.json"
fi

"${ADB_CMD[@]}" shell mkdir -p /sdcard/Download/panvk-test
"${ADB_CMD[@]}" push "$STAGING/libvulkan_panfrost-android-aarch64.so" /sdcard/Download/panvk-test/
"${ADB_CMD[@]}" push "$STAGING/panfrost_icd.aarch64.json" /sdcard/Download/panvk-test/
"${ADB_CMD[@]}" push scripts/termux_panvk_run.sh /sdcard/Download/panvk-test/

echo "Pushed PanVK test artifacts to /sdcard/Download/panvk-test/"
