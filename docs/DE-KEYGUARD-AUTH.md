# RFC: Real Keyguard Authentication, Lockout Policy & Session Lock (`rabbiz-de`)

- **Status:** Draft RFC (Concept — ready for owner review before implementation)
- **Lane:** `de`
- **Target crates:** `de-system`, `de-ui`, `de-core`, `de-config`
- **Related:** [DE-SCENE-INSETS.md](DE-SCENE-INSETS.md), [DE-NOTIFICATIONS-SERVICES.md](DE-NOTIFICATIONS-SERVICES.md), [DE-SEAMS.md](DE-SEAMS.md)

---

## 1. Problem Statement

`rabbiz-de` currently implements:
- Visual lock screen ([`de-ui/src/screens/lock.rs`](../tools/rabbiz-de/crates/de-ui/src/screens/lock.rs)) with swipe-up-to-unlock, clock/date, notifications preview, fingerprint prompt pill, and quick actions.
- Compositor-level **Keyguard isolation** in [`DisplayStack`](../tools/rabbiz-de/crates/de-api/src/display_stack.rs) and [`render_with`](../tools/rabbiz-de/crates/de-core/src/runtime/frame/render.rs) so app/home surfaces are never rendered while strictly locked.
- Privileged `ext-session-lock-v1` protocol handler ([`protocols/session_lock.rs`](../tools/rabbiz-de/crates/de-core/src/protocols/session_lock.rs)).

However, **there is no real credential gate yet**: swiping up immediately unlocks the device. Before RabbizOS can be used as a daily-driver phone, Keyguard needs real authentication that protects the user's session without introducing brittle PAM deadlocks inside the compositor thread.

---

## 2. Security & Architectural Principles

1. **Never block the compositor thread on PAM/crypto:** Credential verification MUST run in an isolated helper thread or short-lived helper process (`rabbiz-auth-helper`), communicating only `VerifyResult::{Accepted, Rejected, LockedOut { retry_after_s }}`.
2. **Never log or persist plaintext credentials:** PIN/passphrase buffers are zeroized immediately after verification (`zeroize` / manual overwrite + `compiler_fence`), never written to logs, IPC, or crash dumps.
3. **Fail-closed on error:** If the auth backend crashes or times out while locked, Keyguard stays locked and offers retry — it never falls back to unlocked.
4. **Input & render isolation:** While `Keyguard` is active (either built-in or external `ext-session-lock-v1`):
   - `DisplayStack` filters out all layers below `DisplayArea::Keyguard` (already enforced).
   - `runtime/input/` routes **all** touch, pointer, and keyboard events exclusively to Keyguard (or the active `ext-session-lock-v1` surface), never to background apps, Home, Dock, or Shade.
   - `rabbiz-de ctl` IPC refuses `Launch`, `SetPanel(open)`, and `Screenshot` while locked (only `GetState` and `Lock` are allowed).

---

## 3. Authentication Modes & Credential Storage

Support four progressive modes configured in `de-config::Prefs::lock_method`:

| Mode | UX on Swipe-Up | Verification Backend |
|---|---|---|
| `Swipe` (dev default) | Immediate unlock (current behavior) | None |
| `Pin` (4–12 digits) | Full-screen numeric keypad + dots | Argon2id hash in `~/.config/rabbiz-shell/keyguard.hash` (`0600`) **or** Linux PAM (`rabbiz-lock` service) |
| `Passphrase` | `rabbiz-osk` (allowed in `DisplayArea::Critical` over Keyguard) + masked text field | Linux PAM (`pam_unix` for user `rabbiz`) |
| `Biometric + Fallback` | Rear fingerprint (`fprintd` via `de-system/src/fingerprint.rs`) | Unlocks directly after first post-boot PIN/Passphrase; falls back to PIN/Passphrase after 5 failed touches or reboot |

