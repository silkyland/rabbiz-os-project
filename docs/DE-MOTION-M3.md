# DE motion: what Material 3 teaches us (study + proposals)

Status: **research and proposals**. P1 through P5 have host-side policy. P6 and P7 are documented. Lane: `de`.
Written 2026-11-18.
Like every rule here this is a hypothesis: the numbers below are starting points to be tuned by
feel on the A21s, and any change to `de-design` must check the goldens and record the decision in
the same change (see [DE-PLAN.md](DE-PLAN.md) §7.17a and `tools/rabbiz-de/crates/de-design/src/motion.rs`).

## 0. What was actually read

Source: the public Material Design 3 motion pages, read in a rendered browser on 2026-11-18
(the site is a JavaScript app; plain HTTP fetch returns only a title). Pages that returned content:

- `m3.material.io/styles/motion/overview/how-it-works` (physics system, schemes, spring tokens)
- `.../overview/specs` (spring composites, web curve table)
- `.../easing-and-duration/tokens-specs` and `.../applying-easing-and-duration`
- `.../transitions/transition-patterns`

**Not retrieved** (so nothing below depends on them): the stiffness/damping numbers behind each spring
token, the gestures / predictive-back page (returned no text), and the colour, type, shape, elevation
and state-layer pages. Those would be a separate study; this one is deliberately about motion.

Facts are paraphrased from the pages. M3 is Google's design system. RabbizOS borrows ideas only:
our tokens use our own names (`motion.*`), not `md.sys.*`, and the product never claims to be "Material"
([BRAND.md](BRAND.md)).

## 1. What M3 says about motion

### 1.1 Two systems
- **Legacy: easing + duration.** Still used for transitions, "no longer maintained" per the page.
- **Current: a physics system (M3 Expressive, May 2025).** Motion is driven by **springs**, each defined by
  stiffness, damping and initial velocity. Springs handle gestures, interruption and retargeting without
  special cases, and one spring can serve transitions, buttons and gestures, so the product feels consistent.

### 1.2 Schemes
Two presets, choosable per product or per element:
- **Expressive**: overshoots the final value, adds bounce; for hero moments and key interactions.
- **Standard**: eases in with minimal bounce; for utilitarian products.
Custom schemes are allowed. The scheme is chosen once at product level, not baked into each token name.

### 1.3 Token axes
- **Spatial** springs: anything that moves, rotates, resizes or changes corner radius. May overshoot.
- **Effects** springs: opacity and colour. **Never overshoot.**
- Each has **three speeds** by element size: *fast* (small things: switches, buttons), *default*
  (partial-screen: bottom sheet, expanded rail), *slow* (full-screen). Exact values also differ by device
  class (watch, phone, tablet) so "fast" feels fast on every screen.

### 1.4 Web conversion table (spring to cubic-bezier + duration)
Useful as a fallback when an animation cannot be interrupted:

| Spring | Curve (x1,y1,x2,y2) | Duration |
|---|---|---|
| Expressive fast / default / slow spatial | .42,1.67,.21,.90 / .38,1.21,.22,1.00 / .39,1.29,.35,.98 | 350 / 500 / 650 ms |
| Standard fast / default / slow spatial | .27,1.06,.18,1.00 (all three) | 350 / 500 / 750 ms |
| Expressive and Standard effects, fast / default / slow | .31,.94,.34,1.00 / .34,.80,.34,1.00 / .34,.88,.34,1.00 | 150 / 200 / 300 ms |

### 1.5 Legacy easing and duration (still the guidance for transitions)
Easing tokens: **emphasized** (two-segment path on Android; CSS has no equivalent), **emphasized decelerate**
(.05,.7,.1,1), **emphasized accelerate** (.3,0,.8,.15), **standard** (.2,0,0,1), **standard decelerate**
(0,0,0,1), **standard accelerate** (.3,0,1,1).
Durations: short 50 / 100 / 150 / 200 ms, medium 250 / 300 / 350 / 400, long 450 / 500 / 550 / 600,
extra-long 700 / 800 / 900 / 1000 (extra-long only for ambient motion with no user input).

Suggested pairs: begin-and-end on screen: emphasized 500 ms (standard 300); **enter**: emphasized
decelerate 400 ms (standard 250); **exit**: emphasized accelerate 200 ms (standard 200).
Rules of thumb on the page:
1. **Bigger area, longer duration** (200 ms small, 500 ms large) for a constant sense of speed.
2. **Exits are shorter than enters** (they need less attention than what comes next).
3. Leaving **permanently** (dismiss, delete) ends at peak velocity (accelerate) so it feels unrecoverable;
   leaving **temporarily** (drawer) ends at rest just off-screen so it feels retrievable.
4. Entering things decelerate and come to a soft rest.

