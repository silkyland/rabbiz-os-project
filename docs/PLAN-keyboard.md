# Plan: `a21s-osk` — a Gboard-like on-screen keyboard written from scratch in Rust

> Written 2026-11-16 (roadmap item M7.12). Status (2026-11-17): **K1 host half implemented** — `tools/a21s-osk`
> (`osk-core` + `osk-render` + EN/TH layouts) builds and passes 29 host tests; the Wayland client, D-Bus service and
> device build have **not** started, and squeekboard is still the live keyboard. §12 records what changed. Meant to be
> executed by several implementation agents in parallel; §9 splits the work into non-overlapping tracks, §10 is the
> brief each agent gets.
> Evidence tags: **[V]** verified this session (command/file named), **[K]** from general knowledge, *not* checked here
> (a spike in §8 settles it), **[D]** decision made in this plan (changeable by ADR — rules here are hypotheses, not law).

## 1. What was asked

Rewrite the keyboard so it is as close to Gboard as possible: **English (US), Thai, emoji** first; clipboard on the bar;
next-word suggestions "would be great"; ask whether a very small AI is feasible or should be cut. Extending an existing
keyboard is allowed, a full Rust rewrite is allowed.

## 2. Where we are (evidence)

| Fact | Evidence |
|---|---|
| OSK today is **squeekboard 1.43.1-6** (GTK, Rust core), ~**95 MB RSS** | [V] `pacman -Q squeekboard`, `ps -o rss -C squeekboard` on device |
| Layouts are our own YAML overrides `us.yaml` (EN+TH+emoji views in one file) and `th.yaml` | [V] `rootfs-overlay/home/gamer/.local/share/squeekboard/keyboards/` |
| squeekboard has **no long-press, no "cycle input source" action, no suggestion bar**; our own file header says so and works around it with views | [V] header of `us.yaml`; [K] upstream feature list |
| Paste is a patch of ours: key action `paste` shells out to `wl-paste` | [V] `patches/squeekboard/0001-clipboard-paste.patch` |
| Language switch = `gsettings` reorder + `current=0`; show/hide = D-Bus `sm.puri.OSK0 SetVisible` | [V] `rootfs-overlay/usr/local/bin/a21s-osk-ctl` (used by Quick Settings tiles) |
| Dark mode needs a squeekboard **restart** (GTK3 settings read once) | [V] `docs/DESKTOP-SHELL.md` §3.2 |
| Hardware: 8 cores (`asimd fphp asimdhp`, A55-class), 2.6 GB RAM, ~1.3 GB available with the shell up, 9.7 GB disk free | [V] `free -m`, `nproc`, `/proc/cpuinfo`, `df -h /` |
| **Rust 1.100 is installed on the device**, `wl-clipboard 2.3.0`, `libxkbcommon 1.13.2`, `wayland-protocols 1.50`, `labwc 0.20.2` | [V] `pacman -Q` |
| **No emoji font is installed** (fontconfig lists 0 emoji/color fonts) — today's emoji view cannot render colour glyphs. `noto-fonts-emoji` is in the repos: 9.2 MiB download / 10.2 MiB installed | [V] `fc-list`, `pacman -Si noto-fonts-emoji` |
| Thai font present: Noto Sans Thai (+ Looped) | [V] `fc-list :lang=th` |
| No hunspell dictionaries, no `cliphist`, no `wtype` on device | [V] `ls /usr/share/hunspell`, `pacman -Q` |
| labwc exposes a data-control protocol (needed for clipboard history) | [K] claim in `docs/DESKTOP-SHELL.md` row M7.11; my `wayland-info` probe printed nothing → **re-check in spike S1** |
| Wayland IME plumbing in Rust exists: `smithay-client-toolkit` has a `seat::input_method` module (`ZwpInputMethodV2`); `wayland-protocols-misc` has `input-method-v2` and `virtual-keyboard-v1`; crates `zwp-input-method-service` / `imekit` wrap them | [V] [SCTK docs](https://docs.rs/smithay-client-toolkit/0.21.1/smithay_client_toolkit/seat/input_method/struct.ZwpInputMethodV2.html), [zwp-input-method-service](https://docs.rs/zwp-input-method-service/latest/zwp_input_method_service/), [imekit](https://docs.rs/crate/imekit/0.1.0). I found **no ready-made Rust on-screen-keyboard UI** to extend |
| Alternative OSK **Stevia** (ex phosh-osk-stub) already has a completion bar with pluggable completers (hunspell, presage-experimental, pipe, fzf, varnam) | [V] [man page](https://www.mankier.com/1/phosh-osk-stevia), [release notes](https://newreleases.io/project/gnome-gitlab/guidog/stevia/release/v0.47.0). It is C/GTK4, Phosh-oriented; Thai quality and RAM **unmeasured** |
| Repo note: `docs/DESKTOP-SHELL.md` says M7.12 is described in "§7.3", but §7 is *Risks* and there is no §7.3 — M7.12 had no real spec until this file | [V] `grep -n '^## ' docs/DESKTOP-SHELL.md` |

## 3. Pushback on the request (read this before approving)

1. **"Extend an existing keyboard" is the more expensive path here, not the cheaper one.** squeekboard can't do the three
   things that make Gboard feel like Gboard (long-press alternates, a suggestion/toolbar strip, swipe-on-space/backspace)
   without deep changes to its GTK widget code, and it is the thing costing 95 MB. Stevia has the bar and long-press but is a
   second large GTK stack we'd have to bend to Thai + our theme. **Recommendation [D]: rewrite as a small Rust program, keep
   squeekboard installed as a one-line rollback.** This is the owner's own suggestion; evidence supports it.
2. **"Small AI" — recommendation: cut the neural model from v1, keep a hole for it.** What users perceive as "smart" in a
   keyboard is mostly (a) completing the word being typed and (b) the 3 most likely next words. Both are done well, in a few MB
   and microseconds, by a **trie + n-gram (bigram/trigram) model with on-device learning** — no neural network. A neural LM
   would add training-data/licensing work, an inference runtime, and CPU on in-order A55 cores that are also running the
   shell and games, for a gain I expect to be marginal at this size *[K — measure in K8 before believing me]*. The code keeps a
   `Predictor` trait so a tiny quantised model can slot in later (K9) if the n-gram benchmark shows a real gap.
3. **"Gboard-like" is 80 % feel, not layout.** The things people notice: key preview bubble, instant feedback on touch-down
   (not touch-up), two-thumb typing without dropped keys, backspace that repeats and accelerates, shift/caps state machine,
   auto-capitalise, the action key changing with the field. These are in the scope of K2 and have acceptance numbers (§6).
   Glide typing (swipe-to-type) is the one famous Gboard feature I propose to **exclude** from v1 (needs its own decoder and
   a dictionary-quality bar we don't have yet) — see question Q2.
4. **Thai has no spaces**, so "complete the current word" and "predict the next word" both need a word segmenter in the loop.
   That is the real risk in the suggestion feature (§5.3), not the model size.

## 4. Architecture [D]

Own process `a21s-osk` (binary and crate keep the repo's technical `a21s-*` naming, see `docs/BRAND.md` §Naming), source in
`tools/a21s-osk/` (sibling of `tools/a21s-mirror/`), **pure Rust, no GTK/Qt**.

```
 Wayland compositor (labwc)
   │  zwlr_layer_shell_v1 (bottom anchor, exclusive zone = keyboard height → apps resize)
   │  zwp_input_method_v2 (activate/deactivate, surrounding_text, content_type, commit_string, delete_surrounding_text)
   │  zwp_virtual_keyboard_v1 (fallback: apps with no text-input, XWayland, nested weston/Steam, terminals)
   │  ext-data-control-v1 / zwlr_data_control_v1 (clipboard history — never needs to own the selection)
   │  wl_touch (multi-touch) + wl_pointer (mouse), wl_shm buffers
   ▼
 a21s-osk  ─ crates ─────────────────────────────────────────────────────────────
   osk-core     pure logic, no I/O: layout model, shift/caps FSM, gesture recogniser (tap/hold/swipe/repeat),
                input-state machine (what to commit/delete), Predictor trait, clipboard-history model, emoji model
   osk-predict  trie + n-gram (mmap'd, read-only) + user-learning overlay + Thai segmenter; builds `*.model` files offline
   osk-render   tiny-skia + cosmic-text (rustybuzz shaping, swash colour glyphs) → RGBA scene; damage-rect redraw only
   osk-wayland  all protocol glue + D-Bus (zbus) service; owns the event loop (calloop), zero timers when idle
   a21s-osk     composition root (the only crate that knows all the others)
 data/          layouts/*.toml (en-us, th-kedmanee, symbols), emoji/ (generated from Unicode + CLDR), models/*.bin
```
Dependency direction: `osk-core ← osk-predict, osk-render ← osk-wayland ← a21s-osk`; `osk-core` has **no** Wayland/GPU/font
dependency so it (and `osk-render` → PNG) builds and tests on the Mac host.

Why not QML in the shell [K]: Quickshell exposes no input-method-v2 client I know of, a shell crash would take the keyboard
with it, and software Qt Quick costs more RAM/CPU than a tiny-skia surface. Why not iced/slint [K]: another toolkit to fit
to Thai/emoji shaping and to multi-touch; the keyboard UI is a grid + strip + two panels — small enough to draw directly.
*Spike S2 measures the tiny-skia path on the device before this is locked.*

### Text-delivery rules (the part that usually breaks)
- Primary path: `zwp_input_method_v2` `commit_string` (+ `delete_surrounding_text` for backspace when the app supports it).
  Thai and emoji go through this; **no keymap trickery needed**.
- Fallback path (no active text-input — S6 lists which of Chrome, Qt apps, Dolphin, terminal, nested Weston are affected):
  `zwp_virtual_keyboard_v1` with a generated keymap that contains the Unicode keysyms of the active layout + control keys
  (squeekboard does the same). Backspace/Enter/arrows always use real keysyms.
- Keep `QT_IM_MODULE` / `GTK_IM_MODULE` **unset** (AGENTS.md rule stays); Chrome may need `--enable-wayland-ime` to use
  text-input-v3 **[K, S6]**.
- Only one input method per seat: `a21s-osk` and `squeekboard` must never both run. The switch lives in one place
  (`a21s-osk-ctl` + labwc `autostart`), default flips only at K7.
- Compatibility: keep the D-Bus name/object `sm.puri.OSK0` (`SetVisible`, `Visible`) so Quick Settings and `a21s-osk-ctl toggle`
  work unchanged; add our own `org.rabbiz.OSK1` for language, theme reload, clipboard, settings.
- **Sensitive fields:** if `content_type` hint says password/sensitive/no-learning, disable suggestions, learning and
  clipboard recording from that field. (security/data-integrity class: changes only by explicit ADR.)

## 5. Feature spec (what "Gboard-like" means, concretely)

All Gboard facts below are **[K]** (from memory of the Android app) — K0 task T-ref: owner supplies real reference screenshots
and recordings (Q1) and the layout TOMLs are calibrated against them, not against this text.

### 5.1 Layout (portrait; widths relative to one letter key = 1.0)
| Row | EN (US) | TH (Kedmanee) |
|---|---|---|
| strip | suggestion strip (3 slots) + toolbar toggle (see 5.4) | same |
| number row | `1 2 3 4 5 6 7 8 9 0` (setting, default on — matches our current `us.yaml`) | Thai digits ๑–๙ ๐ on the same row |
| 1 | `q w e r t y u i o p` | existing 12-key Kedmanee row from `th.yaml` (11–12 keys per row, tone marks) |
| 2 | `a s d f g h j k l` (row inset 0.5) | |
| 3 | `⇧(1.5)  z x c v b n m  ⌫(1.5)` | `⇧(1.5) … ⌫(1.5)` |
| bottom | `?123(1.5)  ,  😀/🌐  space(≈5)  .  ⏎(1.5, accent colour, icon = field action)` | `?123  ,  😀/🌐  space(Thai label)  .  ⏎` |

Visual: rounded-rect keys (~8 dp radius), letter keys one tone, function keys a second tone, action key = accent. Dark/light
from the shell theme (§7). Key labels use Noto Sans Thai for Thai; combining marks must render (dotted-circle base) — S4.

### 5.2 Gestures and behaviour (acceptance numbers in §6)
- Touch-down highlights + shows a **preview bubble** above the key (suppressed for function keys and when the bubble would leave the surface).
- Character commits on **touch-up** (so a slide-off cancels) *or* touch-down — pick by S2 latency measurement; default Gboard behaviour is touch-up.
- **Long-press (≈400 ms)** → alternates popup (a → à á â ä æ ã å ā; number-row digits on top-row letters; `.` → `, ? ! : ; …`; Thai keys → tone/vowel variants, ๑–๙ digits); release over an item to commit; 🌐 long-press → language list.
- **Shift:** tap = one-shot; double-tap / long-press = caps lock; auto-capitalise at sentence start using `surrounding_text` + `content_type` (not for URL/email/password fields).
- **Backspace:** tap deletes one *grapheme cluster* (Thai: a consonant with its vowel/tone marks goes as one step — decision per Q3); hold repeats, accelerating after ~1 s; swipe-left while held deletes by word.
- **Space:** tap = space (double-space → ". " only if enabled); **swipe left/right on space = move cursor**; 🌐 cycles EN → TH → (emoji stays a panel).
- **Enter / action key:** icon and `enter_key_hint` follow the field (⏎ / go / search / send / next).
- **Multi-touch:** keys are tracked per touch id; a second finger never cancels the first (two-thumb typing).
- **Symbols pages:** `?123` → numbers/punctuation page, `=\<` → second symbols page, as Gboard.
- Haptic/click feedback optional (Q5; `feedbackd` is installed, vibrator support on mainline for the A21s is **unknown**).

### 5.3 Suggestions and the small-model decision
- **Strip:** 3 candidates. Left = current word as typed (Gboard shows it quoted), centre = best completion/correction, right = alternate. After a space the strip shows **next-word** candidates. Tap commits the word + a space.
- **Tiers** (each tier ships on its own; the product works at every tier):
  - **T0** prefix completion: trie + unigram frequency, per language (EN, TH).
  - **T1** bigram/trigram **n-gram** model (pruned, quantised, memory-mapped read-only file) + **user learning overlay** (counts persisted in `$XDG_STATE_HOME/a21s-osk/user.db`, local only, capped size, one-key "forget" in settings, never written for sensitive fields) + simple typo correction (edit-distance-1 over the trie, keyboard-adjacency weighted).
  - **T2** *(deferred, K9)* tiny quantised neural LM behind the same `Predictor` trait — only if the K8 benchmark shows T1 leaves a measurable gap.
- **English:** word list + counts from open corpora (candidates: wordfreq data, Wikipedia dumps — **licence of each must be checked in S5 before use**).
- **Thai:** needs a segmenter. Plan: dictionary **longest-/maximal-matching** (newmm-style) on a trie, written in `osk-predict` (≈ a few hundred lines) or via a Rust Thai wordcut crate / `libthai` **[K — verify crate and licence in S5; LibThai is LGPL, LEXiTRON dictionary has its own licence, NECTEC transliteration corpus is CC BY-NC-SA — non-commercial/share-alike, so not assumed usable]**. Current-word rule: segment the last ~30 characters before the cursor (from `surrounding_text`, else our own committed-text buffer), treat the last token as the prefix if it is a proper prefix of longer dictionary words. Thai also gets an optional sequence-validity filter (rejects impossible mark orders, like `fcitx5-libthai` does).
- **Footprint targets** [D, unmeasured]: all models ≤ 30 MB on disk, ≤ 15 MB resident (mmap pages), lookup ≤ 5 ms per keystroke on the device.
- **Quality metric** (S5/K8): keystroke-saving rate and top-3 hit rate on a held-out text set, EN and TH, reported against a trivial baseline (frequency-only). Ship T1 only if it beats T0 by a margin we write down *before* looking at the result.

### 5.4 Toolbar and clipboard
- The strip's left button opens the **toolbar** (as Gboard's `>`): clipboard, emoji, (settings, language later).
- **Clipboard panel:** recent text items (newest first), tap = insert at cursor via `commit_string`, **pin** (persisted), **delete**, "clear all". Unpinned items expire after a configurable time (default 1 h, as Gboard) and are **memory-only** — never written to disk; pinned ones persist in `$XDG_STATE_HOME`.
- Source: data-control protocol, text only in v1 (images/files later). Skip when the offer carries a password-manager hint mime (`x-kde-passwordManagerHint` convention **[K]**) or the focused field is sensitive.
- Boundary with M7.11 (shell clipboard system): history logic lives in a library module (`osk-core::clip`, data-control reader in `osk-wayland`) with a stable API, so M7.11 can later extract it into a daemon without a rewrite. Whoever takes M7.11 must coordinate with this plan; until then the keyboard is the only history owner and `cliphist` is **not** installed.

### 5.5 Emoji panel
- Category tabs along the bottom (recent, smileys, people, animals, food, activities, travel, objects, symbols, flags), scrolling grid with kinetic scroll, **search** (EN and TH via Unicode CLDR annotations — `cldr-emoji-annotation` is in the Arch repos [V `pacman -Ss emoji`]), **recents**, **skin-tone** long-press, ZWJ sequences.
- Data generated offline from Unicode `emoji-test.txt` + CLDR annotations into a compact binary/RON file (build tool in `osk-predict` or a `dict-build` bin; licence = Unicode licence **[K, verify]**).
- Font: **install `noto-fonts-emoji`** (10 MiB). Whether Arch's build is CBDT-bitmap or COLRv1 matters for `swash` rendering — S3 checks. If colour rendering via cosmic-text fails, fallback is pre-rasterising the glyph atlas at build time.
- Today's emoji view in `us.yaml` currently has no colour font to draw with — this is a pre-existing bug independent of this plan.

## 6. Quality bars (targets [D]; K8 measures them on the device, nothing here is measured yet)

| Bar | Target |
|---|---|
| idle CPU | 0 % (no periodic timers; event-driven only) |
| resident memory, keyboard shown, T1 models mapped | ≤ 30 MB RSS (squeekboard today: ~95 MB [V]) |
| cold start to first frame | ≤ 300 ms |
| touch-down → pixels changed | ≤ 1 frame (16 ms @ 60 Hz) |
| touch → `commit_string` on the wire | ≤ 20 ms (core+wayland, excluding app) |
| two-finger rapid typing | 0 dropped/duplicated keys over a 300-key scripted run |
| predictor | ≤ 5 ms per keystroke at p99 on the device |
| redraw | damage rects only; a keypress repaints ≤ the key + bubble + strip |
| rollback | squeekboard restored by one command; verified each milestone |

## 7. Integration with the shell and rootfs

- **Theme:** the shell's `Appearance` service (already generates GTK/Qt settings) additionally writes
  `~/.config/a21s-osk/theme.toml` (dark/light, accent, radius, font) and pokes `org.rabbiz.OSK1.ReloadTheme`; the keyboard
  repaints **without restart** (squeekboard needed one). No literal colours in the keyboard source except the fallback theme.
  (Shell code rule: design tokens live in `Theme/Theme.qml` only — the OSK consumes them via the generated file.)
- **Nav bar / apps:** keyboard is a layer-shell surface with an exclusive zone; the shell NavBar (auto-hide) and labwc `<margin>`
  must be checked together — acceptance in K7: focused text field is never hidden behind the keyboard in Chrome, Dolphin, a QML app, and the terminal.
- **Quick Settings:** Keyboard and Language tiles keep working through the compat D-Bus names; Language cycles inside `a21s-osk` (no `gsettings` juggling).
- **Rootfs:** new files under `rootfs-overlay/` (`/usr/local/bin/a21s-osk`, `/usr/share/a21s-osk/{layouts,models,emoji}`, labwc `autostart` switch); package dep `noto-fonts-emoji` added to the install script. `deploy_shell.sh`-style deploy script `scripts/deploy_osk.sh` re-chowns `/`, `/home`, `/home/gamer` and uses `--no-same-owner` (AGENTS rule).
- **AGENTS.md:** at K7 replace the squeekboard-specific bullets (IM env vars stay) and document the new "never run two IMs", sensitive-field and device-queue rules.

## 8. Milestones

Every milestone ends with: host tests green, `scripts/check.sh` still green, device smoke via `scripts/device_session.sh` (never concurrent device use), squeekboard rollback still works, ROADMAP M7.12 line updated.

| # | Milestone | Done when |
|---|---|---|
| **K0** | **Spikes** (parallel, ≤ ½ day each, results appended to this file §12) | see list below |
| **K1** | Walking skeleton | `a21s-osk` shows a QWERTY layer-shell surface, tap commits to a test text-input client through im-v2, activate/deactivate show/hide, `sm.puri.OSK0` compat works, host PNG snapshot test exists, `osk-testclient` logs commits |
| **K2** | Gboard feel (EN) | all §5.2 behaviours incl. long-press, shift FSM, repeat backspace, space-swipe, preview bubble, action key, symbols pages, multi-touch; §6 latency/RSS bars measured |
| **K3** | Thai + language switch + fallback | Kedmanee layout from `th.yaml`, tone/vowel alternates, grapheme-aware backspace, language cycle, virtual-keyboard fallback with generated keymap proven in terminal + nested Weston |
| **K4** | Emoji | panel, search EN+TH, recents, skin tones, colour glyphs rendering on device |
| **K5** | Strip + toolbar + clipboard | strip UI with a stub predictor, toolbar, clipboard history/pin/delete/expiry, sensitive-field suppression verified |
| **K6** | Predictor T0 → T1 (EN first, then TH) | offline model build tool, mmap loader, user learning overlay, §5.3 metrics reported |
| **K7** | Shell integration + switch default | theme file + live reload, nav-bar coordination, QS tiles, autostart default flips to `a21s-osk`, docs/AGENTS/ARCHITECTURE updated, `deploy_osk.sh` |
| **K8** | Hardening + measurement | 1 h soak without RSS growth, fuzz tests for gesture/input FSMs, all §6 bars measured, T2 go/no-go ADR |
| K9 | Later / optional | T2 tiny neural LM, glide typing, more languages, clipboard images, floating/split layouts |

### K0 spikes
- **S1 Protocol census** — as `gamer` on the device list which of `zwp_input_method_manager_v2`, `zwp_virtual_keyboard_manager_v1`, `zwlr_layer_shell_v1`, `ext_data_control_manager_v1`/`zwlr_data_control_manager_v1`, `zwp_text_input_manager_v3` labwc 0.20.2 offers. (Decides clipboard feasibility.)
- **S2 Render path** — Rust hello-world: layer-shell surface + tiny-skia + cosmic-text on the device (cross-compile from the Mac *or* build on the device where Rust is installed — pick one and document it in `docs/BUILD.md`); measure RSS, touch-down→frame latency, 3-finger correctness. Gate for the §4 architecture.
- **S3 Emoji glyphs** — render Noto Color Emoji through cosmic-text/swash (check CBDT vs COLRv1 in the Arch package).
- **S4 Thai shaping** — render the full Kedmanee key labels and a sentence with stacked marks; confirm combining marks display on key caps.
- **S5 Data and licences** — pick EN and TH word-frequency/n-gram sources and the Thai segmenter; write down each licence; run a Python/host baseline for keystroke-saving rate (research only; production builder is Rust).
- **S6 App/protocol matrix** — which text protocol each target app uses (Chrome with/without `--enable-wayland-ime`, Qt Quick/QtWidgets, Dolphin/Kate, terminal, nested Weston/Steam, XWayland) → decides how much the virtual-keyboard fallback must cover.
- **S7 Haptics/sound** — does the A21s have a working vibrator or `feedbackd` profile on mainline.

## 9. Work split for parallel agents (disjoint file ownership)

After K1 freezes the cross-crate APIs, tracks run in parallel. Each track owns its directory; nobody edits another's files — API changes go through the plan owner.

| Track | Owns | Needs |
|---|---|---|
| A core + layouts + gestures | `tools/a21s-osk/crates/osk-core/`, `data/layouts/` | none (pure Rust; host tests) |
| B render | `crates/osk-render/` | `osk-core` scene types |
| C wayland + D-Bus | `crates/osk-wayland/`, `osk-testclient` | S1, S2 |
| D predictor + data build | `crates/osk-predict/`, `tools/a21s-osk/dict-build/`, `data/models/` | S5 |
| E emoji + clipboard | emoji data generator, `osk-core::clip`, panels' scene code | S1, S3 |
| F shell integration | `rootfs-overlay/` changes, `scripts/deploy_osk.sh`, `scripts/verify_osk.sh`, QML tiles, docs | K2+ |

K0 spikes are independent of each other. K1 is done by **one** agent (it fixes the seams). After that A/B/C/D can overlap; E after C; F last.

## 10. Brief template for each implementation agent

1. Read `AGENTS.md` (ground rules apply: **serialize device use with `scripts/device_session.sh`**, deploy only your own files, never leak `~/.credentail`, no `tar -C /` without `--no-same-owner`, keep `QT_IM_MODULE`/`GTK_IM_MODULE` unset) and this file §4–§7.
2. State your milestone/track, the files you own, and what "done" means (copy the row from §8).
3. Write tests first for pure logic (`cargo test` on the host); renderer work must produce a PNG snapshot you compare to the Gboard reference screenshots.
4. Report facts vs guesses; record any rule you think is wrong with evidence (ADR text in the same change).
5. Leave squeekboard working; never run two input methods on the seat.

## 11. Questions only the owner can answer

- **Q1** Can you provide Gboard reference screenshots/screen recordings (EN, TH, symbols, emoji, clipboard) from a real Android phone, same screen size if possible? Without them "as close as possible" is calibrated from my memory.
- **Q2** Is **glide typing** (swipe to type) wanted? (Proposed: not in v1.)
- **Q3** Thai backspace: delete the whole consonant+marks cluster (Gboard-like) or one code point? (Proposed: cluster.)
- **Q4** Is learning from what you type (stored locally, one-tap forget, never for password fields) acceptable?
- **Q5** Key click sound / vibration wanted? (Depends on S7.)
- **Q6** Thai layouts: Kedmanee only (as now), or also Pattachote?
- **Q7** Languages after EN/TH (e.g. German, since the customer base is German)? Affects model/data pipeline, not v1.

## 12. Spike results

*(each spike appends: date, command/measurement, result, what it changes in this plan)*

### 2026-11-16 — K0 first pass (device `7.3.0-g14a425b3a7dc-dirty`, labwc 0.20.2)

| Spike | Status | Result | Effect on plan |
|---|---|---|---|
| **S1** protocol census | **done (static)** | `strings /usr/bin/labwc` shows `wlr_input_method_manager_v2_create`, `wlr_text_input_manager_v3_create`, `wlr_layer_shell_v1_create`, `wlr_data_control_manager_v1_create` **and** `wlr_ext_data_control_manager_v1_create`, plus a virtual-keyboard manager (2 string hits). A live `wayland-info` listing was not taken (`wayland-utils` not installed, 68 KiB in repo) | §4 protocol set is available; clipboard history via data-control is feasible. Live confirmation can reuse the S2 binary (it binds layer-shell, shm, seat) |
| **S3** emoji font | **done** | `pacman -S noto-fonts-emoji` installed (`/usr/share/fonts/noto/NotoColorEmoji.ttf`, 10.7 MB). Table directory has **`CBDT`/`CBLC`, no `COLR`** → bitmap colour emoji. Rendering through cosmic-text/swash is **not yet proven** (S2 binary not built) | §5.5 keeps the fallback (pre-rasterised atlas). **System change made:** emoji font and `pkgconf` now installed on the device (both needed for the build / plan) |
| **S7** haptics | **done (static)** | `/sys/class/input/*/name`: touchscreen TD4150, `s2mpu12-pwrkey`, `gpio-keys`, `AUD3004X Headset Input` — **no force-feedback/vibrator input device, no LEDs**. `feedbackd 0.8.9` is installed but has nothing to drive | Q5: haptics unavailable for now; sound click only if wanted |
| **S6** app matrix | **partial** | `a21s-browser` (Chrome) passes `--force-device-scale-factor=1.5` and **no** `--enable-wayland-ime` → today Chrome may not use text-input-v3 and would depend on the virtual-keyboard fallback or a flag change. Other apps not tested | Chrome flag decision moved to K3; rest of matrix still open |
| **S5** data/licences | **research only** | `wordcut-engine` (Rust Thai segmenter): current 1.1.x lists Apache-2.0, an old 0.3.1 was LGPL-3.0, and it ships a separate `LICENSE-PyThaiNLP` for bundled data → check the exact version. `nlpo3` + `words_th.txt` (~62k words) is described as CC0 only by secondary sources → confirm in the PyThaiNLP repo. `wordfreq`: code Apache, data CC BY-SA 4.0 with separately-licensed sources (Google Books, OpenSubtitles) and **Thai coverage not confirmed**. Wikipedia text is CC BY-SA. CLDR/Unicode licence for emoji annotations **not confirmed**. NECTEC corpora noted earlier are non-commercial | Nothing is cleared for redistribution yet. CC BY-SA data would make shipped model files share-alike — decide with Q4/Q7 before K6. Thai segmenter: prefer own newmm-style code over a dependency until the licence is confirmed |
| **S2** render on device | **blocked** | Spike crate written: `tools/a21s-osk/spikes/s2-render/` (layer-shell + tiny-skia + cosmic-text, `--png` mode for S3/S4, touch→commit latency and RSS logging). First device build failed for missing `pkg-config` (fixed by installing `pkgconf`). During the second build **the device stopped answering SSH and ping** (cause unknown: likely RAM exhaustion from compiling cosmic-text on 2.6 GB with the shell + Steam running, or a crash/reboot — not diagnosed) | **Do not compile on the device.** Cross-compile on the Mac (needs `aarch64-unknown-linux-gnu` target + a linker such as zig/`cargo-zigbuild` or an aarch64 sysroot; none installed on the host) or limit the build (`-j1`, `CARGO_BUILD_JOBS=1`, `opt-level=1`) after confirming free RAM. Record the chosen build route in `docs/BUILD.md` |
| **S4** Thai shaping | **blocked on S2** | `--png` mode of the spike renders the Kedmanee sample row, stacked-mark words and emoji; not run | — |

### 2026-11-17 — K1 host slice (`tools/a21s-osk`, no device)

| Item | Result |
|---|---|
| **Core (Track A)** | `osk-core`: layout TOML → layers/keys, per-row normalised geometry with insets, hit testing, shift/caps FSM (single tap = one-shot, double tap ≤ 300 ms = lock), grapheme-cluster backspace (`unicode-segmentation`), space-swipe cursor moves, double-space period, auto-capitalise, language cycle, gesture recogniser (tap on release, slide-off cancel, 400 ms long-press, 50 ms repeat, horizontal swipe, independent multi-touch). |
| **Render (Track B)** | `osk-render`: tiny-skia rounded keys + cosmic-text labels → pixmap/PNG; per-layer surface height (the emoji panel is one row shorter). 24 PNGs written by `cargo run -p osk-render --bin osk-preview` to `build/osk/` (2x, dark/light/shift). |
| **Layouts** | `data/layouts/en-us.toml` (5 layers, alternates) and `th-kedmanee.toml` (6 rows, Thai numerals, tone-mark alternates) — Kedmanee rows copied from the deployed squeekboard YAML so the owner's muscle memory carries over. |
| **Tests** | 29 host tests green (`cargo test --manifest-path tools/a21s-osk/Cargo.toml`); `scripts/check.sh` runs them when `cargo` exists. Reviewed PNGs: EN base/numbers/emoji, TH base, shifted EN. |

Effects on the plan:

- **S3 — partial (host only).** Apple Color Emoji renders **in colour** through cosmic-text/swash on the Mac (sbix path), so the scene model is right. The device question is unchanged: Arch ships CBDT/CBLC `noto-fonts-emoji`, whose swash path is unproven → keep the pre-rasterised-atlas fallback in §5.5.
- **S4 — partial (host only).** Kedmanee key labels shape correctly, including stacked marks on key caps (dotted-circle base appears, as Gboard). Noto Sans Thai on the device is still unproven, and the renderer must not assume `Family::SansSerif` resolves to a Thai font on Arch — pick the family explicitly at K3.
- **S2 — still blocked.** No aarch64 Linux target or linker on the Mac. Pick one before K1 device work and write it into `docs/BUILD.md`: (a) `rustup target add aarch64-unknown-linux-gnu` + `zig`/`cargo-zigbuild`, or (b) an ARM64 Arch sysroot. Do **not** compile the spike on the device (first attempt cost the device its SSH).
- **K1 acceptance splits** into host half (done: layout maths, commands, PNGs) and Wayland half (`osk-wayland` + `osk-testclient`, still to write).
- **Renderer rule [V].** Text must go through cosmic-text's own `Buffer::draw`; hand-placing `swash` glyph images double-printed glyphs in every label (the first three attempts did) because baseline/placement conventions were being re-derived. Do not reintroduce manual glyph placement.
- **Surface height decision [D].** One fixed surface height per language: the base layer's row count fixes it, and a layer with fewer rows (numbers, symbols, emoji) gets proportionally taller keys instead of a shorter keyboard. This is what Gboard does and it keeps the layer-shell exclusive zone from resizing on every `?123` tap. Do not reintroduce per-layer heights without checking the app re-flow cost on device.

### 2026-11-17 — first run on the device (S2/S3/S4 device evidence)

Built in a `rust:1.99-bookworm` container (Docker Desktop runs it as **native aarch64**, so no
cross toolchain was needed — route recorded in `docs/BUILD.md` §6), then installed over SSH:

| What | Result |
|---|---|
| `a21s-osk-preview` (our renderer) at `/usr/local/bin/` | 24 PNGs in **1.56 s** on the device; EN and TH reviewed from the phone's own fonts |
| Thai shaping on device | **works** — Noto Sans Thai, Kedmanee labels + stacked marks, no tofu, no manual atlas |
| **S3 emoji on device** | **works in colour** — Arch's CBDT/CBLC `noto-fonts-emoji` renders through cosmic-text/swash, so the §5.5 "pre-rasterised atlas" fallback is **not needed** (keep it only as a contingency) |
| **S2 spike on the real screen** | `osk-spike` (layer-shell + shm + touch) drew a 720×640 bottom surface over the running shell: setup 63 ms, **first draw 109–139 ms**, RSS **32–36 MB**, touch capability bound. Screenshot: `build/osk-device/spike-on-device.png` |
| Install state | both binaries are installed but **inert**: the preview only writes PNGs, the spike is a demo that grabs no text input. squeekboard is still the live keyboard and no session unit was touched |

Consequences:

- **Build route settled.** For any Rust that must run on the phone: build in the arm64 Rust
  container, copy the binary, run it under the device lock. No `cargo-zigbuild`, no sysroot.
- **S2 gate passes** for layer-shell + shm + cosmic-text at 720 px: latency and RSS are inside
  the §6 bars (first frame ≤ 300 ms; RSS ≤ 30 MB is *not* met by the spike at 32–36 MB, but the
  spike carries six rows and no model — measure again in K1 with the real scene).
- **The remaining work is the text path**, not rendering: `zwp_input_method_v2` (commit/delete,
  surrounding text) and the virtual-keyboard fallback. That is `osk-wayland`, still unwritten.

### 2026-11-17 (later) — K1 device half: the keyboard types on the phone

`crates/osk-wayland` + binary `a21s-osk` (SCTK 0.19 for layer-shell/seat/shm, raw
`zwp_input_method_v2` from `wayland-protocols-misc`). It draws the real `osk-core` layouts through
`osk-render`, turns `wl_touch` into the gesture detector, and delivers text with
`commit_string` / `delete_surrounding_text` + `commit(done_count)`.

Verified on the A21s with a GTK4 test client (`tools/a21s-osk/tests/osk_test_client.py`), touch
injected through `scripts/touch_swipe.py`:

| Case | Result |
|---|---|
| focus an entry → keyboard appears | surface maps at 720×330, exclusive zone reserves it, entry stays visible |
| tap `a`, `b` | client log `changed: 'A'`, `changed: 'b'` (auto-capitalise armed on the empty field) |
| switch to Thai (TH key), type, backspace | `changed: 'ห'` then `changed: ''` — one Thai grapheme deleted cleanly |
| Thai number row | shows `1 2 3 …`; tap → `1`; shift then tap → `๑` |

**Bug found and fixed:** `zwp_input_method_v2` text lengths are **UTF-8 bytes**, not characters.
`Command::Backspace` now carries `bytes` (a Thai consonant+mark cluster is 9 bytes), otherwise
backspace cut a Thai character in half. Host tests updated (30 total).

Install/switch (root, device): `a21s-osk-switch start|stop|status` →
`rootfs-overlay/usr/local/bin/a21s-osk-switch`. It kills the other IM first and
waits, because the seat allows exactly one. **Squeekboard is still the session default**; the
autostart was not touched, so a reboot always returns to the known-good keyboard.

Known gaps before flipping the default (K7):

- backspace **hold-to-repeat** — the gesture detector emits `Repeat`, but the Wayland loop has no timer yet;
- long-press **alternates popup** not wired to the UI (the data is in the layouts);
- Thai has no shifted layer yet (the squeekboard YAML had `ฤ ฆ ฏ โ ฌ ็ ๋ ษ ศ ซ ฅ` on shift);
- emoji panel is a static 30-key page (no category scroll/search/recents);
- no `sm.puri.OSK0` D-Bus compat, so the Quick Settings keyboard/language tiles still target squeekboard;
- no theme file from `Appearance` (fixed dark theme today);
- no `zwp_virtual_keyboard` fallback → Chrome/XWayland text fields need `--enable-wayland-ime` or the fallback (K3).

Screenshots: `build/osk-device/step7-kb-test.png` (keyboard over the client),
`step8-typed.png` (`A`+`b` committed), `step10-thai-digit.png` (`๑`), `step9-thai.png` (Thai layout).

### 2026-11-17 (final) — owner's Gboard reference: panel, Thai digits, input purpose

Owner sent Gboard screenshots (Thai + English) and said "follow this design". Changes:

| Area | Now |
|---|---|
| Thai layout | exactly the owner's latest Gboard reference: **no separate digit row** — the top row is the Kedmanee letter row (`ๅ / _ ภ …`) with the Arabic `1…0` as the small corner hint; shift commits that digit. Thai digits ๑…๐ live on the `๑๒๓` page (first row) |
| Panel (strip) | Gboard layout: `[clipboard]` · three suggestion slots with dividers · `[mic]`; the word being typed is drawn in the first slot. Clipboard pastes the current selection via `wl-paste` (history is K5); mic is a deliberate stub (no audio input on mainline) |
| Bottom row | `?123 , 😀 🌐 space . ⏎` — globe key replaces the TH/EN text key; space keeps the language name (`English` / `ไทย`). All function keys (shift, backspace, return, globe, emoji, clipboard, mic) are drawn with **bundled Material Icons** (Google, Apache-2.0) instead of emoji/text glyphs |
| Input purpose | `zwp_text_input_v3` content_type is followed: email → `@ .com` row, URL → `/ .com :`, digits/number/phone → number pad. Email/URL force the Latin layout even in Thai, and auto-capitalise is off outside prose |
| Thai rare letters | ฤ ฆ ฏ ฎ ษ ศ ซ live on long-press (squeekboard had them on a separate "upper" view); `๑๒๓` opens the Thai-digit number page |
| Backspace hold | repeats every 50 ms and stops at the start of the text |
| Theme | `~/.config/a21s-osk/theme.toml` (`mode`, `accent`) applied live via `org.rabbiz.OSK1.ReloadTheme` |
| Shell compat | owns `sm.puri.OSK0` (SetVisible + Visible) so `a21s-osk-ctl toggle` and the Quick Settings tiles work; `NextLanguage` on `/org/rabbiz/OSK1` drives the language tile |
| Default | **`a21s-osk` is the session keyboard** (`~gamer/.config/labwc/autostart` prefers it, squeekboard is the fallback); `a21s-osk-switch` still flips back |

Verified on the device (screenshots in `build/osk-device/`): Thai panel with Thai digits +
Arabic hints, English panel, email field with the `@`/`.com` layout, long-press alternates,
backspace repeat, live theme swap, hide/show via D-Bus, language switch via D-Bus.

Still open (documented, not blocking daily use): suggestions are a compose word rather than a
predictor (K6), clipboard is paste-only (K5), emoji is a static page (K4), mic/voice, and the
`zwp_virtual_keyboard` fallback for Chrome/XWayland fields.

### 2026-11-17 (K6/K5 slice) — predictor + clipboard history

- **K6 (predictor, T1-lite)**: `osk-core::predict` — hand-written EN/TH seed words (no third-party
  corpus, so nothing to license), plus **user learning**: unigrams + bigrams counted from what the
  user types, persisted to `$XDG_STATE_HOME/a21s-osk/predictor.tsv` (atomic write, 2 s debounce).
  The strip shows 3 suggestions (completion of the current word, then next-word from bigram, then
  frequency); tapping one deletes the partial word and commits `word + space`. Tests cover
  completion, bigram prediction, tap-to-apply and persistence.
- **K5 (clipboard history)**: a `wl-paste --watch` child (wl-clipboard uses data-control, so it
  works without keyboard focus) feeds a memory-only list of the last 20 selections; the clipboard
  button opens a sheet listing the 5 newest, tap inserts and closes. Nothing is written to disk.
- **K4 (emoji categories/search/recents)**: still the static single page — the remaining piece of
  the owner's list.

### 2026-11-17 (K4 slice) — emoji categories, search and recents

- **Data pipeline**: `tools/a21s-osk/dict-build/build_emoji.py` downloads nothing at build time
  but reads the cached `data/raw/{emoji-test.txt,cldr-en.xml,cldr-th.xml}` and generates
  `data/emoji.tsv` (char, group, English name, Thai name — 253 emoji across the 9 Unicode groups;
  Unicode + CLDR licences) plus `data/layouts/emoji.toml` (one layer per group, a search layer,
  a results layer and a recents layer).
- **Panel**: the emoji key now opens `emoji` as a *panel layout* (`KeyAction::Panel`), with
  `◀ / ▶` group paging, `abc` back to the app layout, a Material `search` key and the recents
  layer shown first once the user has picked an emoji (persisted in
  `$XDG_STATE_HOME/a21s-osk/emoji-recent.txt`, 24 entries).
- **Search**: `search` opens a compact letter layer; typing filters `EmojiIndex` by English **and
  Thai** CLDR names, `search` again shows the results grid, tapping a result commits it and
  records the recent. Backspace edits the query instead of the app text.
- **URL/email fields**: the globe key is back in the `email`/`url`/`num` bottom rows (owner report:
  Chrome's omnibox could not switch language).
- Tests: panel open, group paging, search-by-English-name, result commit + recents, plus Thai-name
  search on the index directly.

### 2026-11-17 (input fixes) — purpose re-sends, globe key, Enter

- **Chrome re-sends `content_type` on every keystroke**, and `set_purpose()` used to force the
  Latin layout each time, so typing the first Thai character in the omnibox bounced back to
  English. `set_purpose()` is now idempotent (acts only when the purpose changes).
- The `email`/`url`/`num` bottom rows carried the literal `🌐` **emoji token**, which the parser
  treats as a text key: the globe was blue and tapping it typed an emoji instead of switching
  language. They now use the `language` key (Material icon).
- **Enter did nothing**: `commit_string("\n")` is ignored by single-line fields (Chrome omnibox,
  search boxes). The client now owns a `zwp_virtual_keyboard_v1` with a minimal xkb keymap and
  sends a real Return key press/release (evdev code 28) — verified by a Chrome search that
  actually navigated. Arrow keys will use the same path (K2).

