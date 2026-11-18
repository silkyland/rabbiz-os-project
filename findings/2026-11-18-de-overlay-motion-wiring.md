---
status: hypothesis
lane: de
device: any
kernel: n/a
author: Codex
ai_assisted: true
date: 2026-11-18
evidence:
  - cmd: "rg -n 'fn render|Rect::new|progress|scale' tools/rabbiz-de/crates/de-ui/src/{power,osd,screenshot,error}.rs"
  - cmd: "rg -n 'Screen::(Power|Osd|Screenshot)|Gesture::LongPress|new_popup' tools/rabbiz-de/crates/de-core/src"
supersedes: []
---

# Overlay motion needs a compositor-driven lifetime

## Observed

At `3110ee9`, the Rust power menu, OSD, screenshot thumbnail and error sheet each render one
fixed layout. The live dispatcher can draw Power and Error as shell screens. `de-sim` draws
the OSD and screenshot thumbnail as static scenes. The drawer long-press handler is a no-op,
and notification toasts have no `de-ui` renderer. Not tested on device. [Scale-in audit](../docs/DE-MOTION-M3.md#3-gaps-and-proposals)

## Interpretation

An edge transition needs the compositor to retain the overlay through its exit and drive
its position or clip from animation progress. For Power, the intended bottom sheet also
needs layout and hit testing that agree. The OSD and screenshot thumbnail can keep their
dimensions while translating from the left and bottom edges, respectively. The same pattern
can guide the drawer action sheet and toasts when they land. These directions are hypotheses
pending feel review. [Pattern catalogue](../docs/DE-PLAN.md#transition-pattern-catalogue)

## Reproduce

Run the source searches above and trace the render calls in `de-core/src/shell.rs` and
`de-sim/src/main.rs`. Check the static `power`, `osd`, `screenshot` and `error` scenes with
`./scripts/de_test_host.sh`. To verify playback once wired, capture enter and exit frames
for each overlay on the A21s and check its bounds, edge direction and final position.
