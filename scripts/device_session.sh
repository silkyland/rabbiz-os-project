#!/usr/bin/env bash
# Feed a device mutation/test script on stdin. Host development remains parallel.
# Waits at most DEVICE_LOCK_WAIT seconds (default 45) and then FAILS with the
# holder's info, so an agent never sits silently on a busy slot.
set -euo pipefail
host="${1:-root@172.16.42.1}"
wait_s="${DEVICE_LOCK_WAIT:-45}"
echo "Waiting up to ${wait_s}s for the shared A21s test slot on $host..." >&2
exec ssh "$host" "
lock=/run/lock/a21s-device-test.lock
exec 9>>\$lock
if ! flock --timeout $wait_s 9; then
  echo 'A21s test slot busy after ${wait_s}s. Holder:' >&2
  cat \$lock >&2 || true
  ps -eo pid,etime,args | grep -E 'rabbiz-de|a21s-|bash -se' | grep -v grep >&2 || true
  exit 75
fi
: > \$lock
echo \"pid=\$\$ since=\$(date +%T) host=\${SSH_CLIENT%% *}\" >&9
bash -se
"
