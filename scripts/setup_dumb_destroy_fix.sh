#!/usr/bin/env bash
# Build + install the compositor-side CMA leak fix (tools/qt-scanout/dumb-destroy.c) on the device.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
host="${1:-root@172.16.42.1}"
scp -q tools/qt-scanout/dumb-destroy.c "$host:/var/tmp/dumb-destroy.c"
ssh "$host" 'set -eu
install -d /usr/local/lib/a21s-qt-scanout
gcc -shared -fPIC -O2 -Wall -Wextra -Werror /var/tmp/dumb-destroy.c -o /usr/local/lib/a21s-qt-scanout/dumb-destroy.so -ldl
echo "installed; takes effect when labwc restarts (reboot)."'
