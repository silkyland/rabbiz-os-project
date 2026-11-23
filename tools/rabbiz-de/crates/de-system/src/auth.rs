//! Keyguard authentication types, deterministic lockout backoff state machine,
//! and secret-buffer zeroization (`docs/DE-KEYGUARD-AUTH.md`, Phase 1).
//!
//! Pure, deterministic logic with zero compositor or blocking I/O dependencies
//! so it can be unit-tested on any host and shared between the compositor and
//! the out-of-thread/out-of-process auth helper.

use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use zeroize::Zeroize;

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
    /// Wall-clock epoch seconds when this state was last recorded.
    #[serde(default)]
    pub last_wall_s: Option<u64>,
    /// Remaining cooldown seconds at `last_wall_s` (prevents wall-clock rollback bypass).
    #[serde(default)]
    pub remaining_cooldown_s: Option<u32>,
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
    ///
    /// If the wall clock moved backwards (`now_s < last_wall_s`), the stored
    /// `remaining_cooldown_s` is preserved so setting the clock back cannot
    /// bypass an active lockout.
    #[must_use]
    pub fn cooldown_remaining(&self, now_s: u64) -> Option<u32> {
        let until = self.state.cooldown_until_s?;
        if let Some(last_wall) = self.state.last_wall_s {
            if now_s < last_wall {
                return self
                    .state
                    .remaining_cooldown_s
                    .filter(|&r| r > 0)
                    .or_else(|| {
                        (until > last_wall)
                            .then(|| (until - last_wall).min(u64::from(u32::MAX)) as u32)
                    });
            }
        }
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
            self.state.remaining_cooldown_s = None;
            self.state.last_wall_s = Some(now_s);
            self.state.bfu_unlocked = true;
            return AuthOutcome::Accepted;
        }

        self.state.consecutive_failures = self.state.consecutive_failures.saturating_add(1);
        self.state.last_wall_s = Some(now_s);
        let failures = self.state.consecutive_failures;

        if failures < FIRST_COOLDOWN_FAILURES {
            self.state.cooldown_until_s = None;
            self.state.remaining_cooldown_s = None;
            let remaining = (FIRST_COOLDOWN_FAILURES - failures) as u8;
            AuthOutcome::Rejected {
                attempts_remaining: remaining,
            }
        } else {
            let secs = cooldown_duration_secs(failures);
            self.state.cooldown_until_s = Some(now_s.saturating_add(u64::from(secs)));
            self.state.remaining_cooldown_s = Some(secs);
            AuthOutcome::Cooldown {
                retry_after_secs: secs,
            }
        }
    }
}

/// Resolve the persisted lockout state path (`$XDG_CONFIG_HOME/rabbiz-shell/keyguard.state`).
#[must_use]
pub fn keyguard_state_path() -> Option<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        if !xdg.is_empty() {
            return Some(
                PathBuf::from(xdg)
                    .join("rabbiz-shell")
                    .join("keyguard.state"),
            );
        }
    }
    let home = std::env::var("HOME").ok()?;
    if home.is_empty() {
        return None;
    }
    Some(
        PathBuf::from(home)
            .join(".config")
            .join("rabbiz-shell")
            .join("keyguard.state"),
    )
}

/// Load persisted [`LockoutPolicy`] from `path`.
///
/// - Missing file (`NotFound`) -> fresh `LockoutPolicy::new()`.
/// - Unreadable or corrupt file -> fail closed with maximum cooldown (`300 s`).
/// - Wall clock moved backwards since last save -> preserve remaining cooldown from `now_s`.
#[must_use]
pub fn load_lockout_policy(path: &Path, now_s: u64) -> LockoutPolicy {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return LockoutPolicy::new(),
        Err(e) => {
            eprintln!(
                "rabbiz-de: unreadable keyguard state {}: {e}; applying max cooldown",
                path.display()
            );
            return max_cooldown_policy(now_s);
        }
    };
    let mut state: LockoutState = match serde_json::from_str(&text) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "rabbiz-de: corrupt keyguard state {}: {e}; applying max cooldown",
                path.display()
            );
            return max_cooldown_policy(now_s);
        }
    };
    if let (Some(last_wall), Some(rem)) = (state.last_wall_s, state.remaining_cooldown_s) {
        if now_s < last_wall && rem > 0 {
            state.cooldown_until_s = Some(now_s.saturating_add(u64::from(rem)));
            state.last_wall_s = Some(now_s);
        }
    }
    LockoutPolicy::from_state(state)
}

