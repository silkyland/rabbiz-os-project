#!/usr/bin/env bash
# mirror_setup.sh — install and enable wayvnc on the A21s for Rabbiz Mirror (macOS).
#
#   ./scripts/mirror_setup.sh                 # listen on USB + localhost only
#   ./scripts/mirror_setup.sh --lan           # also listen on all interfaces (Wi-Fi)
#   DEVICE=<device-ip> ./scripts/mirror_setup.sh
#
# Idempotent. Packages are downloaded on the Mac (the device may have no Wi-Fi)
# and installed with `pacman -U`; nothing here touches /lib or the kernel.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

DEVICE=${DEVICE:-172.16.42.1}
LISTEN="172.16.42.1 127.0.0.1"
CACHE="tools/a21s-mirror/.cache/pkgs"
UNIT_PATH="/home/gamer/.config/systemd/user/a21s-wayvnc.service"
HELPER_PATH="/usr/local/bin/a21s-mirror"

while [ $# -gt 0 ]; do
    case "$1" in
        --lan) LISTEN="0.0.0.0" ;;
        --host) DEVICE=$2; shift ;;
        -h|--help) sed -n '2,12p' "$0"; exit 0 ;;
        *) echo "unknown option: $1" >&2; exit 2 ;;
    esac
    shift
done

say() { printf '\033[1;34m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33mwarning:\033[0m %s\n' "$*"; }

say "Checking SSH to root@$DEVICE"
ssh -o BatchMode=yes -o ConnectTimeout=6 "root@$DEVICE" true

# --- 1. wayvnc (and its two AUR-free deps) --------------------------------
if ssh "root@$DEVICE" 'pacman -Q wayvnc neatvnc aml' >/dev/null 2>&1; then
    say "wayvnc already installed on the device"
else
    say "Downloading wayvnc packages for aarch64 into $CACHE"
    mkdir -p "$CACHE"
    for package in \
        aml-1.0.0-1-aarch64.pkg.tar.xz \
        neatvnc-1.0.3-1-aarch64.pkg.tar.xz \
        wayvnc-0.10.2-1-aarch64.pkg.tar.xz
    do
        if [ ! -f "$CACHE/$package" ]; then
            curl -skL --fail --max-time 120 \
                -o "$CACHE/$package" \
                "https://mirror.archlinuxarm.org/aarch64/extra/$package"
        fi
    done
    say "Installing packages on the device"
    scp -q "$CACHE"/aml-1.0.0-1-aarch64.pkg.tar.xz \
           "$CACHE"/neatvnc-1.0.3-1-aarch64.pkg.tar.xz \
           "$CACHE"/wayvnc-0.10.2-1-aarch64.pkg.tar.xz "root@$DEVICE:/tmp/"
    ssh "root@$DEVICE" 'pacman -U --noconfirm /tmp/aml-1.0.0-1-aarch64.pkg.tar.xz /tmp/neatvnc-1.0.3-1-aarch64.pkg.tar.xz /tmp/wayvnc-0.10.2-1-aarch64.pkg.tar.xz'
fi

# A manually started wayvnc (e.g. from an earlier probe) would hold port 5900.
ssh "root@$DEVICE" 'pkill -x wayvnc 2>/dev/null || true'

# --- 2. user unit in the gamer session -----------------------------------
say "Writing $UNIT_PATH"
GAMER_UID=$(ssh "root@$DEVICE" 'id -u gamer')
ssh "root@$DEVICE" "install -d -o gamer -g gamer /home/gamer/.config/systemd/user && cat > $UNIT_PATH" <<UNIT
[Unit]
Description=A21s wayvnc screen mirror
Documentation=man:wayvnc(1)
StartLimitIntervalSec=0
StartLimitBurst=0

[Service]
Environment=XDG_RUNTIME_DIR=/run/user/${GAMER_UID}
Environment=WAYLAND_DISPLAY=wayland-0
ExecStart=/usr/bin/wayvnc -o DSI-1 -f 30 $LISTEN
Restart=always
RestartSec=2

[Install]
WantedBy=default.target
UNIT

# --- 3. root-side control helper used by the Mac app ----------------------
say "Writing $HELPER_PATH"
ssh "root@$DEVICE" "cat > $HELPER_PATH && chmod 755 $HELPER_PATH" <<'HELPER'
#!/bin/sh
# a21s-mirror — start/stop/status of the wayvnc mirror in the gamer labwc session.
set -eu
UNIT=a21s-wayvnc.service
RUNTIME=/run/user/$(id -u gamer)

as_gamer() {
    runuser -u gamer -- env XDG_RUNTIME_DIR="$RUNTIME" \
        DBUS_SESSION_BUS_ADDRESS="unix:path=$RUNTIME/bus" systemctl --user "$@"
}

listen_addresses() {
    ss -ltn 2>/dev/null | awk '$4 ~ /:5900$/ {sub(/:[0-9]+$/, "", $4); print $4}' | sort -u
}

wait_listening() {
    i=0
    while [ "$i" -lt 40 ]; do
        if [ -n "$(listen_addresses)" ]; then
            echo "wayvnc listening on $(listen_addresses | tr '\n' ' ')"
            return 0
        fi
        sleep 0.25
        i=$((i + 1))
    done
    echo "wayvnc did not open port 5900" >&2
    as_gamer status "$UNIT" --no-pager >&2 || true
    return 1
}

case "${1:-status}" in
start)
    as_gamer start "$UNIT"
    wait_listening
    ;;
stop)
    as_gamer stop "$UNIT"
    echo "wayvnc stopped"
    ;;
restart)
    as_gamer restart "$UNIT"
    wait_listening
    ;;
status)
    echo "unit: $(as_gamer is-active "$UNIT" 2>/dev/null || true)"
    addresses=$(listen_addresses)
    if [ -n "$addresses" ]; then
        echo "listening: $(echo "$addresses" | tr '\n' ' ')"
    else
        echo "listening: no"
    fi
    ;;
logs)
    runuser -u gamer -- env XDG_RUNTIME_DIR="$RUNTIME" \
        journalctl --user-unit "$UNIT" -n "${2:-60}" --no-pager
    ;;
enable)
    as_gamer enable "$UNIT"
    as_gamer restart "$UNIT"
    wait_listening
    ;;
*)
    echo "usage: a21s-mirror {start|stop|restart|status|logs|enable}" >&2
    exit 2
    ;;
esac
HELPER

# --- 4. enable + start in the running session ----------------------------
say "Enabling and starting a21s-wayvnc in the gamer session"
ssh "root@$DEVICE" 'u=$(id -u gamer); runuser -u gamer -- env XDG_RUNTIME_DIR=/run/user/$u DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/$u/bus systemctl --user daemon-reload && a21s-mirror enable'

say "Done. Launch the Mac app with:"
printf '    open "dist/Rabbiz Mirror.app"     # after ./scripts/build_mac_app.sh\n'
printf '    ssh root@%s a21s-mirror status\n' "$DEVICE"
if [ "$LISTEN" = "0.0.0.0" ]; then
    warn "wayvnc has no authentication and is now reachable from the LAN. Use only on trusted networks."
fi
