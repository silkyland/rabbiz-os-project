#!/usr/bin/env bash
# Build + install the compositor-side CMA leak fix (tools/qt-scanout/dumb-destroy.c) on the device.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
host="${1:-root@172.16.42.1}"
remote_stage=$(ssh "$host" 'umask 077 && mktemp -d /root/.deploy.XXXXXX')
trap 'ssh -q "$host" "rm -rf -- \"\$1\"" _ "$remote_stage" 2>/dev/null || true' EXIT
scp -q tools/qt-scanout/dumb-destroy.c "$host:$remote_stage/dumb-destroy.c"
ssh "$host" 'sh -eu -s' -- "$remote_stage" <<'REMOTE'
stage="$1"
trap 'rm -rf -- "$stage"' EXIT
install -d /usr/local/lib/a21s-qt-scanout
gcc -shared -fPIC -O2 -Wall -Wextra -Werror "$stage/dumb-destroy.c" -o /usr/local/lib/a21s-qt-scanout/dumb-destroy.so -ldl
echo "installed; takes effect when labwc restarts (reboot)."
REMOTE
