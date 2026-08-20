//! Runtime deobfuscation of the master signing key, plus
//! domain-separated per-purpose subkey derivation.
//!
//! The plaintext master key is never present in the shipping binary.
//! `build.rs` XOR-scrambles it into `OBFUSCATED_MASTER` at compile time;
//! `master_key()` unscrambles it into a stack buffer on demand.
//!
//! No caller should hold the master key longer than one call — every
//! consumer takes a *purpose-derived subkey* via `derive_key()` so that
//! recovering (for example) the license-signing subkey does not also
//! give the attacker the integrity-check subkey.
//!
//! Domain-separation strings live below as `*_PURPOSE` constants. They
//! ARE visible in the binary; that is intentional. Knowing "which
//! purposes exist" without the master key is worthless.

use hmac::{Hmac, Mac};
use sha2::Sha256;

include!(concat!(env!("OUT_DIR"), "/master_key_obf.rs"));

type HmacSha256 = Hmac<Sha256>;

pub const LICENSE_PURPOSE: &[u8] = b"phantom.license.v1";
pub const INTEGRITY_PURPOSE: &[u8] = b"phantom.integrity.v1";
pub const STATE_PURPOSE: &[u8] = b"phantom.state.v1";
pub const TIME_ANCHOR_PURPOSE: &[u8] = b"phantom.time-anchor.v1";

/// Deliberately non-obvious mixing function. Must match `build.rs`
/// byte-for-byte or the whole thing collapses. `#[inline(never)]` keeps
/// the compiler from constant-folding the loop below into the master
/// key itself — that would put the plaintext right back in .rodata.
#[inline(never)]
fn derive_xor_byte(i: usize) -> u8 {
    const A: u32 = 0xA5F3_7B24;
    const B: u32 = 0x9E3E_1C71;
    const C: u32 = 0x4B27_D9A6;

    let i32_ = i as u32;
    let rot = A.rotate_left(i32_ & 31);
    let sum = rot.wrapping_add(B.wrapping_mul(i32_.wrapping_add(17)));
    let mix = sum ^ C.rotate_right((i32_ * 3) & 31);
    ((mix >> ((i32_ & 3) * 8)) & 0xFF) as u8
}

/// Reconstruct the master key into a stack buffer. Callers should treat
/// the result as short-lived and hand it straight to a derivation call.
#[inline(never)]
fn master_key() -> [u8; 32] {
    let mut key = [0u8; 32];
    let obf = std::hint::black_box(&OBFUSCATED_MASTER);
    for i in 0..32 {
        key[i] = obf[i] ^ derive_xor_byte(i);
    }
    key
}

/// HKDF-style derivation: subkey = HMAC-SHA256(master_key, purpose).
/// Each subsystem gets a unique key; leaking one does not compromise
/// the others. The stack buffer holding the master key is
/// volatile-zeroed the moment the HMAC is finalized, so it does not
/// live on to be picked up by a core dump or a stack-scanner.
pub fn derive_key(purpose: &[u8]) -> [u8; 32] {
    let mut mk = master_key();
    let mut mac = HmacSha256::new_from_slice(&mk).expect("HMAC key length is fixed");
    mac.update(purpose);
    let bytes = mac.finalize().into_bytes();
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    volatile_zero(&mut mk);
    out
}

/// Volatile write-zeros over a buffer. `write_volatile` prevents the
/// compiler from optimizing the writes away as dead stores — plain
/// `mk = [0; 32]` on a stack-local that goes out of scope compiles
/// to nothing.
fn volatile_zero(buf: &mut [u8]) {
    for b in buf.iter_mut() {
        unsafe { std::ptr::write_volatile(b, 0) };
    }
    // Compiler fence keeps the writes from being reordered past this
    // point (they cannot cross a Release fence).
    std::sync::atomic::fence(std::sync::atomic::Ordering::Release);
}

/// Current master key generation. Bumped in build.rs when the seed
/// rotates so callers can gate on it if they need to.
pub fn master_generation() -> u8 {
    MASTER_KEY_GEN
}

#[cfg(test)]
mod tests {
    use super::*;

    // Master key is stable across calls. Regression guard against a
    // refactor that would accidentally use random/temporal state.
    #[test]
    fn master_key_is_deterministic() {
        let a = master_key();
        let b = master_key();
        assert_eq!(a, b);
    }

    // The master key must NOT be all zero — if the build script ever
    // silently produced a zero key, the obfuscation would be a no-op.
    #[test]
    fn master_key_is_not_trivial() {
        let mk = master_key();
        assert_ne!(mk, [0u8; 32]);
        assert_ne!(mk, [0xFFu8; 32]);
        // Byte spread — no more than 4 identical bytes in a row.
        let mut run = 1;
        for i in 1..mk.len() {
            if mk[i] == mk[i - 1] {
                run += 1;
                assert!(run <= 4, "master key has a run of {run} identical bytes");
            } else {
                run = 1;
            }
        }
    }

    // Different purposes MUST produce distinct subkeys.
    #[test]
    fn purposes_produce_distinct_keys() {
        let license = derive_key(LICENSE_PURPOSE);
        let integrity = derive_key(INTEGRITY_PURPOSE);
        let state = derive_key(STATE_PURPOSE);
        let anchor = derive_key(TIME_ANCHOR_PURPOSE);

        assert_ne!(license, integrity);
        assert_ne!(license, state);
        assert_ne!(license, anchor);
        assert_ne!(integrity, state);
        assert_ne!(integrity, anchor);
        assert_ne!(state, anchor);
    }

    // Derivation is deterministic — same purpose in different calls
    // yields the same subkey.
    #[test]
    fn derivation_is_deterministic() {
        let a = derive_key(LICENSE_PURPOSE);
        let b = derive_key(LICENSE_PURPOSE);
        assert_eq!(a, b);
    }

    // The obfuscated bytes should NOT equal the derived master. If they
    // did, obfuscation collapsed to identity and `strings` reveals it.
    #[test]
    fn obfuscated_bytes_differ_from_master() {
        let mk = master_key();
        assert_ne!(mk[..], OBFUSCATED_MASTER[..]);
    }

    // The XOR function must produce a non-constant sequence. If it
    // returned the same byte for every index, every position would be
    // scrambled with the same key and `strings` would still leak the
    // pattern.
    #[test]
    fn xor_function_is_position_dependent() {
        let mut seen = std::collections::HashSet::new();
        for i in 0..32 {
            seen.insert(derive_xor_byte(i));
        }
        assert!(
            seen.len() >= 16,
            "XOR key had only {} distinct bytes across 32 positions",
            seen.len()
        );
    }

    // volatile_zero must actually zero its buffer, and must not be
    // dead-code-eliminated the way a plain `= [0; N]` would be.
    #[test]
    fn volatile_zero_wipes_buffer() {
        let mut buf = [0x5Au8; 32];
        super::volatile_zero(&mut buf);
        assert_eq!(buf, [0u8; 32]);
    }
}
