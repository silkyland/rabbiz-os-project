# Findings

A finding is one fact (or one disproven guess) about hardware, the kernel, or system behaviour,
with the evidence behind it. Findings replace long shared handoff logs: many contributors
writing the same file causes conflicts and loses provenance.

## Where

`devices/<vendor>/<codename>/findings/YYYY-MM-DD-<slug>.md` for device findings.
`findings/YYYY-MM-DD-<slug>.md` at the repo root for `device: any` findings.
One file per topic. Do not edit someone else's finding to change its conclusion — add a new
finding that links to it (`supersedes:` / `related:`), or update only `status` with new evidence.

## Format

```markdown
---
status: hypothesis        # hypothesis | reproduced | verified | refuted
lane: kernel              # de | kernel | userland | docs
device: samsung/a21s      # or: any
kernel: 7.3.0-g14a425b3a7dc   # or: n/a
author: name-or-handle
ai_assisted: true         # true | false
date: 2026-11-18
evidence:                 # at least one entry unless status is hypothesis
  - cmd: "dmesg | grep -i underrun"
  - file: devices/samsung/a21s/findings/logs/2026-11-18-underrun.txt
supersedes: []
---

# Short claim as a sentence

## Observed
What happened, exactly. Commands, outputs, versions.

## Interpretation
What you think it means. Mark guesses as guesses.

## Reproduce
Steps another person or agent can run.
```

## Status rules

| Status | Requirement |
|---|---|
| `hypothesis` | A reasoned guess. No evidence needed, but must say so. Not a rule. |
| `reproduced` | Observed more than once by the author, with commands and logs. |
| `verified` | Reproduced by a second person/agent, or proven by a fix that is itself on-device verified. |
| `refuted` | Disproven. Keep the file and say what disproved it. |

A device `AGENTS.md` rule may cite only `verified` findings. If a rule's finding is later
refuted, the rule must be removed or changed in the same PR that refutes it.

## Hygiene

- Redact secrets, serial numbers, IMEIs and real MAC/IP addresses before committing.
- Put long logs in `findings/logs/` and link them; keep the finding itself short.
- `./scripts/check.sh` runs `scripts/check_findings.mjs`, which validates the front matter.