**First-unlock-after-boot rule (BFU vs AFU):**
- Biometric (`fprintd`) is disabled until the user completes one successful PIN or Passphrase unlock after boot (matching Android's Direct Boot / Strong Auth requirement).

---

## 4. Lockout Backoff State Machine (`de-system/src/auth.rs`)

A pure, deterministic state machine prevents brute-force guessing on a lost phone:

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthOutcome {
    Accepted,
    Rejected { attempts_remaining: u8 },
    Cooldown { retry_after_secs: u32 },
}
```

| Consecutive Failures | Cooldown Duration | Notes |
|---|---|---|
| 1–4 | `0 s` | Show shake animation + subtle error text |
| 5 | `30 s` | Disable keypad & show countdown timer |
| 6–9 | `30 s` | Same tier |
| 10 | `120 s` | Disable biometric until PIN/Passphrase succeeds |
| 15+ | `300 s` (5 min) | Persisted monotonic failure counter (`keyguard.state`, `0600`) so rebooting does not reset lockout |

---

## 5. `ext-session-lock-v1` + Built-In Keyguard Coordination

`de-core/src/protocols/session_lock.rs` already tracks `self.session_locked` and delegates `lock()`/`unlock()`/`new_surface()`. Complete the contract:

1. **Rendering:** When `self.session_locked == true` and an external `LockSurface` is mapped on the output, render the external `LockSurface` in `DisplayArea::Keyguard` (instead of the built-in `de-ui/src/screens/lock.rs` surface).
2. **Dead-locker fail-closed:** If an external `ext-session-lock-v1` client dies without calling `unlock()`, the Wayland protocol **forbids** unlocking the session. `rabbiz-de` MUST keep `DisplayStack` in locked mode and render a red/amber recovery Keyguard screen offering built-in PAM/PIN unlock so the user is never stranded on a black screen.
3. **OSK on Lockscreen:** When `Passphrase` mode is active, `rabbiz-osk` is permitted to render in `DisplayArea::Critical` above Keyguard, with predictive/learned-word caching disabled (`text-input` `content_purpose::Password`).

---

## 6. UI Layout (`de-ui/src/screens/lock_bouncer.rs`)

Keep `lock.rs` focused on the ambient/preview lock screen, and add a dedicated `lock_bouncer.rs` for the credential entry overlay (matching Android's `KeyguardBouncer`):

- **Transition:** Swiping up on `lock.rs` when `lock_method != Swipe` slides in `lock_bouncer.rs` (instead of unlocking). Swiping down or tapping Back returns to the ambient lock screen.
- **Emergency Call button:** Always visible at the bottom of the PIN/Passphrase bouncer when a modem is present (`rabbiz-dialer --emergency` launched in `DisplayArea::Keyguard` without unlocking the session).
- **Quick actions on ambient lock screen:**
  - `Flashlight`: toggles torch directly without unlocking.
  - `Camera` / `Quick Note`: launches in restricted lockscreen mode (or prompts bouncer if app requires unlocked session).

---

## 7. Verification Plan

1. **L0 (macOS host):**
   - Unit tests for `LockoutPolicy` (backoff thresholds, countdown expiry, reset on success, reboot persistence).
   - Unit tests for `lock_bouncer.rs` keypad geometry/hit-testing (`0–9`, Backspace, Submit, Emergency) and `de-sim` visual rendering.
   - Unit tests for IPC lock guard (`Launch` / `Screenshot` rejected when locked).
2. **L1 (`rabbiz-de-build:arm64`):**
   - Build PAM / Argon2id verification worker and `ext-session-lock-v1` dead-client fail-closed handler.
3. **L2 (A21s on-device):**
   - Set a test PIN, lock via power button / `rabbiz-de ctl lock`, verify swipe-up shows bouncer, wrong PIN increments counter & triggers 30 s cooldown on 5th try, right PIN unlocks cleanly.
   - Kill an external `ext-session-lock-v1` test client mid-lock and confirm session stays locked with recovery bouncer.