### 1.6 Six transition patterns
| Pattern | Use | Idea |
|---|---|---|
| Container transform | card to detail, FAB to sheet, search box to results, app open | a persistent container morphs; strongest relationship, most expressive |
| Forward / backward | consecutive hierarchy levels (inbox to thread) | horizontal slide; Android adds fade, iOS adds parallax |
| Lateral | peers (tabs, carousel, gallery) | grouped slide in unison, no fade/parallax; hints at swipe |
| Top level | switching top destinations | quick fade out then fade in, no shared element (contents unrelated) |
| Enter and exit | dialogs, menus, snackbars, sheets, drawers | expand/collapse along x or y away from the edge; **avoid scale and z-axis** (implies elevation) |
| Skeleton loaders | loading to loaded | subtle pulse top-left to bottom-right, content fades in on top |

Spatial-model rule from the same page: the direction something enters from tells users where it lives
(notification from the top, nav drawer from the left, bottom sheet and keyboard from the bottom).

## 2. Where DE already agrees (nothing to do)

`crates/de-design/src/motion.rs` was written before this study and already matches M3 in several places.
Compared by value, not by assumption:

| DE today | M3 equivalent | Verdict |
|---|---|---|
| `BEZIER_STANDARD` (.2,0,0,1) | standard | identical |
| `BEZIER_ENTER` (.05,.7,.1,1) | emphasized decelerate | identical |
| `BEZIER_EXIT` (.3,0,.8,.15) | emphasized accelerate | identical |
| Springs with velocity-preserving retarget (M2-T4) | "springs handle interruption and retargeting" | same principle |
| `SpringPreset::Smooth` zero bounce for app/sheet motion | Standard scheme, used for utilitarian products | same choice, stricter |
| Degrade ladder + reduced-motion + safe mode (p95 > 20 ms steps down) | not in M3; a device-class concern | DE is **ahead** here, keep |
| Projected-end-position release (`PROJECT_AHEAD_S`), rubber overscroll | gesture handling (page not retrieved) | keep, unverified against M3 |

## 3. Gaps and proposals

Each proposal says what to change and how to know it worked. None touches the frozen `de-api` seam.

**P1. Split motion into spatial and effects.** Today a preset like `Snappy` (12 % bounce) can be applied
to anything. Add a kind (`Spatial` | `Effects`) and make effects (opacity, colour, blur radius) clamp to
no overshoot; overshoot on opacity >1 or negative blur is a bug, not a style.
Effects now use critical damping at the selected preset duration. The solver also bounds each
step between its current value and target, dropping velocity when it crosses a bound.
*Done when:* a unit test shows an effects spring never leaves [0,1] for any preset, and a golden-frame
test of the launch morph keeps opacity monotonic.

**P2. Name the "scheme".** Introduce `MotionScheme { Standard, Expressive }`. `Standard` is exactly today's
presets (default). `Expressive` is opt-in and only raises bounce on *small, playful, spatial* elements
(toggles, charging pill, hero moments), never on app open/close. A user setting switches it; reduced-motion
forces `Standard` with no bounce. Reason not to default to Expressive: on the A21s the DE renders in
software, and overshoot enlarges the animated damage area and adds frames; the DE plan already forbids
bounce on app motion. This is a hypothesis to settle by owner feel review.
*Done when:* the scheme is one value read by the animation clock, and tests prove `Standard` reproduces
current numbers bit-for-bit.

The clock defaults to Standard. Expressive raises spatial `Snappy` bounce from 12 % to 16 %.
`Soft` stays at the 25 % ceiling and `Smooth` stays at zero for apps and sheets. Reduced motion
or safe mode selects Standard with zero spring bounce. The existing fallback durations and curves
still apply. The retained-layer and morph drivers accept the clock. A user-facing setting and live
compositor integration are deferred until the compositor drives these animations.

**P3. Speed tiers by element size.** Define `Fast | Default | Slow` and choose from the covered-area
fraction, instead of per-screen presets. Proposed starting mapping onto DE's existing ladder
(`DURATIONS_MS = 90, 160, 240, 360, 520`): small controls fast (about 160 ms), partial-screen surfaces
(drawer, shade, power menu, OSD) default (about 360 ms), full-screen morph slow (about 520 ms, under
`DURATION_MAX_MS`).
*Done when:* a table in `de-design` maps each surface to a tier and a test checks every shipped transition
is covered.

The reference table lives in `de-design::motion::Transition::timing`. An exhaustive `Screen`
mapping requires each navigable surface to have a row. When the moving area's geometry is known,
Fast covers less than 10 % of the output, Default covers 10 % to under 75 %, and Slow starts at
75 %. The OSD uses Fast because the moving pill covers about 3.5 % of the reference output.

**P4. Exits shorter than enters, and two exit curves.** Give each transition an exit duration of roughly
45 % of its enter (M3: 200 vs 400-500 ms). Dismissed notification, closed app, cleared recent: accelerate
curve (permanent). Shade or drawer that can be pulled back: ends at rest at the edge (temporary).
*Done when:* close-app morph is measurably shorter than launch, and a swipe-dismiss uses `BEZIER_EXIT`.

