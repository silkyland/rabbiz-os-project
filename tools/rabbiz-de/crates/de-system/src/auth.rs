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

/// Default iteration count for salted PIN key derivation.
pub const DEFAULT_PIN_ITERATIONS: u32 = 50_000;

/// FIPS 180-4 SHA-256 initial hash values.
const SHA256_INIT: [u32; 8] = [
    0x6a09_e667,
    0xbb67_ae85,
    0x3c6e_f372,
    0xa54f_f53a,
    0x510e_527f,
    0x9b05_688c,
    0x1f83_d9ab,
    0x5be0_cd19,
];

/// FIPS 180-4 SHA-256 round constants.
const SHA256_K: [u32; 64] = [
    0x428a_2f98,
    0x7137_4491,
    0xb5c0_fbcf,
    0xe9b5_dba5,
    0x3956_c25b,
    0x59f1_11f1,
    0x923f_82a4,
    0xab1c_5ed5,
    0xd807_aa98,
    0x1283_5b01,
    0x2431_85be,
    0x550c_7dc3,
    0x72be_5d74,
    0x80de_b1fe,
    0x9bdc_06a7,
    0xc19b_f174,
    0xe49b_69c1,
    0xefbe_4786,
    0x0fc1_9dc6,
    0x240c_a1cc,
    0x2de9_2c6f,
    0x4a74_84aa,
    0x5cb0_a9dc,
    0x76f9_88da,
    0x983e_5152,
    0xa831_c66d,
    0xb003_27c8,
    0xbf59_7fc7,
    0xc6e0_0bf3,
    0xd5a7_9147,
    0x06ca_6351,
    0x1429_2967,
    0x27b7_0a85,
    0x2e1b_2138,
    0x4d2c_6dfc,
    0x5338_0d13,
    0x650a_7354,
    0x766a_0abb,
    0x81c2_c92e,
    0x9272_2c85,
    0xa2bf_e8a1,
    0xa81a_664b,
    0xc24b_8b70,
    0xc76c_51a3,
    0xd192_e819,
    0xd699_0624,
    0xf40e_3585,
    0x106a_a070,
    0x19a4_c116,
    0x1e37_6c08,
    0x2748_774c,
    0x34b0_bcb5,
    0x391c_0cb3,
    0x4ed8_aa4a,
    0x5b9c_ca4f,
    0x682e_6ff3,
    0x748f_82ee,
    0x78a5_636f,
    0x84c8_7814,
    0x8cc7_0208,
    0x90be_fffa,
    0xa450_6ceb,
    0xbef9_a3f7,
    0xc671_78f2,
];

fn sha256_compress(state: &mut [u32; 8], block: &[u8; 64]) {
    let mut w = [0_u32; 64];
    for (i, chunk) in block.chunks_exact(4).enumerate() {
        w[i] = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
    }
    for i in 16..64 {
        let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
        let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
        w[i] = w[i - 16]
            .wrapping_add(s0)
            .wrapping_add(w[i - 7])
            .wrapping_add(s1);
    }

    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
    for i in 0..64 {
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let ch = (e & f) ^ ((!e) & g);
        let temp1 = h
            .wrapping_add(s1)
            .wrapping_add(ch)
            .wrapping_add(SHA256_K[i])
            .wrapping_add(w[i]);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let maj = (a & b) ^ (a & c) ^ (b & c);
        let temp2 = s0.wrapping_add(maj);

        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(temp1);
        d = c;
        c = b;
        b = a;
        a = temp1.wrapping_add(temp2);
    }

    state[0] = state[0].wrapping_add(a);
    state[1] = state[1].wrapping_add(b);
    state[2] = state[2].wrapping_add(c);
    state[3] = state[3].wrapping_add(d);
    state[4] = state[4].wrapping_add(e);
    state[5] = state[5].wrapping_add(f);
    state[6] = state[6].wrapping_add(g);
    state[7] = state[7].wrapping_add(h);
}

/// Compute the standard FIPS 180-4 SHA-256 digest of `data`.
#[must_use]
pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut state = SHA256_INIT;
    let mut chunks = data.chunks_exact(64);
    for chunk in &mut chunks {
        let mut block = [0_u8; 64];
        block.copy_from_slice(chunk);
        sha256_compress(&mut state, &block);
    }

    let rem = chunks.remainder();
    let mut pad = [0_u8; 128];
    pad[..rem.len()].copy_from_slice(rem);
    pad[rem.len()] = 0x80;

    let total_blocks = if rem.len() + 1 + 8 <= 64 { 1 } else { 2 };
    let bit_len = (data.len() as u64).wrapping_mul(8);
    let len_offset = total_blocks * 64 - 8;
    pad[len_offset..len_offset + 8].copy_from_slice(&bit_len.to_be_bytes());

    let mut first = [0_u8; 64];
    first.copy_from_slice(&pad[..64]);
    sha256_compress(&mut state, &first);
    if total_blocks == 2 {
        let mut second = [0_u8; 64];
        second.copy_from_slice(&pad[64..128]);
        sha256_compress(&mut state, &second);
    }

    let mut out = [0_u8; 32];
    for (i, word) in state.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

