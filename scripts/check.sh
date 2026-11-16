#!/usr/bin/env bash
# One command for every host-side check. No CI exists in this repo yet; run before committing.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
node scripts/check_shell_seams.mjs          # real tree
node shell/tests/seams.test.mjs             # checker still fires on bad fixtures
node shell/tests/launchflow.test.mjs        # LaunchFlow seam test (no Qt, no device)
