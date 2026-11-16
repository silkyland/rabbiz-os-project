#!/usr/bin/env bash
# One command for every host-side check. No CI exists in this repo yet; run before committing.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
node scripts/check_shell_seams.mjs          # real tree
node shell/tests/seams.test.mjs             # checker still fires on bad fixtures
node shell/tests/launchflow.test.mjs        # LaunchFlow seam test (no Qt, no device)
node shell/tests/gesture.test.mjs             # Gesture classifier (no Qt, no device)
node shell/tests/notificationgroups.test.mjs  # Shade notification grouping (no Qt, no device)
node tests/calc.test.mjs                      # Calculator expression evaluator (no Qt, no device)
node tests/clock.test.mjs              # clock app logic (no Qt, no device)
node tests/notes.test.mjs              # notes app logic (no Qt, no device)
node tests/photos.test.mjs             # photos app logic (no Qt, no device)
node tests/music.test.mjs              # music app logic (no Qt, no device)
node tests/terminal.test.mjs           # terminal app logic (no Qt, no device)
python3 -B -m unittest discover -s tools/qt-scanout/tests  # Mesa version gate (no device)
