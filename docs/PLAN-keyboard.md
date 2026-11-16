# Plan: `a21s-osk` — a Gboard-like on-screen keyboard written from scratch in Rust

> Written 2026-11-16 (roadmap item M7.12). Status: **PLAN ONLY — no code written.** Meant to be executed by several
> implementation agents in parallel; §9 splits the work into non-overlapping tracks, §10 is the brief each agent gets.
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
