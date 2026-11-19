# Interaction and motion contract

Scope: RabbizOS DE, handset first. Owner direction, 2026-11-18: visible press
feedback on app icons; dock accepts at most six apps, uses all its width at
rest, and opens slightly as a dragged icon approaches. Notification and quick
panels have no opaque fullscreen background; their controls use frosted glass.

## Research and decisions

The official [Material 3 motion specification](https://m3.material.io/styles/motion)
uses springs that preserve position and initial velocity across gestures,
interruptions and retargeting. Spatial channels (position, dimensions, corners)
are separate from effects channels (opacity and colour); effects must not
overshoot. Small controls use fast springs; fullscreen transitions use slow
springs. Apply the existing `de-design::motion` tokens and the session-wide
motion scheme, rather than creating a separate animation system per feature.

[Material interaction states](https://m3.material.io/foundations/interaction/states/state-layers)
and [the accessible Material 2 states reference](https://m2.material.io/design/interaction/states.html)
make press feedback distinguishable from dragging. One pressed/dragged item
owns the interaction. The state layer uses a content-derived colour; feedback
must not depend on motion alone. A launcher-specific small scale reduction is
our design choice, not a universal Material requirement.

The official [Android drag/drop lifecycle](https://developer.android.com/develop/ui/views/touch-and-input/drag-drop/concepts)
separates entering, moving within, leaving and dropping on a target. The target
can change appearance on entry and restore itself on exit; a cancelled drag is
not a drop. This informs the dock's anticipatory expansion. The exact expansion
amount and proximity distance are RabbizOS tuning choices, to verify on device.

## Implementation acceptance

1. Touch-down gives immediate icon feedback; release/cancel reverses from its
   current position. Launch morph begins from the currently painted icon bounds.
   Long-press transitions into a lifted icon, not two stacked icons.
2. Drawer rows launch on tap and lift on hold; scrolling cancels press feedback.
   A lifted drawer icon continues onto Home for placement in the grid or dock.
3. Dock at rest distributes exactly its occupied icons, equally spaced, from
   one through six. There are no reserved empty positions. An empty dock has no
   fake apps. Paint, tap bounds and launch origins share the same geometry.
4. Near the dock, the shelf expands slightly and a real prospective insertion
   position appears. Existing icons make space. Leaving restores the resting
   layout. Dropping commits once; cancellation preserves persisted membership.
   Six is a hard maximum; a full-dock drop follows the existing swap policy.
5. Home/Recents drags preserve the app process. The app card follows the finger;
   Recents settles to its actual card bounds. The backdrop is Home, never a
   blurred copy of the app being dismissed. Back provides edge feedback and
   never sends `xdg_toplevel.close`. Inside an app, Back goes to the app first,
   as on Android: an open menu closes, a dialog gets Escape, anything else gets
   the Back key (`KEY_BACK` / `XF86Back`); leaving the app is the home swipe.
   In Recents entered from an app, Back returns to that app. (Owner decision
   2026-11-19, replacing "Back returns Home"; see
   [AUDIT-UIUX-2026-11-19.md](AUDIT-UIUX-2026-11-19.md) Q1.)
6. Reduced motion removes optional scale/expansion and retains colour feedback.
   Repeated input can interrupt and reverse a transition without a position jump.
7. Panels are transparent between controls. Frosted controls sample the actual
   underlying scene. A cached/frozen backdrop is labelled as such; it is not
   described as live blur. A performance limit requires device measurements.
8. Measure cold backdrop construction separately from steady drag frames.
   Report CPU frame times, submitted-frame gaps, sample count, renderer, kernel
   and test path. Queue gaps are not panel scanout timestamps and do not prove
   display FPS. A process CPU average is not a frame-rate measurement.

## Verification order

Complete and measure panels first, then navigation/morph, icon pressure, and
finally dock anticipation/reflow. Host tests cover pure state/geometry;
screenshots and performance logs cover the actual A21s renderer. Do not mark
hardware capabilities available just because a tile can change its local bool.