The resulting enter/exit pairs are 160/72 ms, 360/162 ms and 520/234 ms. The host morph driver uses
the selected pair for committed transitions and hands a mid-flight reversal to the spring.
Temporary exits currently reuse `BEZIER_ENTER`, which reaches the edge at zero velocity. Permanent
app closes, recents removals and notification dismissals select `BEZIER_EXIT`. The launcher now
drives the app morph in `de-core`. Swipe-dismiss playback for recents and notifications is pending.

**P5. Complete the easing set.** Add `standard decelerate` (0,0,0,1) and `standard accelerate` (.3,0,1,1),
plus a two-segment approximation of **emphasized** for the "begins and ends on screen" case (container
transform, temporary exits). Keep `bezier_eval` and add endpoint and monotonicity tests.

`motion.rs` now exposes both standard edge curves and `emphasized_eval`. The emphasized
curve normalizes two cubic segments that join at (0.166666, 0.4). Host tests check all three
easings against an independent double-precision evaluation of the published coordinates,
with an absolute progress tolerance of 0.00001. Existing transitions keep their current
curve assignments. [Reference easing paths](https://github.com/material-components/material-components-android/blob/1.13.0/docs/theming/Motion.md#curves-easing--duration)

**P6. A written transition-pattern catalogue.** The [DE plan catalogue](DE-PLAN.md#transition-pattern-catalogue)
maps the v1 screen changes to the six patterns and records the current playback scope.

**P7. Audit for scale-in dialogs.** For overlays that enter or exit, use edge translation or an
anchored reveal along one axis. `PRESS_SCALE` remains available for touch-down feedback.

| Surface | Source audit |
|---|---|
| Power menu | The renderer paints a fixed centered sheet and scrim from a single layout. The bottom-sheet position and transition specified for P9 need a live input and presentation path. [Power renderer](../tools/rabbiz-de/crates/de-ui/src/power.rs) [Shell dispatch](../tools/rabbiz-de/crates/de-core/src/shell.rs) |
| Volume/brightness OSD | The pill has a fixed rectangle against the left edge. The simulator renders a single frame, and the live shell dispatcher has no OSD render case. [OSD renderer](../tools/rabbiz-de/crates/de-ui/src/osd.rs) [Simulator scenes](../tools/rabbiz-de/crates/de-sim/src/main.rs) [Shell dispatch](../tools/rabbiz-de/crates/de-core/src/shell.rs) |
| Screenshot toast | The simulator paints a fixed 160×96 thumbnail. The screenshot trigger and thumbnail lifetime are pending in M4-T8. [Screenshot renderer](../tools/rabbiz-de/crates/de-ui/src/screenshot.rs) [M4](DE-PLAN.md#m4-system-surfaces-and-services-effort-l) |
| Notification, charging and launch-message toasts | The dedicated toast renderers are pending. Launch failures currently select the static error sheet. [P8, P12 and P20](DE-PLAN.md#718-screen-contracts-v1) [Launch timeout](../tools/rabbiz-de/crates/de-core/src/frame.rs) [Error renderer](../tools/rabbiz-de/crates/de-ui/src/error.rs) |
| Menus | The drawer's long-press action sheet is pending. Wayland app popups are tracked and positioned by the compositor. Their contents and animation belong to the client. [Drawer gesture](../tools/rabbiz-de/crates/de-core/src/input.rs) [Popup handler](../tools/rabbiz-de/crates/de-core/src/xdg.rs) |

The audited shell renderers contain no scale-in or scale-out transitions to replace. The remaining
work needs overlay lifecycle and compositor wiring. Not tested on device. [Overlay motion finding](../findings/2026-11-18-de-overlay-motion-wiring.md)

**P8. Scale by form factor.** M3 values differ per device class. When the device profile says `tablet`,
multiply durations by a factor read from the profile rather than hard-coding the phone numbers; handset
stays at 1.0. (`devices/<vendor>/<codename>/device.toml` already carries `form_factor`.)

**P9. Make motion testable.** Add (a) numeric tests that each spring settles within its stated duration and
within its bounce limit, (b) golden frame strips for the morph at t = 0, 25, 50, 75, 100 %, and (c) an
on-device frame-time log for the launch morph, recorded as a finding with `lane: de` and its evidence
level. Without (c) nothing here may be called "verified on device".

## 4. Explicitly not adopted

- M3 Expressive bounce as the default for app launch/close (see P2).
- The token names `md.sys.motion.*` and any Google branding or artwork in UI or docs shown to users.
- Colour, type, shape, elevation and state layers from M3: not studied here and not decided by this document.
- Extra-long (700-1000 ms) durations except for ambient, non-interactive motion; DE caps at 600 ms.

## 5. Open questions for the owner

1. Expressive scheme as an opt-in setting only, or also the default for small controls?
2. How short may a close be? (M3 suggests under half the enter time; try 200 ms on the A21s and judge by feel.)
3. Do tablets get slower motion, or the same timings with larger distances?
4. Should the stiffness/damping values behind the M3 spring tokens be fetched from the Compose source before
   we compare spring *shapes*, rather than only durations and curves?
