#!/usr/bin/env bash
set -euo pipefail

ROOTED=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --rooted)
      ROOTED=1
      shift
      ;;
    -h|--help)
      echo "Usage: $0 [--rooted]"
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      exit 1
      ;;
  esac
done

SRC_DIR="/sdcard/Download/panvk-test"
DEST_DIR="$HOME/panvk-test"

mkdir -p "$DEST_DIR"
cp "$SRC_DIR/libvulkan_panfrost-android-aarch64.so" "$DEST_DIR/"
cp "$SRC_DIR/panfrost_icd.aarch64.json" "$DEST_DIR/"
chmod 755 "$DEST_DIR/libvulkan_panfrost-android-aarch64.so"

# Patch ICD.library_path to the executable $HOME/panvk-test/ path
python3 - <<PY
import json, os
icd_path = os.path.expanduser("$DEST_DIR/panfrost_icd.aarch64.json")
so_path = os.path.expanduser("$DEST_DIR/libvulkan_panfrost-android-aarch64.so")
with open(icd_path, "r") as f:
    data = json.load(f)
data["ICD"]["library_path"] = so_path
with open(icd_path, "w") as f:
    json.dump(data, f, indent=2)
print("Patched ICD library_path ->", so_path)
PY

export VK_ICD_FILENAMES="$DEST_DIR/panfrost_icd.aarch64.json"
export LD_LIBRARY_PATH="$DEST_DIR:${LD_LIBRARY_PATH:-}"

run_vk() {
  if (( ROOTED == 1 )); then
    tsu -c "VK_ICD_FILENAMES='$VK_ICD_FILENAMES' LD_LIBRARY_PATH='$LD_LIBRARY_PATH' $*"
  else
    "$@"
  fi
}

echo "=== vulkaninfo --summary ==="
run_vk vulkaninfo --summary || true

echo "=== vkcube --wsi headless ==="
if vkcube --help 2>&1 | grep -q headless; then
  run_vk vkcube --wsi headless --c 10 || true
else
  echo "vkcube --wsi headless not supported; check Termux:X11 or DISPLAY=:0"
fi
