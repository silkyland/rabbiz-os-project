#!/usr/bin/env bash
# One command for every host-side check. No CI exists in this repo yet; run before committing.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
node scripts/check_shell_seams.mjs          # real tree
node scripts/check_findings.mjs             # finding front matter (docs/FINDINGS.md)
node scripts/check_links.mjs                # relative markdown links resolve
node tests/findings.test.mjs                # the finding linter itself
node shell/tests/appcatalog.test.mjs        # Preferred base apps and duplicate entries
node shell/tests/iconlayout.test.mjs        # ~/Desktop + dock icon order/membership (no Qt, no device)
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
python3 -B tests/test_holdmenu.py                           # Vol Down + Power hold-to-menu (no evdev, no device)
python3 -B tests/test_volume_keys.py                       # Actual key loop with deterministic evdev events
node shell/tests/chargingevents.test.mjs                    # Stable charging connection notifications

# rabbiz-de host gate (M0): only when the workspace exists (integration worktree)
if [[ -f tools/rabbiz-de/Cargo.toml ]]; then
  ./scripts/de_test_host.sh                           # cargo test --workspace + golden check
fi

# rabbiz-osk (M7.12): pure-Rust keyboard core + host renderer (skipped when cargo is absent)
# osk-wayland is Linux-only (libxkbcommon), so the host check skips it; it is built in the arm64
# container and tested on the device (docs/BUILD.md §6).
if command -v cargo >/dev/null 2>&1; then
  cargo test --quiet --manifest-path tools/rabbiz-osk/Cargo.toml --workspace --exclude osk-wayland
fi
