# RabbizOS "Cream Glass" light theme: token summary (DRAFT)

Status: **proposal**, not yet applied to `tools/rabbiz-de/crates/de-design`. The owner
preferred this direction over the dark plum shell on 2026-11-17. It is a hypothesis
like every other design rule here: change it when evidence says so, and record the
change (ADR) in the same MR that touches `de-design` and the goldens.

Source: Stitch project `RabbizOS Phone Shell` (id `17138230838278197132`), design system
`RabbizOS Cream Glass` (asset `16824239322201846243`). Values below were read from the
HTML Stitch generated for the quick-settings screen, not guessed from the screenshot.
Stitch drew a 430 px wide phone; the device is 720 px wide, so **device px = css px x 1.674**
(rounded). Re-measure on the A21s before freezing.

Inspiration: modern frosted-glass phone UIs (large pills, capsule sliders). No other
vendor's names, logos or artwork are used.

## Colour

| Token | Value | Use |
|---|---|---|
| `cream-50` | `#FDFBF7` | lightest surface, art tile highlight |
| `cream-100` | `#F9F5EE` | |
| `cream-200` | `#F4EDE1` | base canvas (brand "warm cream") |
| `cream-300` | `#EADDC9` | |
| `cream-400` | `#DAC9B3` | sand (brand "icon tile") |
| `copper` | `#B8683A` | accent, active label, primary glyph |
| `copper-dark` | `#A55C32` | pressed accent (brand copper) |
| `copper-light` | `#E2976C` | glow, highlight |
| `plum` | `#302135` | text and icons on light glass |
| `plum-muted` | `#58435E` | secondary text |
| `plum-subtle` | `#7A6482` | tertiary text, hints |
| `mint` | `#3F9E8A` | positive state (mobile data, charging, ready) |
| `coral` | `#D95C51` | alert state (mute) |
| `violet` | `#8E6F9E` | secondary accent (design system only, not yet used in screens) |

Text alpha on glass: primary `plum` 100 %, label `plum` 80 %, secondary `plum` 65 %,
chevron / disabled `plum` 40 %.

## Glass surfaces

| Token | Fill | Blur | Shadow | Use |
|---|---|---|---|---|
| `glass-card` | white 45 % | 28 px, saturate 180 % | `0 10 30 -5` plum 6 %, 1 px white 70 % inner top | grouped cards, sliders, toggle grid |
| `glass-pill` | white 50 % | 24 px | `0 6 18 -3` plum 5 %, 1 px white 80 % inner top | wide pills (Wi-Fi, device hub) |
| `glass-circle` | white 42 % | 20 px | `0 4 14` plum 4 %, 1 px white 80 % inner top | inactive round toggles |
| `active-toggle` | white 100 % | none | `0 8 20 -3` copper 18 %, 1 px white 90 % inner top | active round toggle |
| ring | white 60 to 90 % 1 px | | | outline on every glass piece |

Wallpaper under glass: image blurred ~12 px at 85 % opacity, a vertical overlay
`white 35 % -> #F7EFE4 45 % -> #F2E5D2 60 %`, then a final 28 px blur at white 20 %.
Wallpapers must stay **warm and light**; plum text must reach at least 4.5:1 on them
(lock clock: 7:1).

## Shape

| Token | css px | device px | Use |
|---|---|---|---|
| `radius-pill` / `full` | 9999 | 9999 | pills, circles, dock, sliders |
| `radius-card` | 26 to 32 | 44 to 54 | grouped cards |
| `radius-tile` | 16 | 27 | app icon squares (first-party) |
| toggle circle | 48 | 80 | quick toggles |
| small circle | 36 to 40 | 60 to 67 | header buttons, pill icon discs |
| slider capsule | 122 tall | ~204 | brightness / volume |

## Type

| Role | Family | Size css / device | Weight |
|---|---|---|---|
| Headline, clock | Public Sans (Stitch design system says Outfit; the generated HTML used Public Sans: **decide**) | clock 96 / 160 | 300 light |
| Screen title | same | 20 / 33 | 600 |
| Body, labels | Plus Jakarta Sans | 13 / 22 | 600 title, 500 label |
| Caption | Plus Jakarta Sans | 11 / 18 | 500 |
| Overline | Plus Jakarta Sans | 10 / 17 | 600, uppercase, tracking wide |

Thai text: verify the chosen family has Thai glyphs or add a Thai fallback
(Plus Jakarta Sans has none). This is an open question, not solved here.

## Component rules

- Active toggle = solid white disc + coloured glyph (`mint` data, `coral` mute, `copper` others) + 8 px status dot top right; label turns `copper`, weight 600.
- Inactive toggle = `glass-circle` + `plum` 80 % glyph.
- Capsule slider: fill is white 75 to 95 % gradient rising from the bottom, icon sits at the bottom of the capsule, brightness icon `copper`, volume icon `plum`.
- Dock: floating `glass-card` capsule, 5 icons, 24 px gap to the gesture bar.
- First-party icons: bright rounded squares with a bold glyph; **third-party icons keep their own logo** (AGENTS.md rule).
- Gesture bar: 32 px zone, 128 x 4 px pill `plum` 35 %.

## Open items (need an owner decision or evidence)

1. Font: Outfit (design system) vs Public Sans (generated). Thai fallback.
2. Dark variant: keep the old plum theme as the dark mode, or drop it? Colour tokens above are light only.
3. Glass blur is expensive: the A21s renders in software/Mali with a 96 MiB CMA budget.
   Measure `backdrop-filter` equivalent cost with `rabbiz-de --stats` before committing to
   live blur; fall back to pre-blurred wallpaper plus flat translucent fills.
4. Stitch invented content (version strings, earbuds battery, weather) is placeholder only.

## Where this lands (when approved)

`crates/de-design/src/` (tokens only, no literals in screens) -> `crates/de-ui` screens ->
`tests/golden` re-bless -> ADR in `docs/DE-PLAN.md`. Order and effort belong to M2.
