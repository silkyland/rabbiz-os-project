# UI/UX audit — gestures, navigation and Material 3 (2026-11-19)

Lane: `de` · Device: samsung/a21s (720×1600) · Evidence: **L0** (code reading + host tests;
nothing here was tried on the phone yet) · AI-assisted.

Scope: `tools/rabbiz-de` (the live DE since the 2026-11-18 cut-over). The audit was made on top
of the **uncommitted** working-tree changes already present in `de-ui` / `de-core` (velocity
window, QuickSwitch, drawer fling). Reference: stock Android gesture navigation (Pixel /
AOSP Launcher3 + SystemUI) and Material 3. Owner decisions in `DE-PLAN.md`,
`DE-INTERACTION-MOTION.md` and `BRAND.md` win over M3; where one visibly costs usability it is
listed under *Questions*, not silently "fixed".

Scale assumption used throughout (a **guess**, needs a device check): 2 px ≈ 1 dp, i.e. the
360 dp-wide layout stock One UI uses on this panel. The code has no dp scale; tokens are raw px.

## 1. What a user feels today (gestures and navigation)

| # | Situation | What happens now | What Android does | Status |
|---|---|---|---|---|
| G1 | Swipe **right** along the bottom bar | Jumped to the *oldest* app in Recents; swipe left went to the previous app | Swipe right = back to the app you just used | **Fixed** (`gestures.rs` `QuickSwitch { to_previous }`, `input.rs`) |
| G2 | Swipe up past ~20 %, change your mind, flick down | Recents opened anyway | Flick down returns you to the app | **Fixed** (`decide_navigation`: downward fling ⇒ Cancel, test added) |
| G3 | Back swipe from the edge, released between 40 and 48 px | Bell still showed the dark "not armed" colour, but back fired | Arrow arms exactly where release commits | **Fixed** (`BACK_COMMIT_PX` shared by tracker, feedback and `navigation.rs`) |
| G4 | **Back inside an app** (browser page, dialog, settings sub-page) | Always leaves the app (minimise to Home) | Back goes to the app first; leaves only at the app's root | **Fixed** after owner answered Q1 (`navigation.rs` `app_back`): menu closes → dialog gets Escape → otherwise Back key. Leaving the app = home swipe |
| G5 | Swipe up on a card in Recents (to close an app) | Goes to Home; no card dismiss exists in `de-core` (`RecentsModel::dismiss` is unused) | Swipe card up = close that app | **Fixed**: the card under the finger follows it up and fades; release ≥ 18 % of the height or an upward fling ≥ 1000 px/s sends `xdg_toplevel.close` and the card flies off (`navigation.rs` `card_dismiss_*`, `recents.rs` `CardLift` / `dismiss_on_release` + test). Swiping up on the space around the cards still goes Home. An app that refuses to close (e.g. an unsaved-changes dialog) gets its card back |
| G6 | Swipe up, pause (Recents preview), then keep swiping up fast | Already committed to Recents at the pause; later motion ignored | Pause only *previews*; release decides, a further fling still goes Home | **Fixed**: the pause now emits `NavPaused` (the card springs to the Recents size), moving on by 2× slop emits `NavResumed` (the card springs back under the finger, no jump), and the release decides with `decide_navigation(.., paused)`: fling up = Home, fling down = Cancel, otherwise Recents. Tests added |
| G7 | Back gesture while in Recents | Goes Home | Returns to the app you came from | **Fixed** (`recents_from_app`, morphs back from the card; from Home it still goes Home) |
| G8 | Horizontal scrub on the bar | Nothing follows the finger, then a hard cut to the other app | The app card slides with the finger | Open — feed `dx` into the morph instead of `return` (`input.rs` NavBar Progress) |
| G9 | Fling threshold for Home is 450 px/s from 3 % rise | Light lifts may count as Home | AOSP uses ~0.5 dp/ms (~1000 px/s here) — **from memory, verify** | Tune on device |

Things that are already good and should be kept: projection-based pager / pull / drawer
springs that keep the finger's velocity (`pager.rs`, `pull.rs`), rubber-band at page ends,
"never skips more than one page", resting-before-lift is not a fling, long-press 400 ms
(= Android default), second finger cancels, swipe-down anywhere on Home opens the panel,
pull-down-to-close the drawer only when scrolled to the top.

## 2. Visual / Material 3 (verified file:line unless marked guess)

**P0 — usability-breaking**

1. **Home icon labels unreadable on the default wallpaper.** Labels use `t.text` (plum
   `#302135`, `de-ui/src/home.rs:414`) on the dark `ember` wallpaper (`de-config/src/prefs.rs:14`);
   estimated ~1.4:1 in the upper rows. The home clock already uses cream. Launcher3: light label
   + shadow on wallpaper regardless of theme. Fix: `on_wallpaper` token + 1–2 px 40 % shadow;
   add a wallpaper case to `de-design/tests/contrast.rs`.
2. **Panel text over bright apps.** GPU path always uses `tokens::DEFAULT.scrim` (40 %)
   (`de-core/src/frame.rs:932`); cream text on it over a white app ≈ 2.3:1 (estimate). Also
   ignores dark theme, and the capture path uses `de.theme.scrim` (`composite.rs:91`) so
   screenshots differ from the screen. Fix: themed `panel_scrim` ≥ 60 %.
3. **No touch-down feedback on icons.** `PRESS_SCALE` / `PRESS_SCALE_MS`
   (`de-design/src/motion.rs:96-98`) have no callers; no state layer on home, dock, drawer,
   search rows. Contradicts acceptance item 1 of `DE-INTERACTION-MOTION.md`. Fix: 10 % content
   colour layer + 0.96 scale in 90 ms on down; 16 % while dragged; reduced motion keeps colour only.
