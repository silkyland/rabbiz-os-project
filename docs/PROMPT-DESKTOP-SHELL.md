# Prompt for the next agent(s)

Copy everything in the block below into a fresh agent session opened in this repository
(`/Users/bunditnuntates/Sites/Labs/android-arm-linux`, branch `feat/desktop-shell-rewrite`).
To split work between several agents, replace the last line (`YOUR TASK:`) with one backlog item from
`docs/HANDOFF-DESKTOP-SHELL.md` §3. Give two agents different files, never the same ones.

```text
You are continuing the "desktop shell rewrite" for a Samsung Galaxy A21s running bare-metal mainline
Linux (labwc + Quickshell). The previous agent ran out of budget and left a written handoff.

READ FIRST, in this order, before touching anything:
1. docs/HANDOFF-DESKTOP-SHELL.md   (state, device loop, traps, prioritized backlog with exit criteria)
2. docs/DESKTOP-SHELL.md           (plan, owner decisions D1-D5, spike results, progress log)
3. docs/ARCHITECTURE.md            ("Shell seams" table, ADR-6/7/8)
4. AGENTS.md                       (hard device rules + "Where shell code goes")

Ground rules:
- The device is a lab device: flashing, rebooting and package changes are allowed, but AGENTS.md hard rules still
  apply (never overwrite /lib, keep usb0 unmanaged, never print ~/.credentail, ...). Never type passwords for the
  user: if SSH needs the key loaded or a password, stop and ask them to run `ssh-add` / `ssh-copy-id` themselves.
- Verify on the device, not by reading code: use ./scripts/shell_reload.sh, the `launcher` IPC calls and `grim`
  screenshots described in the handoff. Say plainly what you verified and what you did not.
- Run ./scripts/check.sh before every commit; keep dependency direction Theme <- Services <- Widgets <- Windowing
  <- Desktop/Panels (enforced by scripts/check_shell_seams.mjs). New pure logic goes in a plain .js file with a
  node test, like shell/A21s/Windowing/LaunchFlow.js.
- Rules are hypotheses: if evidence shows a rule or decision is wrong, show the evidence, propose the change and
  record it (ADR in docs/ARCHITECTURE.md). Security/data-integrity rules change only by explicit decision.
- The owner is not a Linux/BL expert: explain decisions as what the user of the phone experiences, separate facts
  from guesses, object plainly when a request rests on a wrong assumption, collect questions only the owner can
  answer. Reply to the owner in Thai, write code/docs in English.
- Commit small and often on branch feat/desktop-shell-rewrite with a message that says what was verified on the
  device. Do not push to main. Push the branch when a milestone is verified.
- Update docs/HANDOFF-DESKTOP-SHELL.md (state table + backlog) and docs/DESKTOP-SHELL.md §10 at the end of your
  work so the next agent starts from the truth.

YOUR TASK: <paste one backlog item from docs/HANDOFF-DESKTOP-SHELL.md section 3, e.g.
"1. Verify Steam end to end (ADR-8)" or "3. M7.8 Settings v2">
```

Suggested split for parallel agents (disjoint files):
- Agent A: backlog 1 (Steam) — `rootfs-overlay/usr/local/bin/a21s-steam`, `steam`, ADR-8 only.
- Agent B: backlog 2 (review findings) — `shell/A21s/Services/{SystemControl,Network,Bluetooth}.qml`.
- Agent C: backlog 3 (Settings v2) — `shell/apps/settings/`, `shell/A21s/Services/Appearance.qml`.
- Agent D: backlog 4 (gestures + close morph) — `shell/A21s/Windowing/`, `shell/A21s/Panels/NavBar.qml`.
