# Prompt for the Next Agent(s) — `rabbiz-de` Structure & Base Architecture

Copy everything inside the code block below into a fresh agent session opened in this repository.
To split work across parallel agents, replace `YOUR TASK:` with one backlog item from
[HANDOFF-de-audit-2026-11-19.md](HANDOFF-de-audit-2026-11-19.md) §4. Assign disjoint files to each agent.

```text
You are working in the `de` lane on `tools/rabbiz-de` (the Rust Wayland compositor and mobile/desktop
shell for RabbizOS). A full structural audit and modern-shell gap analysis was completed and recorded
in `docs/HANDOFF-de-audit-2026-11-19.md`.

READ FIRST, in this order, before editing anything:
1. docs/HANDOFF-de-audit-2026-11-19.md  (audit findings, modern-shell gaps, prioritized backlog)
2. docs/DE-SEAMS.md                     (frozen `de-api` contract and crate dependency direction)
3. tools/rabbiz-de/scripts/check_seams.py (enforced seam rules)
4. CONTRIBUTING.md & AGENTS.md          (lane rules, evidence levels, naming rules, hardware safety)

Ground rules:
- One lane (`de`), one concern per PR/commit. Keep refactors strictly behavior-preserving unless the
  task explicitly asks for a feature change.
- Changing any frozen `de-api` type/field requires owner sign-off BEFORE landing code (docs/DE-SEAMS.md).
- Keep crate dependency direction strictly one-way; never bypass `tools/rabbiz-de/scripts/check_seams.py`.
- Never run `cargo fmt` across the whole workspace (it reformats unrelated files); run `rustfmt` only
  on files you created or edited.
- Follow the repo naming rule: no anti-homes (`misc`, `utils`, `helpers`, `common`, `_v2`) — name the
  actual domain concept.
- Run host gates before finishing:
  - `python3 tools/rabbiz-de/scripts/check_seams.py`
  - `./scripts/de_test_host.sh` (on macOS/Linux)
  - `node scripts/check_links.mjs` (if touching docs)
  - For Linux-only crates (`de-core`, `de-backend`, `de-render`, `de-protocols`, `de-bin`), verify
    the build in the arm64 container if available, or state honestly "Not built/tested for Linux/device".
- State your exact evidence level (L0 host-tested / L1 built / L2 on-device). Never claim on-device
  success without a device log.
- Do not touch another task's uncommitted working-tree edits (check `git status` first; only stage/edit
  files belonging to your task).
- Reply to the owner in Thai; write code, commit messages, and technical docs in English.
- Update `docs/HANDOFF-de-audit-2026-11-19.md` §4 at the end of your work so the next agent sees what
  is completed and what remains.

YOUR TASK: <paste one item from docs/HANDOFF-de-audit-2026-11-19.md §4, e.g.
"Tier 0.1 — Manifest & Seam Hygiene" or "Tier 1.3 — Sub-module de-core">
```

## Suggested Split for Parallel Agents (Disjoint Files)

- **Agent A (Tier 0.1 — Manifest & Seam Hygiene)**: `tools/rabbiz-de/crates/de-bin/Cargo.toml`, `tools/rabbiz-de/tests/smoke/Cargo.toml`, `scripts/de_test_host.sh`, `tools/rabbiz-de/scripts/check_seams.py`, `docs/DE-SEAMS.md`.
- **Agent B (Tier 0.2 — Prefs Ownership Cleanup)**: `tools/rabbiz-de/crates/de-config/src/prefs.rs`, `tools/rabbiz-de/crates/de-apps/src/prefs.rs` (plus call-site updates in `de-core` once coordinated).
- **Agent C (Tier 1.3 — Sub-module `de-core`)**: `tools/rabbiz-de/crates/de-core/src/*` only (pure move/split, zero behavior change).
- **Agent D (Tier 1.4 — Sub-module `de-ui`)**: `tools/rabbiz-de/crates/de-ui/src/*` only (pure move/re-export, 0 pixel diff on goldens).
- **Agent E (Tier 1.5 — Architecture RFC)**: Design doc only (`docs/DE-SCENE-INSETS.md`) proposing `DisplayArea` Z-order + `WindowInsets` + `de-api` changes for owner sign-off before coding.
