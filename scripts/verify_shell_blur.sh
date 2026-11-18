#!/usr/bin/env bash
# Run against an unlocked desktop. Serializes hardware use, never restarts labwc.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
host="${1:-root@172.16.42.1}"
output="${2:-/tmp/rabbiz-shell-blur}"
mkdir -p "$output"
./scripts/device_session.sh "$host" <<'REMOTE'
boot=$(cat /proc/sys/kernel/random/boot_id)
shell_pid=$(pgrep -f '^quickshell -p /usr/share/rabbiz-shell/shell.qml$')
since=$(date '+%Y-%m-%d %H:%M:%S')
ipc() {
    runuser -u rabbiz -- env LANG=C.UTF-8 XDG_RUNTIME_DIR=/run/user/$(id -u rabbiz) WAYLAND_DISPLAY=wayland-0 \
        quickshell -p /usr/share/rabbiz-shell/shell.qml ipc call "$@"
}
cleanup() {
    ipc shade close >/dev/null || true
    ipc drawer close >/dev/null || true
}
trap cleanup EXIT
check_state() {
    python3 -c 'import json,sys; s=json.load(sys.stdin); print(s); assert s["gpu"]; assert s.get("captured", s.get("sceneChildren", 0) == 2); assert s.get("sourceWidth",720) == 720; assert s.get("sourceHeight",1600) == 1600'
}
shot() {
    runuser -u rabbiz -- env XDG_RUNTIME_DIR=/run/user/$(id -u rabbiz) WAYLAND_DISPLAY=wayland-0 grim "$1"
}
ipc drawer close
ipc shade close
sleep 0.4
shot /tmp/rabbiz-blur-home.png
ipc drawer open
sleep 0.5
ipc drawer state | check_state
shot /tmp/rabbiz-blur-drawer.png
ipc drawer close
sleep 0.4
ipc shade open settings
sleep 0.5
ipc shade state | check_state
shot /tmp/rabbiz-blur-shade.png
ipc shade close
sleep 0.4
echo 'warm buffer allocation:'
tail -1 /sys/kernel/debug/dma_buf/bufinfo
for cycle in 1 2 3 4 5 6 7 8 9 10; do
    ipc shade open notifications
    sleep 0.35
    ipc shade state | check_state >/dev/null
    ipc shade close
    sleep 0.35
    ipc drawer open
    sleep 0.35
    ipc drawer state | check_state >/dev/null
    ipc drawer close
    sleep 0.35
done
[ "$boot" = "$(cat /proc/sys/kernel/random/boot_id)" ]
kill -0 "$shell_pid"
echo 'after ten cycles:'
tail -1 /sys/kernel/debug/dma_buf/bufinfo
rg Cma /proc/meminfo
journalctl -k --since "$since" --no-pager > /tmp/rabbiz-blur-kernel.log
if rg -i 'buffer chunks|supplied dmabufs|panfrost.*(fault|timeout)|cma.*alloc failed' /tmp/rabbiz-blur-kernel.log; then
    echo 'New GPU/allocation kernel errors during test' >&2
    exit 1
fi
printf 'PASS: ten shade/drawer cycles, stable boot=%s shell_pid=%s\n' "$boot" "$shell_pid"
REMOTE
for surface in home drawer shade; do
    scp -q "$host:/tmp/rabbiz-blur-$surface.png" "$output/$surface.png"
done
echo "Screenshots: $output"
