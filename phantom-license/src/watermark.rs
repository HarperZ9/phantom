//! Signed origin marks on generated profiles.
//!
//! A profile that Phantom generates is fingerprinted, tagged with the
//! generating machine's fingerprint and license tier, and HMAC-signed
//! with the STATE_PURPOSE subkey. On import, three outcomes are
//! possible:
//!
//! 1. **Unmarked** — the file has no `origin_mark`. Legal; loads with
//!    a warning. This is the shape of every pre-Sprint-14 profile.
//!
//! 2. **Invalid** — the mark is present but its MAC does not verify.
//!    The file has been edited by hand or its mark forged. Rejected.
//!
//! 3. **Valid** — the MAC verifies. Two sub-cases:
//!    a. `origin_fingerprint == this_machine`: local profile, no
//!       restriction.
//!    b. `origin_fingerprint != this_machine`: foreign profile. The
//!       CLI's import path enforces the tier policy — Free tier
//!       refuses foreign profiles; Pro and Enterprise accept them.
//!
//! What this defends against: a Free-tier user cannot pull a
//! Pro-generated profile off someone else's machine and use it. The
//! plaintext-JSON export/import format stays portable, but portability
//! is a paid-tier feature enforced by the client.
//!
//! What this does NOT defend against: a Pro-tier user redistributing
//! their marked profiles. That is a licensing-terms violation, not a
//! technical one — Phantom is not DRM.

use crate::keys;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

type HmacSha256 = Hmac<Sha256>;

/// Wire format for a signed origin mark. Attached to a
/// `HardwareProfile` under `metadata.origin_mark`.
///
/// The MAC covers, in this exact order:
///   profile_hash (32 bytes) || origin_fingerprint (16) ||
///   origin_tier_byte (1) || issued_epoch_days (LE u32, 4)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OriginMark {
    /// Hex SHA-256 of the profile's canonical serialization (with
    /// `origin_mark` cleared) at the moment of signing.
    pub profile_hash_hex: String,
    /// Hex of the 16-byte machine fingerprint that generated the
    /// profile.
    pub origin_fingerprint_hex: String,
    /// String form of the tier the origin machine held at generation
    /// ("Free", "Pro", "Enterprise").
    pub origin_tier: String,
    /// Unix days at generation.
    pub issued_epoch_days: u32,
    /// Hex HMAC-SHA256 signature.
    pub mac_hex: String,
}

fn tier_byte(tier_str: &str) -> u8 {
    match tier_str {
        "Free" => 0,
        "Pro" => 1,
        "Enterprise" => 2,
        _ => 0xFF,
    }
}

fn compute_mark_mac(
    profile_hash: &[u8; 32],
    origin_fingerprint: &[u8; 16],
    tier_str: &str,
    issued_epoch_days: u32,
) -> String {
    let sk = keys::derive_key(keys::STATE_PURPOSE);
    let mut mac = HmacSha256::new_from_slice(&sk).expect("HMAC key length is fixed");
    mac.update(profile_hash);
    mac.update(origin_fingerprint);
    mac.update(&[tier_byte(tier_str)]);
    mac.update(&issued_epoch_days.to_le_bytes());
    let bytes = mac.finalize().into_bytes();
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn hex_to_bytes<const N: usize>(hex: &str) -> Option<[u8; N]> {
    if hex.len() != 2 * N {
        return None;
    }
    let mut out = [0u8; N];
    for i in 0..N {
        out[i] = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).ok()?;
    }
    Some(out)
}

/// Constant-time hex-string comparison.
fn ct_eq(a: &str, b: &str) -> bool {
    let a = a.as_bytes();
    let b = b.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Sign a profile-content hash + fingerprint + tier into an OriginMark.
///
/// Callers hash the profile with `origin_mark` unset, hand the digest
/// in here, and then embed the returned mark. This lets the mark cover
/// the exact bytes the caller is about to write.
pub fn sign(
    profile_hash: [u8; 32],
    origin_fingerprint: [u8; 16],
    tier_str: &str,
    issued_epoch_days: u32,
) -> OriginMark {
    OriginMark {
        profile_hash_hex: profile_hash.iter().map(|b| format!("{:02x}", b)).collect(),
        origin_fingerprint_hex: origin_fingerprint
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect(),
        origin_tier: tier_str.to_string(),
        issued_epoch_days,
        mac_hex: compute_mark_mac(
            &profile_hash,
            &origin_fingerprint,
            tier_str,
            issued_epoch_days,
        ),
    }
}

/// Convenience wrapper for the common path: hash arbitrary canonical
/// bytes, sign, return the mark.
pub fn sign_bytes(
    canonical_bytes: &[u8],
    origin_fingerprint: [u8; 16],
    tier_str: &str,
    issued_epoch_days: u32,
) -> OriginMark {
    let mut hasher = Sha256::new();
    hasher.update(canonical_bytes);
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&hasher.finalize());
    sign(hash, origin_fingerprint, tier_str, issued_epoch_days)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// The mark verified and was issued on this machine.
    Local,
    /// The mark verified and was issued on a different machine that
    /// held the named tier.
    Foreign { origin_tier: String },
    /// A mark was present but its MAC did not verify.
    Invalid,
    /// The mark's covered hash does not match the profile's actual
    /// content (someone edited the profile without re-signing).
    ContentTampered,
    /// The mark has structural corruption (bad hex, wrong lengths).
    Malformed,
}