const fn max_cooldown_policy(now_s: u64) -> LockoutPolicy {
    LockoutPolicy::from_state(LockoutState {
        consecutive_failures: MAX_COOLDOWN_FAILURES,
        cooldown_until_s: Some(now_s.saturating_add(COOLDOWN_TIER3_SECS as u64)),
        bfu_unlocked: false,
        last_wall_s: Some(now_s),
        remaining_cooldown_s: Some(COOLDOWN_TIER3_SECS),
    })
}

/// Atomically persist `state` to `path` with mode `0600` (`temp` -> `fsync` -> `rename`).
pub fn save_lockout_state_atomic(path: &Path, state: &LockoutState) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string(state)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let tmp = path.with_extension(format!("state.tmp.{}", std::process::id()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)?;
    use std::io::Write as _;
    file.write_all(json.as_bytes())?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&tmp, path)
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

/// Overwrite a credential buffer with zeroes using `zeroize` and clear its length.
pub fn zeroize_secret(buf: &mut Vec<u8>) {
    buf.zeroize();
    buf.clear();
}

/// Default Argon2id memory cost (19 MiB, stored in credential so it can be raised later).
pub const DEFAULT_ARGON2_M_COST_KIB: u32 = 19_456;
/// Default Argon2id time cost (iterations).
pub const DEFAULT_ARGON2_T_COST: u32 = 2;
/// Default Argon2id parallelism.
pub const DEFAULT_ARGON2_P_COST: u32 = 1;

fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn hex_decode_exact<const N: usize>(hex: &str) -> Option<[u8; N]> {
    let bytes = hex.as_bytes();
    if bytes.len() != N * 2 {
        return None;
    }
    let mut out = [0_u8; N];
    for (i, slot) in out.iter_mut().enumerate() {
        let hi = hex_nibble(bytes[i * 2])?;
        let lo = hex_nibble(bytes[i * 2 + 1])?;
        *slot = (hi << 4) | lo;
    }
    Some(out)
}

fn verify_legacy_sha256(pin: &str, salt_hex: &str, hash_hex: &str, iterations: u32) -> bool {
    if iterations == 0 {
        return false;
    }
    let Some(salt) = hex_decode_exact::<16>(salt_hex) else {
        return false;
    };
    let Some(expected) = hex_decode_exact::<32>(hash_hex) else {
        return false;
    };
    let mut initial = Vec::with_capacity(salt.len() + pin.len());
    initial.extend_from_slice(&salt);
    initial.extend_from_slice(pin.as_bytes());
    let mut digest: [u8; 32] = Sha256::digest(&initial).into();
    zeroize_secret(&mut initial);

    let mut step = [0_u8; 48];
    step[32..48].copy_from_slice(&salt);
    for _ in 1..iterations {
        step[..32].copy_from_slice(&digest);
        digest = Sha256::digest(step).into();
    }
    step.zeroize();
    let ok = bool::from(digest.ct_eq(&expected));
    digest.zeroize();
    ok
}

/// Persisted salted PIN credential (no plaintext PIN stored).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PinCredential {
    /// Current Argon2id PHC format with explicit parameters for future upgrades.
    Argon2id {
        phc: String,
        m_cost_kib: u32,
        t_cost: u32,
        p_cost: u32,
    },
    /// Legacy iterated SHA-256 format — automatically migrated to `Argon2id` on next successful verify.
    LegacySha256 {
        salt_hex: String,
        hash_hex: String,
        iterations: u32,
    },
}

