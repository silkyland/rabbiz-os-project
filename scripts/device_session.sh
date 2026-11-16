#!/usr/bin/env bash
# Feed a device mutation/test script on stdin. Host development remains parallel.
set -euo pipefail
host="${1:-root@172.16.42.1}"
echo "Waiting for the shared A21s test slot on $host..." >&2
exec ssh "$host" 'flock --close --timeout 300 /run/lock/a21s-device-test.lock bash -se'
