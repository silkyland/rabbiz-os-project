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
SO_SHA256="${PANVK_SO_SHA256:-}"
ICD_SHA256="${PANVK_ICD_SHA256:-}"

if [[ ! "$SO_SHA256" =~ ^[0-9a-fA-F]{64}$ ]] || [[ ! "$ICD_SHA256" =~ ^[0-9a-fA-F]{64}$ ]]; then
  echo "ERROR: PANVK_SO_SHA256 and PANVK_ICD_SHA256 must be set to expected 64-char SHA-256 hex digests." >&2
  echo "       Example:" >&2
  echo "         PANVK_SO_SHA256=<sha256> PANVK_ICD_SHA256=<sha256> $0${SERIAL:+ -s $SERIAL}" >&2
  exit 2
fi

verify_sha256() {
  local expected="$1"
  local file="$2"
  if command -v sha256sum >/dev/null 2>&1; then
    printf '%s  %s\n' "$expected" "$file" | sha256sum -c -
  elif command -v shasum >/dev/null 2>&1; then
    printf '%s  %s\n' "$expected" "$file" | shasum -a 256 -c -
  else
    echo "ERROR: neither sha256sum nor shasum found to verify $file" >&2
    return 1
  fi
}

if [[ ! -f "$STAGING/libvulkan_panfrost-android-aarch64.so" ]]; then
  echo "Downloading libvulkan_panfrost-android-aarch64.so..."
  curl -fsSL --max-time 120 -o "$STAGING/libvulkan_panfrost-android-aarch64.so" "${BASE_URL}/libvulkan_panfrost-android-aarch64.so"
fi
verify_sha256 "$SO_SHA256" "$STAGING/libvulkan_panfrost-android-aarch64.so"

if [[ ! -f "$STAGING/panfrost_icd.aarch64.json" ]]; then
  echo "Downloading panfrost_icd.aarch64.json..."
  curl -fsSL --max-time 120 -o "$STAGING/panfrost_icd.aarch64.json" "${BASE_URL}/panfrost_icd.aarch64.json"
fi
verify_sha256 "$ICD_SHA256" "$STAGING/panfrost_icd.aarch64.json"

"${ADB_CMD[@]}" shell mkdir -p /sdcard/Download/panvk-test
"${ADB_CMD[@]}" push "$STAGING/libvulkan_panfrost-android-aarch64.so" /sdcard/Download/panvk-test/
"${ADB_CMD[@]}" push "$STAGING/panfrost_icd.aarch64.json" /sdcard/Download/panvk-test/
"${ADB_CMD[@]}" push scripts/termux_panvk_run.sh /sdcard/Download/panvk-test/

echo "Pushed PanVK test artifacts to /sdcard/Download/panvk-test/"
