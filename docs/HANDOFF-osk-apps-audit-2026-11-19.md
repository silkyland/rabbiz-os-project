# Structural Audit Handoff: `tools/rabbiz-osk` & `tools/rabbiz-apps` (2026-11-19)

Lane `de`, companion workspaces to `tools/rabbiz-de` (see [HANDOFF-de-audit-2026-11-19.md](HANDOFF-de-audit-2026-11-19.md)).  
Evidence level: **L0 (static audit + host tests on `rabbiz-osk`)**. Read-only audit; no changes applied to uncommitted `rabbiz-osk` or `rabbiz-apps` files.  
**Not tested on device.**

---

## 1. `tools/rabbiz-osk` (3 crates, ~4.7k LOC)

### What Works Well
- Clean 3-crate separation: `osk-core` (pure layout/gesture/prediction logic) → `osk-render` (`tiny-skia` + `cosmic-text`) → `osk-wayland` (Linux-only Wayland client).
- Strong host test coverage: `cargo test --manifest-path tools/rabbiz-osk/Cargo.toml --workspace --exclude osk-wayland` passes **45 tests** (`gesture_tests` 6, `hw_keyboard_tests` 1, `keyboard_tests` 23, `layout_tests` 10, `render_tests` 5) in ~0.8 s, already gated in [scripts/check.sh](../scripts/check.sh).

### Structural Findings
- **P1 — Monolithic Wayland binary**: [osk-wayland/src/main.rs](../tools/rabbiz-osk/crates/osk-wayland/src/main.rs) is **1,414 LOC** in a single file (mixing layer-shell, `zwp_input_method_v2`, virtual keyboard, SHM buffers, shell-IM socket bridge, D-Bus, and event loop). Split into `wayland.rs`, `input_method.rs`, `shm.rs`, `bridge.rs`, `main.rs`.
- **P2 — Missing workspace lints & MSRV**: [tools/rabbiz-osk/Cargo.toml](../tools/rabbiz-osk/Cargo.toml) omits `[workspace.lints]` (`unsafe_code = "warn"`, `clippy::pedantic`) and `rust-version`, unlike `tools/rabbiz-de/Cargo.toml`.
- **P2 — Manifest & version drift**:
  - [osk-render/Cargo.toml](../tools/rabbiz-osk/crates/osk-render/Cargo.toml) declares `tiny-skia = "0.11"` in both `[dependencies]` and `[dev-dependencies]` (duplicate).
  - `tiny-skia = "0.11"` lags `rabbiz-de` (`0.12`).
  - `data/fonts/MaterialIcons-Regular.ttf` duplicates font assets bundled in `tools/rabbiz-de/assets/fonts/`.
- **P3 — Stale prototype & test warning**:
  - `tools/rabbiz-osk/spikes/s2-render/` is excluded from the workspace (`exclude = ["spikes/s2-render"]`) — archive or remove once superseded.
  - Minor compiler warning (`unused_mut`) in `osk-core/tests/keyboard_tests.rs:237`.

---

## 2. `tools/rabbiz-apps` (13 crates, ~33.9k LOC)

### What Works Well
- Reuses `de-api`, `de-config`, `de-design`, and `de-paint` directly from `../rabbiz-de/crates/` through shared `rabbiz-app-kit`, keeping all 12 pure-Rust Wayland apps visually aligned with the shell.

### Structural Findings
- **P1 — Missing from `scripts/check.sh`**: Despite being the largest Rust workspace in the repo (~33.9k LOC across 13 crates), `tools/rabbiz-apps` has **no host test gate** in [scripts/check.sh](../scripts/check.sh).
- **P1 — Oversized `main.rs` monoliths**:
  - `rabbiz-contacts/src/main.rs` (**2,376 LOC**), `rabbiz-files/src/main.rs` (**2,050 LOC**), `rabbiz-clock/src/main.rs` (**1,866 LOC**), `rabbiz-dialer/src/app.rs` (**1,752 LOC**), `rabbiz-calculator/src/main.rs` (**1,497 LOC**), `rabbiz-music/src/main.rs` (**1,448 LOC**), `rabbiz-pdf/src/main.rs` (**1,344 LOC**).
- **P2 — Inconsistent internal app structure**: Apps mix different conventions (`headless.rs` + `model.rs` vs `cli.rs` + `mcp.rs` vs `app.rs` + `model.rs` vs monolithic `main.rs`). Standardize on `model.rs` (pure host-testable state), `ui.rs` (rendering), `cli.rs` (headless/CLI/MCP), and thin `main.rs` (Wayland wiring).
- **P2 — Missing workspace lints, MSRV & seam check**: [tools/rabbiz-apps/Cargo.toml](../tools/rabbiz-apps/Cargo.toml) omits `[workspace.lints]` and `rust-version`. Untracked `clippy-report.txt` / `clippy-report2.txt` should be cleaned up and ignored if temporary.
- **P3 — Doc drift vs `docs/DESKTOP-SHELL.md`**: [DESKTOP-SHELL.md](DESKTOP-SHELL.md) still describes replacing apps with KDE/Qt packages (`kalk`, `kclock`, `dolphin`, `marknote`), while the active codebase ships 12 native Rust Wayland apps in `tools/rabbiz-apps/`.

---

## 3. Recommended Cleanup Order

1. **`rabbiz-osk` quick hygiene** — **done (2026-11-19)**:
   - Added `[workspace.lints.rust]` (`unsafe_code = "warn"`) and `rust-version = "1.88"` across `osk-core`, `osk-render`, `osk-wayland`.
   - Removed duplicate `tiny-skia` dev-dep in `osk-render/Cargo.toml`.
   - Fixed `unused_mut` warning in `osk-core/tests/keyboard_tests.rs` (45/45 host tests pass with 0 warnings).
   - Remaining: split `osk-wayland/src/main.rs` (1,414 LOC).
2. **`rabbiz-apps` host gate & macOS unlock** — **done (2026-11-19)**:
   - Moved `smithay-client-toolkit` under `[target.'cfg(target_os = "linux")'.dependencies]` in `rabbiz-app-kit/Cargo.toml` and added `wayland_stub.rs` for non-Linux hosts.
   - Unlocked full `cargo test --workspace` across all 13 `rabbiz-apps` crates on macOS and Linux, and enabled it unconditionally in `scripts/check.sh`.
   - Removed stray `clippy-report*.txt` files.
3. **Standardize `rabbiz-apps` app skeleton** — **4/7 large apps split (2026-11-19)**:
   - Split `rabbiz-contacts` (`main.rs` 2,376 → 769 LOC + `views.rs`, `ui.rs`), `rabbiz-files` (`main.rs` 2,050 → 767 LOC + `interaction.rs`, `ui.rs`), `rabbiz-clock` (`main.rs` 1,866 → 437 LOC + `model.rs`, `ui.rs`, `cli.rs`, `mcp.rs`), and `rabbiz-calculator` (`main.rs` 1,497 → 475 LOC + `model.rs`, `ui.rs`).
   - Remaining: `rabbiz-dialer/src/app.rs` (1,752), `rabbiz-music/src/main.rs` (1,448), `rabbiz-pdf/src/main.rs` (1,344).