impl PinCredential {
    /// Create a new Argon2id PIN credential using a random 16-byte salt from `getrandom`.
    pub fn create(pin: &str) -> Result<Self, String> {
        let mut salt = [0_u8; 16];
        getrandom::fill(&mut salt).map_err(|e| format!("getrandom failed: {e}"))?;
        let cred = Self::create_with_params(
            pin,
            salt,
            DEFAULT_ARGON2_M_COST_KIB,
            DEFAULT_ARGON2_T_COST,
            DEFAULT_ARGON2_P_COST,
        );
        salt.zeroize();
        cred
    }

    /// Create an Argon2id PIN credential with explicit salt and cost parameters.
    pub fn create_with_params(
        pin: &str,
        salt: [u8; 16],
        m_cost_kib: u32,
        t_cost: u32,
        p_cost: u32,
    ) -> Result<Self, String> {
        let params = Params::new(m_cost_kib, t_cost, p_cost, None)
            .map_err(|e| format!("argon2 params: {e}"))?;
        let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
        let salt_str = SaltString::encode_b64(&salt).map_err(|e| format!("argon2 salt: {e}"))?;
        let phc = argon2
            .hash_password(pin.as_bytes(), &salt_str)
            .map_err(|e| format!("argon2 hash: {e}"))?
            .to_string();
        Ok(Self::Argon2id {
            phc,
            m_cost_kib,
            t_cost,
            p_cost,
        })
    }

    /// Verify `pin` against this stored credential.
    ///
    /// Returns `(ok, upgraded)` where `upgraded` is `Some(new_cred)` when a legacy
    /// credential succeeded and should be persisted in the new Argon2id format.
    #[must_use]
    pub fn verify_and_upgrade(&self, pin: &str) -> (bool, Option<Self>) {
        debug_assert_ne!(
            std::thread::current().name(),
            Some("main"),
            "PIN verification must not run on the compositor main thread"
        );
        match self {
            Self::Argon2id { phc, .. } => {
                let Ok(parsed) = PasswordHash::new(phc) else {
                    return (false, None);
                };
                let ok = Argon2::default()
                    .verify_password(pin.as_bytes(), &parsed)
                    .is_ok();
                (ok, None)
            }
            Self::LegacySha256 {
                salt_hex,
                hash_hex,
                iterations,
            } => {
                let ok = verify_legacy_sha256(pin, salt_hex, hash_hex, *iterations);
                if !ok {
                    return (false, None);
                }
                let upgraded = Self::create(pin).ok();
                (true, upgraded)
            }
        }
    }

    /// Verify `pin` against this stored credential.
    #[must_use]
    pub fn verify(&self, pin: &str) -> bool {
        self.verify_and_upgrade(pin).0
    }
}