/// Verify a stored mark against a freshly-computed profile hash and
/// the caller's current fingerprint.
pub fn verify(mark: &OriginMark, profile_hash: &[u8; 32], this_fingerprint: &[u8; 16]) -> Verdict {
    let expected_profile_hex: String = profile_hash.iter().map(|b| format!("{:02x}", b)).collect();
    if !ct_eq(&mark.profile_hash_hex, &expected_profile_hex) {
        return Verdict::ContentTampered;
    }

    let origin_fp: [u8; 16] = match hex_to_bytes(&mark.origin_fingerprint_hex) {
        Some(b) => b,
        None => return Verdict::Malformed,
    };

    let expected_mac = compute_mark_mac(
        profile_hash,
        &origin_fp,
        &mark.origin_tier,
        mark.issued_epoch_days,
    );
    if !ct_eq(&expected_mac, &mark.mac_hex) {
        return Verdict::Invalid;
    }

    if &origin_fp == this_fingerprint {
        Verdict::Local
    } else {
        Verdict::Foreign {
            origin_tier: mark.origin_tier.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash_of(bytes: &[u8]) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        let mut h = [0u8; 32];
        h.copy_from_slice(&hasher.finalize());
        h
    }

    #[test]
    fn sign_then_verify_local() {
        let content = b"canonical profile bytes";
        let hash = hash_of(content);
        let fp = [0xAA; 16];
        let mark = sign(hash, fp, "Pro", 20_000);
        assert_eq!(verify(&mark, &hash, &fp), Verdict::Local);
    }

    #[test]
    fn sign_then_verify_foreign_returns_tier() {
        let hash = hash_of(b"content");
        let origin_fp = [0x11; 16];
        let this_fp = [0x22; 16];
        let mark = sign(hash, origin_fp, "Enterprise", 20_000);
        match verify(&mark, &hash, &this_fp) {
            Verdict::Foreign { origin_tier } => assert_eq!(origin_tier, "Enterprise"),
            v => panic!("expected Foreign(Enterprise), got {v:?}"),
        }
    }

    // Editing the profile content without re-signing must surface as
    // ContentTampered, not as Invalid — the distinction lets the CLI
    // give a useful error message.
    #[test]
    fn content_change_surfaces_as_tampered() {
        let hash = hash_of(b"original");
        let fp = [0x33; 16];
        let mark = sign(hash, fp, "Pro", 20_000);
        let mutated = hash_of(b"MUTATED");
        assert_eq!(verify(&mark, &mutated, &fp), Verdict::ContentTampered);
    }

    // A mark with a plausible profile_hash but a forged MAC must
    // Invalid, not Local. This is the key protection.
    #[test]
    fn forged_mac_is_invalid() {
        let hash = hash_of(b"content");
        let fp = [0x44; 16];
        let mut mark = sign(hash, fp, "Pro", 20_000);
        // Flip a nibble of the MAC.
        let mut chars: Vec<char> = mark.mac_hex.chars().collect();
        chars[0] = if chars[0] == 'a' { 'b' } else { 'a' };
        mark.mac_hex = chars.into_iter().collect();
        assert_eq!(verify(&mark, &hash, &fp), Verdict::Invalid);
    }

    // Tier substitution is critical: an attacker cannot claim a
    // Free-tier mark is really Enterprise without recomputing the MAC.
    #[test]
    fn tier_substitution_breaks_mac() {
        let hash = hash_of(b"content");
        let fp = [0x55; 16];
        let mut mark = sign(hash, fp, "Free", 20_000);
        mark.origin_tier = "Enterprise".into();
        assert_eq!(verify(&mark, &hash, &fp), Verdict::Invalid);
    }

    // Fingerprint substitution is equally critical: an attacker cannot
    // rewrite origin_fingerprint to match their own machine without
    // recomputing the MAC.
    #[test]
    fn fingerprint_substitution_breaks_mac() {
        let hash = hash_of(b"content");
        let origin_fp = [0x66; 16];
        let mut mark = sign(hash, origin_fp, "Pro", 20_000);
        let attacker_fp_hex: String = [0x77u8; 16].iter().map(|b| format!("{:02x}", b)).collect();
        mark.origin_fingerprint_hex = attacker_fp_hex;
        // Attacker imports on their own machine (fp = 0x77).
        assert_eq!(verify(&mark, &hash, &[0x77; 16]), Verdict::Invalid);
    }

    #[test]
    fn malformed_fingerprint_hex_is_malformed() {
        let hash = hash_of(b"content");
        let fp = [0x88; 16];
        let mut mark = sign(hash, fp, "Pro", 20_000);
        mark.origin_fingerprint_hex = "not-hex".into();
        // Content hash still matches, so we bypass ContentTampered and
        // hit Malformed.
        assert_eq!(verify(&mark, &hash, &fp), Verdict::Malformed);
    }

    // The sign_bytes convenience wrapper must agree with the manual
    // hash-then-sign path.
    #[test]
    fn sign_bytes_matches_manual_sign() {
        let content = b"canonical profile bytes";
        let fp = [0xAB; 16];
        let a = sign(hash_of(content), fp, "Pro", 20_000);
        let b = sign_bytes(content, fp, "Pro", 20_000);
        assert_eq!(a, b);
    }
}
