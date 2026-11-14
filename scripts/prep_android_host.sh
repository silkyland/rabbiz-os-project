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

SDK="$("${ADB_CMD[@]}" shell getprop ro.build.version.sdk | tr -d '\r\n')"
echo "Detected Android SDK: ${SDK}"

if [[ -z "$SDK" ]] || ! [[ "$SDK" =~ ^[0-9]+$ ]]; then
  echo "Error: could not read numeric ro.build.version.sdk" >&2
  exit 1
fi

if (( SDK < 31 )); then
  echo "Android < 12: Phantom Process Killer not applicable"
  exit 0
fi

"${ADB_CMD[@]}" shell settings put global settings_enable_monitor_phantom_procs false
"${ADB_CMD[@]}" shell device_config set_sync_disabled_for_tests persistent
"${ADB_CMD[@]}" shell device_config put activity_manager max_phantom_processes 2147483647

MONITOR_VAL="$("${ADB_CMD[@]}" shell settings get global settings_enable_monitor_phantom_procs | tr -d '\r\n')"
MAX_VAL="$("${ADB_CMD[@]}" shell device_config get activity_manager max_phantom_processes | tr -d '\r\n')"

echo "Read-back settings_enable_monitor_phantom_procs: ${MONITOR_VAL}"
echo "Read-back max_phantom_processes: ${MAX_VAL}"

if [[ "$MONITOR_VAL" != "false" ]] || [[ "$MAX_VAL" != "2147483647" ]]; then
  echo "Error: read-back mismatch (expected false and 2147483647)" >&2
  exit 1
fi

echo "Host prep complete."
