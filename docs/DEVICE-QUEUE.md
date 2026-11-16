# Shared A21s device queue

The owner authorized coordinating hardware tests across concurrent work on
2026-11-16. Code changes and host tests continue in parallel. Serialize only
device deployment, shell reloads, touch injection, performance measurements,
session changes, power/lock changes and reboots.

Use the same **remote** lock for every task (a host-local lock misses other hosts):

```sh
./scripts/device_session.sh root@172.16.42.1 <<'REMOTE'
# All device mutations and observations belonging to this test go here.
uptime
REMOTE
```

This runs stdin under `flock --close --timeout 300
/run/lock/a21s-device-test.lock bash -se`. The lock releases on exit/disconnect;
`--close` prevents background apps from inheriting and retaining the lock.
Do not leave an SSH holder running while developing on the host. For reboot,
keep ownership until issuing it, then acquire a new session after the device is
reachable. A reboot terminates running applications: announce it beforehand.

| Work | Device status |
|---|---|
| Backdrop blur / shell-only GPU (`01a1e336-a599-7533-adbf-634d83718224`) | Verification complete; device slot released. Future tests use bounded lock sessions |
| Base-app icons (`01a1e32e-8ff9-7d90-824f-8429481eac34`) | Coordination message sent; host work may continue |
| Mirror icon (`01a1e296-4145-7e51-9a25-f1384c0c0a9b`) | Coordination message sent; host work may continue |

Only these Codex app chats were discoverable. CLI/external agents must also
follow this lock for it to provide mutual exclusion. Before deploying, send
only your task's files: copying all of `shell/` can deploy another task's
unfinished changes. Prefer isolated worktrees for code work; never reset,
stage, or overwrite another task's changes.

If uptime, shell PID or unrelated files change during a measurement, invalidate
that result and repeat in an uncontended session. Do not attribute every crash
to the feature under test when another task is changing the device.
