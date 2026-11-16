#!/usr/bin/env bash
# Install a private opt-in experiment; never replace system Mesa or restart labwc.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
host="${1:-root@172.16.42.1}"
COPYFILE_DISABLE=1 tar --no-xattrs --exclude __pycache__ -cf - tools/qt-scanout rootfs-overlay/usr/local/bin/a21s-qt-gpu |
    ssh "$host" 'set -eu
stage=$(mktemp -d /tmp/a21s-qt-scanout.XXXXXX)
trap '\''rm -rf "$stage"'\'' EXIT
tar --no-same-owner -xf - -C "$stage"
cd "$stage"
destination=/usr/local/lib/a21s-qt-scanout
python3 tools/qt-scanout/patch_egl.py /usr/lib/libEGL_mesa.so.0 "$destination"
gcc -shared -fPIC -O2 -Wall -Wextra -Werror tools/qt-scanout/kms-node.c -o "$destination/kms-node.so" -ldl
install -m 644 tools/qt-scanout/Probe.qml "$destination/Probe.qml"
install -m 755 rootfs-overlay/usr/local/bin/a21s-qt-gpu /usr/local/bin/a21s-qt-gpu
echo "Installed opt-in a21s-qt-gpu. Session defaults are unchanged."
'