4. **Touch-target rule is in px, not dp.** `DE-PLAN.md` decision 15 says "≥ 48 px"; Android's
   rule is 48 dp (≈ 96 px, physical floor ≈ 81 px). Under it: drawer search field 56 px
   (`drawer.rs:28`), home Search pill 200×56 (`home.rs:121`), A–Z rail cells 36×30
   (`drawer.rs:20-21`), Recents "Clear all" 68 px tall (`recents.rs:255`). Paint and hit rects do
   match (shared geometry) — good.

**P1 — noticeably off vs Android / M3**

5. Battery icon always full (`chrome.rs:345`, `e1a4`); pick `battery_0…6_bar` by level, danger
   colour < 15 %.
6. Home "Search" pill opens the app drawer (`home.rs:119-121`) — mislabelled.
7. Status bar small: clock 22 px, icons 22 px, SIM digit 13 px (`chrome.rs:275-302`). SystemUI
   ≈ 14 sp clock (28 px). (Icon size from memory.)
8. `de-design/src/type_scale.rs` is unused; screens use drifted `chrome::style` sizes (body 13 sp,
   caption 10 sp, micro 9 sp — below M3's 11 sp floor). No font-scale setting.
9. Drawer is an A–Z list with 28 dp icons; plan P3 and Launcher3 use a 4–5 column grid.
10. Gesture handle changes shape between shell (200×8 at h−24, `chrome.rs:379`) and apps
    (240×4, `nav_bar.rs:18-20`) — it visibly jumps on every app open/close; fixed colour gives
    ~2:1 on light content. Android: one handle, colour adapts to what is under it.
11. Dark-mode parity broken where `tokens::DEFAULT` is hard-coded: `frame.rs:932`,
    `frame.rs:1173`, `nav_bar.rs:15`, `boot.rs:29`.

**P2 — polish:** radii off the M3 shape scale (chip 6 / row 10 / card 14 / sheet 18 dp vs
8/12/16/28); spacing off the 8 px grid (`CELL_H` 172, `LEFT` 44, …); no elevation anywhere
(`ELEVATION_SHADOW` unused — Recents cards on a cream veil barely separate); literal cream/plum
colours outside `tokens.rs` (`home.rs`, `chrome.rs`, `panels.rs`, `lock.rs`); menu highlight is a
solid fill instead of a 10 % state layer; lock hint 20 px at 85 %; reduced motion is env-only
(`RABBIZ_REDUCED_MOTION`); `tokens.rs:133` lists a `sand` wallpaper that no longer ships.

## 3. Changes made in this audit

- `crates/de-ui/src/gestures.rs`: `QuickSwitch { to_previous }` (rightward = previous app, as
  Android); `decide_navigation` cancels on a downward fling; `BACK_COMMIT_PX` const shared by the
  tracker config and the feedback bell; tests updated + one new assertion.
- `crates/de-core/src/input.rs`: QuickSwitch target uses `to_previous`.
- `crates/de-core/src/navigation.rs`: feedback cache key uses `BACK_COMMIT_PX`; new
  `app_back()` (G4) and `recents_from_app` (G7).
- G5/G6 (second round, same day): `gestures.rs` `NavPaused`/`NavResumed`, `NavReleased { paused }`,
  `Gesture::Recents` removed; `launch_morph.rs` `MorphAnimator::chase`; `morph.rs` `Peek` preview state;
  `recents.rs` `CardLift`; `navigation.rs` `CardDismiss`; `shell.rs` and `frame.rs` paint and animate the card.
- `crates/de-core/src/input.rs`: `EdgeBack` on an app calls `app_back`; in Recents returns to the
  app it came from; `show_shell(Recents)` records where Recents was entered from.

Evidence (after both rounds): `cargo test --target aarch64-unknown-linux-gnu -p de-core -p de-ui`
in `rabbiz-de-build:arm64` → de-core 22 passed, de-ui 88 passed; clippy adds no new warning in the
touched code beyond the pre-existing `render_at_pos` argument count.
Not run on the A21s.

## 4. Questions only the owner can answer

- **Q1 Back inside apps — answered 2026-11-19: yes, like Android.** Implemented; contract updated
  in `DE-INTERACTION-MOTION.md` item 5. Known cost: an app that maps neither Escape nor Back
  (e.g. a terminal) does nothing on back — use the home swipe. Original question: should edge-back send a Back key (`KEY_BACK` / XF86Back, which
  Chromium, Firefox and Qt understand) to the focused app, with "leave the app" left to the
  home swipe? That is what Android users expect, but it changes acceptance item 5 of
  `DE-INTERACTION-MOTION.md` (back = return Home) and apps without a back action would do
  nothing on back.
- **Q2 Units.** Adopt dp at 2.0 px for touch targets and text sizes (rule 15 → 96 px)?
- **Q3 Home labels.** Cream-on-wallpaper regardless of light/dark theme (as Launcher3)?
- **Q4 Drawer.** List (current) or the planned 4×6 grid?
- **Q5 Split pull vs two-stage shade.** Keep the split (left = notifications, right = quick
  settings)? Cost: no quick-settings row above notifications; users must learn which corner
  opens what.

## 5. Suggested order of work

1. Device-check G4 (which of our apps honour `XF86Back`), G5 and G6 by feel — these are the
   gestures a phone user hits every minute. G7 and G8 are smaller follow-ups.
2. P0 visuals 1–3 (labels, panel scrim, press feedback) — small, local changes.
3. G8, handle unification (P1-10), then the rest of P1.
4. Device pass: tune G9 and the back commit distance by feel, record as a finding with logs.
