#!/usr/bin/env bash
# mirror_setup.sh — install and enable wayvnc on the A21s for Rabbiz Mirror (macOS).
#
#   ./scripts/mirror_setup.sh                 # listen on 127.0.0.1:5900 only
#   DEVICE=<device-ip> ./scripts/mirror_setup.sh
#
# Connect over an SSH tunnel (wayvnc is bound strictly to loopback):
#   ssh -N -L 5900:127.0.0.1:5900 root@<device-ip>
#
# Idempotent. Packages and their PGP signatures are downloaded over verified TLS
# on the host (the device may have no Wi-Fi), verified with pacman-key on the
# device, and installed with `pacman -U`; nothing here touches /lib or the kernel.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

DEVICE=${DEVICE:-172.16.42.1}
LISTEN="127.0.0.1"
CACHE="tools/rabbiz-mirror/.cache/pkgs"
UNIT_PATH="/home/rabbiz/.config/systemd/user/a21s-wayvnc.service"
HELPER_PATH="/usr/local/bin/rabbiz-mirror"

while [ $# -gt 0 ]; do
    case "$1" in
        --host) DEVICE=$2; shift ;;
        -h|--help) sed -n '2,12p' "$0"; exit 0 ;;
        *) echo "unknown option: $1" >&2; exit 2 ;;
    esac
    shift
done

say() { printf '\033[1;34m==>\033[0m %s\n' "$*"; }

say "Checking SSH to root@$DEVICE"
ssh -o BatchMode=yes -o ConnectTimeout=6 "root@$DEVICE" true

# --- 1. wayvnc (and its two AUR-free deps) --------------------------------
if ssh "root@$DEVICE" 'pacman -Q wayvnc neatvnc aml' >/dev/null 2>&1; then
    say "wayvnc already installed on the device"
else
    say "Downloading wayvnc packages and signatures for aarch64 into $CACHE"
    mkdir -p "$CACHE"
    PKGS=(
        aml-1.0.0-1-aarch64.pkg.tar.xz
        neatvnc-1.0.3-1-aarch64.pkg.tar.xz
        wayvnc-0.10.2-1-aarch64.pkg.tar.xz
    )
    for package in "${PKGS[@]}"; do
        if [ ! -f "$CACHE/$package" ]; then
            curl -fsSL --max-time 120 \
                -o "$CACHE/$package" \
                "https://mirror.archlinuxarm.org/aarch64/extra/$package"
        fi
        if [ ! -f "$CACHE/$package.sig" ]; then
            curl -fsSL --max-time 120 \
                -o "$CACHE/$package.sig" \
                "https://mirror.archlinuxarm.org/aarch64/extra/$package.sig"
        fi
    done

    say "Staging and verifying packages on the device"
    REMOTE_STAGE=$(ssh "root@$DEVICE" 'umask 077 && mktemp -d /root/.mirror-setup.XXXXXX')
    trap 'ssh -q "root@$DEVICE" "rm -rf -- \"\$1\"" _ "$REMOTE_STAGE" 2>/dev/null || true' EXIT

    for package in "${PKGS[@]}"; do
        scp -q "$CACHE/$package" "$CACHE/$package.sig" "root@$DEVICE:$REMOTE_STAGE/"
    done

    ssh "root@$DEVICE" 'bash -se' -- "$REMOTE_STAGE" "${PKGS[@]}" <<'INSTALL'
set -euo pipefail
stage="$1"
shift
trap 'rm -rf -- "$stage"' EXIT
pkg_paths=()
for pkg in "$@"; do
    pacman-key --verify "$stage/$pkg.sig" "$stage/$pkg"
    pkg_paths+=("$stage/$pkg")
done
pacman -U --noconfirm "${pkg_paths[@]}"
INSTALL
    trap - EXIT
fi

# A manually started wayvnc (e.g. from an earlier probe) would hold port 5900.
ssh "root@$DEVICE" 'pkill -x wayvnc 2>/dev/null || true'

# --- 2. user unit in the rabbiz session -----------------------------------
say "Writing $UNIT_PATH (bound to $LISTEN:5900)"
ssh "root@$DEVICE" 'bash -se' -- "$UNIT_PATH" "$LISTEN" <<'WRITE_UNIT'
set -euo pipefail
unit_path="$1"
listen="$2"
rabbiz_uid=$(id -u rabbiz)
runuser -u rabbiz -- mkdir -p "$(dirname "$unit_path")"
runuser -u rabbiz -- tee "$unit_path" >/dev/null <<EOF
[Unit]
Description=A21s wayvnc screen mirror
Documentation=man:wayvnc(1)
StartLimitIntervalSec=0
StartLimitBurst=0

[Service]
Environment=XDG_RUNTIME_DIR=/run/user/${rabbiz_uid}
Environment=WAYLAND_DISPLAY=wayland-0
ExecStart=/usr/bin/wayvnc -o DSI-1 -f 30 ${listen}
Restart=always
RestartSec=2

[Install]
WantedBy=default.target
EOF
runuser -u rabbiz -- chmod 0644 "$unit_path"
WRITE_UNIT

# --- 3. root-side control helper used by the Mac app ----------------------
say "Writing $HELPER_PATH"
ssh "root@$DEVICE" "cat > $HELPER_PATH && chmod 755 $HELPER_PATH" <<'HELPER'
#!/bin/sh
# rabbiz-mirror — start/stop/status of the wayvnc mirror in the rabbiz labwc session.
set -eu
UNIT=a21s-wayvnc.service
RUNTIME=/run/user/$(id -u rabbiz)

as_rabbiz() {
    runuser -u rabbiz -- env XDG_RUNTIME_DIR="$RUNTIME" \
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
    as_rabbiz status "$UNIT" --no-pager >&2 || true
    return 1
}

case "${1:-status}" in
start)
    as_rabbiz start "$UNIT"
    wait_listening
    ;;
stop)
    as_rabbiz stop "$UNIT"
    echo "wayvnc stopped"
    ;;
restart)
    as_rabbiz restart "$UNIT"
    wait_listening
    ;;
status)
    echo "unit: $(as_rabbiz is-active "$UNIT" 2>/dev/null || true)"
    addresses=$(listen_addresses)
    if [ -n "$addresses" ]; then
        echo "listening: $(echo "$addresses" | tr '\n' ' ')"
    else
        echo "listening: no"
    fi
    ;;
logs)
    runuser -u rabbiz -- env XDG_RUNTIME_DIR="$RUNTIME" \
        journalctl --user-unit "$UNIT" -n "${2:-60}" --no-pager
    ;;
enable)
    as_rabbiz enable "$UNIT"
    as_rabbiz restart "$UNIT"
    wait_listening
    ;;
*)
    echo "usage: rabbiz-mirror {start|stop|restart|status|logs|enable}" >&2
    exit 2
    ;;
esac
HELPER

# --- 4. enable + start in the running session ----------------------------
say "Enabling and starting a21s-wayvnc in the rabbiz session"
ssh "root@$DEVICE" 'u=$(id -u rabbiz); runuser -u rabbiz -- env XDG_RUNTIME_DIR=/run/user/$u DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/$u/bus systemctl --user daemon-reload && rabbiz-mirror enable'

say "Done. Forward port 5900 over SSH and launch the Mac app with:"
printf '    ssh -N -L 5900:127.0.0.1:5900 root@%s &\n' "$DEVICE"
printf '    open "dist/Rabbiz Mirror.app"     # after ./scripts/build_mac_app.sh\n'
printf '    ssh root@%s rabbiz-mirror status\n' "$DEVICE"