/// Derive a 32-byte salted, iterated SHA-256 hash for a PIN.
#[must_use]
pub fn derive_pin_hash(pin: &str, salt: &[u8; 16], iterations: u32) -> [u8; 32] {
    let rounds = iterations.max(1);
    let mut initial = Vec::with_capacity(salt.len() + pin.len());
    initial.extend_from_slice(salt);
    initial.extend_from_slice(pin.as_bytes());
    let mut digest = sha256(&initial);
    zeroize_secret(&mut initial);

    let mut step = [0_u8; 48];
    step[32..48].copy_from_slice(salt);
    for _ in 1..rounds {
        step[..32].copy_from_slice(&digest);
        digest = sha256(&step);
    }
    step.fill(0);
    std::hint::black_box(&step);
    compiler_fence(Ordering::SeqCst);
    digest
}

/// Constant-time comparison of two 32-byte digests.
#[must_use]
pub fn ct_eq(a: &[u8; 32], b: &[u8; 32]) -> bool {
    let mut diff = 0_u8;
    for (&x, &y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    std::hint::black_box(diff) == 0
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

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

/// Persisted salted PIN credential (no plaintext PIN stored).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PinCredential {
    pub salt_hex: String,
    pub hash_hex: String,
    pub iterations: u32,
}

impl PinCredential {
    /// Create a salted, iterated PIN credential.
    #[must_use]
    pub fn create(pin: &str, salt: [u8; 16], iterations: u32) -> Self {
        let rounds = iterations.max(1);
        let hash = derive_pin_hash(pin, &salt, rounds);
        Self {
            salt_hex: hex_encode(&salt),
            hash_hex: hex_encode(&hash),
            iterations: rounds,
        }
    }

    /// Verify `pin` against this stored credential using constant-time comparison.
    #[must_use]
    pub fn verify(&self, pin: &str) -> bool {
        if self.iterations == 0 {
            return false;
        }
        let Some(salt) = hex_decode_exact::<16>(&self.salt_hex) else {
            return false;
        };
        let Some(expected) = hex_decode_exact::<32>(&self.hash_hex) else {
            return false;
        };
        let computed = derive_pin_hash(pin, &salt, self.iterations);
        ct_eq(&computed, &expected)
    }
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

    #[test]
    fn sha256_matches_nist_test_vectors() {
        assert_eq!(
            hex_encode(&sha256(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hex_encode(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hex_encode(&sha256(
                b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
            )),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn pin_credential_verifies_correct_and_rejects_wrong_pin() {
        let salt = [0x11_u8; 16];
        let cred = PinCredential::create("482910", salt, 256);
        assert_eq!(cred.iterations, 256);
        assert!(cred.verify("482910"));
        assert!(!cred.verify("482911"));
        assert!(!cred.verify(""));
        assert!(!cred.verify("4829100"));

        let encoded = serde_json::to_string(&cred).expect("serialize PinCredential");
        let decoded: PinCredential =
            serde_json::from_str(&encoded).expect("deserialize PinCredential");
        assert_eq!(decoded, cred);
        assert!(decoded.verify("482910"));
    }

    #[test]
    fn pin_hash_differs_with_different_salt() {
        let salt_a = [0x01_u8; 16];
        let salt_b = [0x02_u8; 16];
        let hash_a = derive_pin_hash("1234", &salt_a, 128);
        let hash_b = derive_pin_hash("1234", &salt_b, 128);
        assert_ne!(hash_a, hash_b);
        assert_eq!(DEFAULT_PIN_ITERATIONS, 50_000);
    }

    #[test]
    fn ct_eq_compares_all_bytes_accurately() {
        let a = sha256(b"rabbiz-pin-test");
        let b = a;
        assert!(ct_eq(&a, &b));

        let mut first_diff = a;
        first_diff[0] ^= 0x01;
        assert!(!ct_eq(&a, &first_diff));

        let mut last_diff = a;
        last_diff[31] ^= 0x80;
        assert!(!ct_eq(&a, &last_diff));
    }
}
