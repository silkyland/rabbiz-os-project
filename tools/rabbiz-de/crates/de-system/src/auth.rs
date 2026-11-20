//! Keyguard authentication types, deterministic lockout backoff state machine,
//! and secret-buffer zeroization (`docs/DE-KEYGUARD-AUTH.md`, Phase 1).
//!
//! Pure, deterministic logic with zero compositor or blocking I/O dependencies
//! so it can be unit-tested on any host and shared between the compositor and
//! the out-of-thread/out-of-process auth helper.

use std::sync::atomic::{compiler_fence, Ordering};

use serde::{Deserialize, Serialize};

/// Configured Keyguard authentication method (`de-config::Prefs::lock_method`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LockMethod {
    /// Dev default: swipe-up unlocks immediately without credential check.
    #[default]
    Swipe,
    /// Numeric PIN (4–12 digits).
    Pin,
    /// Full passphrase verified via OSK + PAM.
    Passphrase,
    /// Rear fingerprint (`fprintd`) with PIN/Passphrase fallback and BFU gate.
    BiometricWithFallback,
}

/// Result of recording a credential verification attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthOutcome {
    /// Credential verified; session may unlock.
    Accepted,
    /// Credential rejected; caller may retry immediately.
    Rejected { attempts_remaining: u8 },
    /// Account is in backoff cooldown; keypad must remain disabled for
    /// `retry_after_secs` seconds.
    Cooldown { retry_after_secs: u32 },
}

/// Persisted lockout counters (`keyguard.state`, mode `0600`).
///
/// `consecutive_failures` and `cooldown_until_s` survive reboots; `bfu_unlocked`
/// tracks whether at least one strong (PIN/Passphrase) unlock has succeeded
/// since boot (Before First Unlock vs After First Unlock).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockoutState {
    pub consecutive_failures: u32,
    pub cooldown_until_s: Option<u64>,
    pub bfu_unlocked: bool,
}

/// Pure, deterministic Keyguard lockout policy (`docs/DE-KEYGUARD-AUTH.md` §4).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LockoutPolicy {
    state: LockoutState,
}

/// Failures before the first 30 s cooldown triggers.
const FIRST_COOLDOWN_FAILURES: u32 = 5;
/// Failures at or above which biometric unlock is disabled until strong auth succeeds.
const BIOMETRIC_DISABLE_FAILURES: u32 = 10;
/// Failures at or above which the maximum 300 s cooldown applies.
const MAX_COOLDOWN_FAILURES: u32 = 15;

const COOLDOWN_TIER1_SECS: u32 = 30;
const COOLDOWN_TIER2_SECS: u32 = 120;
const COOLDOWN_TIER3_SECS: u32 = 300;

impl LockoutPolicy {
    /// Create a fresh policy (BFU state, zero failures, no active cooldown).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Restore a policy from persisted [`LockoutState`].
    #[must_use]
    pub const fn from_state(state: LockoutState) -> Self {
        Self { state }
    }

    /// Inspect the underlying [`LockoutState`] (for persistence or UI display).
    #[must_use]
    pub const fn state(&self) -> &LockoutState {
        &self.state
    }

    /// Whether biometric unlock (`fprintd`) is currently permitted.
    ///
    /// Requires:
    /// - At least one strong unlock since boot (`bfu_unlocked == true`),
    /// - No active cooldown at `now_s`, and
    /// - Fewer than 10 consecutive failures (`consecutive_failures < 10`).
    #[must_use]
    pub fn can_use_biometric(&self, now_s: u64) -> bool {
        self.state.bfu_unlocked
            && self.cooldown_remaining(now_s).is_none()
            && self.state.consecutive_failures < BIOMETRIC_DISABLE_FAILURES
    }

    /// Remaining cooldown seconds at `now_s`, or `None` if no cooldown is active.
    #[must_use]
    pub fn cooldown_remaining(&self, now_s: u64) -> Option<u32> {
        let until = self.state.cooldown_until_s?;
        if until > now_s {
            let rem = (until - now_s).min(u64::from(u32::MAX)) as u32;
            Some(rem)
        } else {
            None
        }
    }