/// Atomically persist a [`PinCredential`] to `path` with mode `0600`.
pub fn save_pin_credential_atomic(path: &Path, cred: &PinCredential) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string(cred)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let tmp = path.with_extension(format!("json.tmp.{}", std::process::id()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)?;
    use std::io::Write as _;
    file.write_all(json.as_bytes())?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&tmp, path)
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
            ..LockoutState::default()
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
            ..LockoutState::default()
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

    #[test]
    fn argon2id_known_answer_and_wrong_pin_rejected() {
        let salt = [0x11_u8; 16];
        let cred = PinCredential::create_with_params("482910", salt, 1024, 1, 1)
            .expect("create_with_params");
        let PinCredential::Argon2id {
            ref phc,
            m_cost_kib,
            t_cost,
            p_cost,
        } = cred
        else {
            panic!("expected Argon2id variant");
        };
        assert!(phc.starts_with("$argon2id$v=19$m=1024,t=1,p=1$"));
        assert_eq!(m_cost_kib, 1024);
        assert_eq!(t_cost, 1);
        assert_eq!(p_cost, 1);

        assert_eq!(cred.verify_and_upgrade("482910"), (true, None));
        assert_eq!(cred.verify_and_upgrade("482911"), (false, None));
        assert_eq!(cred.verify_and_upgrade(""), (false, None));
        assert_eq!(cred.verify_and_upgrade("4829100"), (false, None));

        let encoded = serde_json::to_string(&cred).expect("serialize PinCredential");
        let decoded: PinCredential =
            serde_json::from_str(&encoded).expect("deserialize PinCredential");
        assert_eq!(decoded, cred);
        assert!(decoded.verify("482910"));
    }

    #[test]
    fn pin_credential_create_uses_random_salt() {
        let a = PinCredential::create("1234").expect("create a");
        let b = PinCredential::create("1234").expect("create b");
        assert_ne!(a, b);
        assert!(a.verify("1234"));
        assert!(b.verify("1234"));
        assert!(!a.verify("9999"));
    }

    #[test]
    fn legacy_credential_migrates_to_argon2id_on_successful_verify() {
        // Legacy format for PIN "1234", salt = [0x01; 16], iterations = 1:
        // SHA-256(0x01*16 || "1234")
        let mut expected_input = vec![0x01_u8; 16];
        expected_input.extend_from_slice(b"1234");
        let digest = Sha256::digest(&expected_input);
        let hash_hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        let legacy_json = format!(
            r#"{{"salt_hex":"01010101010101010101010101010101","hash_hex":"{hash_hex}","iterations":1}}"#
        );
        let legacy: PinCredential = serde_json::from_str(&legacy_json).expect("parse legacy");

        // Wrong PIN does not verify or migrate.
        assert_eq!(legacy.verify_and_upgrade("0000"), (false, None));

        // Correct PIN verifies and returns upgraded Argon2id credential.
        let (ok, upgraded) = legacy.verify_and_upgrade("1234");
        assert!(ok);
        let new_cred = upgraded.expect("expected migration to Argon2id");
        assert!(matches!(new_cred, PinCredential::Argon2id { .. }));
        assert!(new_cred.verify("1234"));
        assert!(!new_cred.verify("0000"));
    }

    #[test]
    fn lockout_persists_across_restart_after_five_failures() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("keyguard.state");
        let mut p = LockoutPolicy::new();
        for i in 0..5 {
            p.record_attempt(false, 1_000 + i);
        }
        assert_eq!(p.cooldown_remaining(1_004), Some(30));
        save_lockout_state_atomic(&path, p.state()).expect("save");

        let mode = std::os::unix::fs::PermissionsExt::mode(
            &std::fs::metadata(&path).expect("meta").permissions(),
        ) & 0o777;
        assert_eq!(mode, 0o600);

        // Simulate compositor restart at t = 1010.
        let restored = load_lockout_policy(&path, 1_010);
        assert_eq!(restored.cooldown_remaining(1_010), Some(24));
        assert_eq!(restored.state().consecutive_failures, 5);
    }

    #[test]
    fn corrupt_state_file_triggers_max_cooldown() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("keyguard.state");
        std::fs::write(&path, "{not valid json!!!").expect("write corrupt");

        let restored = load_lockout_policy(&path, 2_000);
        assert_eq!(restored.cooldown_remaining(2_000), Some(300));
        assert_eq!(restored.state().consecutive_failures, 15);
    }

    #[test]
    fn clock_moved_back_one_hour_preserves_cooldown() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("keyguard.state");
        let mut p = LockoutPolicy::new();
        for i in 0..5 {
            p.record_attempt(false, 10_000 + i);
        }
        assert_eq!(p.cooldown_remaining(10_004), Some(30));
        // Move wall clock back 1 hour (3600 s) in-memory.
        assert_eq!(p.cooldown_remaining(10_004 - 3_600), Some(30));

        // Persist and reload after clock moved back 1 hour.
        save_lockout_state_atomic(&path, p.state()).expect("save");
        let restored = load_lockout_policy(&path, 10_004 - 3_600);
        assert_eq!(restored.cooldown_remaining(10_004 - 3_600), Some(30));
    }
}
