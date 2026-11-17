#!/usr/bin/env bash
# de_bench.sh — frame stats run on the A21s; prints the summary JSON.
#
#   ./scripts/de_bench.sh [user@host] [duration_s] [scenario]
# JSONL lands in /tmp/rabbiz-de-stats.jsonl on device; summary on stdout.
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HOST="${1:-root@172.16.42.1}"
DURATION="${2:-10}"
SCENARIO="${3:-m0-bench}"

"$REPO_ROOT/scripts/device_session.sh" "$HOST" <<REMOTE
/usr/local/bin/rabbiz-de run --duration $DURATION \
  --scenario $SCENARIO --stats \
  --stats-file /tmp/rabbiz-de-stats.jsonl \
  --stats-summary /tmp/rabbiz-de-summary.json 2>/dev/null
REMOTE