    /// Record a credential verification attempt at monotonic/epoch timestamp `now_s`.
    ///
    /// Backoff schedule:
    /// - Attempting while a cooldown is active returns `Cooldown { retry_after_secs }`
    ///   without resetting the timer or altering the failure counter.
    /// - `success == true` resets `consecutive_failures = 0`, clears `cooldown_until_s`,
    ///   and sets `bfu_unlocked = true`.
    /// - `success == false` increments `consecutive_failures`:
    ///   - `1..=4`: `0 s` cooldown (`attempts_remaining = 5 - consecutive_failures`)
    ///   - `5..=9`: `30 s` cooldown
    ///   - `10..=14`: `120 s` cooldown (disables biometric)
    ///   - `15+`: `300 s` cooldown
    pub fn record_attempt(&mut self, success: bool, now_s: u64) -> AuthOutcome {
        if let Some(retry_after_secs) = self.cooldown_remaining(now_s) {
            return AuthOutcome::Cooldown { retry_after_secs };
        }

        if success {
            self.state.consecutive_failures = 0;
            self.state.cooldown_until_s = None;
            self.state.bfu_unlocked = true;
            return AuthOutcome::Accepted;
        }

        self.state.consecutive_failures = self.state.consecutive_failures.saturating_add(1);
        let failures = self.state.consecutive_failures;

        if failures < FIRST_COOLDOWN_FAILURES {
            self.state.cooldown_until_s = None;
            let remaining = (FIRST_COOLDOWN_FAILURES - failures) as u8;
            AuthOutcome::Rejected {
                attempts_remaining: remaining,
            }
        } else {
            let secs = cooldown_duration_secs(failures);
            self.state.cooldown_until_s = Some(now_s.saturating_add(u64::from(secs)));
            AuthOutcome::Cooldown {
                retry_after_secs: secs,
            }
        }
    }
}

const fn cooldown_duration_secs(failures: u32) -> u32 {
    if failures >= MAX_COOLDOWN_FAILURES {
        COOLDOWN_TIER3_SECS
    } else if failures >= BIOMETRIC_DISABLE_FAILURES {
        COOLDOWN_TIER2_SECS
    } else {
        COOLDOWN_TIER1_SECS
    }
}

/// Overwrite a credential buffer with zeroes using an optimization barrier and
/// compiler fence, then clear its length.
///
/// Uses 100% safe Rust (`fill(0)` + `black_box` + `compiler_fence`) so it
/// complies with `unsafe_code = "warn"` across the workspace.
pub fn zeroize_secret(buf: &mut Vec<u8>) {
    buf.fill(0);
    std::hint::black_box(&*buf);
    compiler_fence(Ordering::SeqCst);
    buf.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_thresholds_match_rfc_schedule() {
        let mut policy = LockoutPolicy::new();
        let mut t = 1_000_u64;

        // Failures 1..=4: 0s cooldown, attempts_remaining counts down 4, 3, 2, 1.
        for expected_rem in [4_u8, 3, 2, 1] {
            assert_eq!(
                policy.record_attempt(false, t),
                AuthOutcome::Rejected {
                    attempts_remaining: expected_rem
                }
            );
            assert_eq!(policy.cooldown_remaining(t), None);
            t += 1;
        }

        // Failures 5..=9: 30s cooldown each time once previous cooldown expires.
        for _ in 5..=9 {
            assert_eq!(
                policy.record_attempt(false, t),
                AuthOutcome::Cooldown {
                    retry_after_secs: 30
                }
            );
            assert_eq!(policy.cooldown_remaining(t), Some(30));
            t += 30;
            assert_eq!(policy.cooldown_remaining(t), None);
        }

        // Failures 10..=14: 120s cooldown.
        for _ in 10..=14 {
            assert_eq!(
                policy.record_attempt(false, t),
                AuthOutcome::Cooldown {
                    retry_after_secs: 120
                }
            );
            assert_eq!(policy.cooldown_remaining(t), Some(120));
            t += 120;
            assert_eq!(policy.cooldown_remaining(t), None);
        }

        // Failures 15+: 300s cooldown.
        for _ in 15..=17 {
            assert_eq!(
                policy.record_attempt(false, t),
                AuthOutcome::Cooldown {
                    retry_after_secs: 300
                }
            );
            assert_eq!(policy.cooldown_remaining(t), Some(300));
            t += 300;
        }
    }

    #[test]
    fn attempts_during_active_cooldown_do_not_reset_timer_or_unlock() {
        let mut policy = LockoutPolicy::new();
        for _ in 0..4 {
            policy.record_attempt(false, 100);
        }
        assert_eq!(
            policy.record_attempt(false, 100),
            AuthOutcome::Cooldown {
                retry_after_secs: 30
            }
        );

        // 10s later: 20s remain; neither wrong nor right attempt bypasses or resets it.
        assert_eq!(
            policy.record_attempt(false, 110),
            AuthOutcome::Cooldown {
                retry_after_secs: 20
            }
        );
        assert_eq!(policy.state().consecutive_failures, 5);
        assert_eq!(policy.state().cooldown_until_s, Some(130));

        assert_eq!(
            policy.record_attempt(true, 125),
            AuthOutcome::Cooldown {
                retry_after_secs: 5
            }
        );
        assert!(!policy.state().bfu_unlocked);

        // At exact expiry (130), cooldown is over and success unlocks.
        assert_eq!(policy.cooldown_remaining(130), None);
        assert_eq!(policy.record_attempt(true, 130), AuthOutcome::Accepted);
    }

    #[test]
    fn bfu_and_biometric_policy_rules() {
        let mut policy = LockoutPolicy::new();
        // Before First Unlock: biometric is disabled even with 0 failures.
        assert!(!policy.can_use_biometric(100));

        // First strong unlock enables biometric (AFU).
        assert_eq!(policy.record_attempt(true, 100), AuthOutcome::Accepted);
        assert!(policy.state().bfu_unlocked);
        assert!(policy.can_use_biometric(100));

        // 4 failures: still under 10 and no cooldown -> biometric still allowed.
        for _ in 0..4 {
            policy.record_attempt(false, 200);
        }
        assert!(policy.can_use_biometric(200));

        // 5th failure triggers 30s cooldown -> biometric disabled during cooldown.
        policy.record_attempt(false, 200);
        assert!(!policy.can_use_biometric(210));
        // Once 30s cooldown expires (still < 10 failures) -> biometric allowed again.
        assert!(policy.can_use_biometric(230));

        // Push to 10 failures -> biometric stays disabled even AFTER cooldown expires.
        let mut t = 230_u64;
        for _ in 6..=10 {
            policy.record_attempt(false, t);
            t += 300;
        }
        assert_eq!(policy.state().consecutive_failures, 10);
        assert_eq!(policy.cooldown_remaining(t), None);
        assert!(!policy.can_use_biometric(t));

        // Strong unlock resets failure counter and re-enables biometric.
        assert_eq!(policy.record_attempt(true, t), AuthOutcome::Accepted);
        assert!(policy.can_use_biometric(t));
    }

    #[test]
    fn success_resets_failures_and_clears_cooldown() {
        let mut policy = LockoutPolicy::from_state(LockoutState {
            consecutive_failures: 12,
            cooldown_until_s: Some(500),
            bfu_unlocked: false,
        });

        assert_eq!(policy.record_attempt(true, 500), AuthOutcome::Accepted);
        assert_eq!(policy.state().consecutive_failures, 0);
        assert_eq!(policy.state().cooldown_until_s, None);
        assert!(policy.state().bfu_unlocked);
    }

    #[test]
    fn lockout_state_roundtrips_json() {
        let state = LockoutState {
            consecutive_failures: 7,
            cooldown_until_s: Some(1_700_000_030),
            bfu_unlocked: true,
        };
        let encoded = serde_json::to_string(&state).expect("serialize LockoutState");
        let decoded: LockoutState =
            serde_json::from_str(&encoded).expect("deserialize LockoutState");
        assert_eq!(decoded, state);
    }

    #[test]
    fn zeroize_secret_wipes_and_clears_buffer() {
        let mut secret = b"123456".to_vec();
        let orig_cap = secret.capacity();
        assert_eq!(secret.len(), 6);
        zeroize_secret(&mut secret);
        assert!(secret.is_empty());
        assert_eq!(secret.capacity(), orig_cap);
    }
}
