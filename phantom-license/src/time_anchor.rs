//! Monotonic time anchor for detecting clock rollback attacks against
//! license expiration.
//!
//! Threat model: the license expiration check reads
//! `SystemTime::now()`. An attacker who owns the machine can rewind the
//! system clock to a date before the license expired and pass every
//! expiration check indefinitely.
//!
//! Defense: on every LicenseGuard load, we record the current wall
//! time to `<data_dir>/.time_anchor`, HMAC'd with a purpose-derived
//! subkey so it cannot be spoofed without the master. On subsequent
//! loads we read the anchor first. If `now < anchor - grace`, we
//! declare the clock rewound and treat the license as invalid.
//!
//! The anchor is monotone-forward: it only ever advances. Any clock
//! movement backwards larger than [`GRACE_SECS`] is rejected.
//!
//! What this does NOT defend against: an attacker who deletes the
//! anchor file entirely. That merely resets the anchor at the next
//! load — the licence still enforces its own expiration against the
//! (possibly rewound) clock, but the attacker can no longer freeze
//! time indefinitely without also owning the ability to keep deleting
//! the file on every start. Better than nothing; documented as such.

use crate::keys;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

type HmacSha256 = Hmac<Sha256>;

/// Small clock jitter allowance (24 hours). Below this, a backwards
/// clock movement is chalked up to NTP correction or a genuine
/// timezone/DST accident, not tampering.
pub const GRACE_SECS: u64 = 60 * 60 * 24;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AnchorFile {
    /// Highest wall-clock unix time this machine has ever reported.
    highest_seen_unix_secs: u64,
    /// HMAC-SHA256 over `highest_seen_unix_secs.to_le_bytes()` using
    /// the time-anchor purpose subkey. Prevents casual editing.
    mac_hex: String,
}

fn compute_mac(highest: u64) -> String {
    let sk = keys::derive_key(keys::TIME_ANCHOR_PURPOSE);
    let mut mac = HmacSha256::new_from_slice(&sk).expect("HMAC key length is fixed");
    mac.update(&highest.to_le_bytes());
    let bytes = mac.finalize().into_bytes();
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn verify_mac(highest: u64, mac_hex: &str) -> bool {
    // Constant-time comparison at the hex level (both sides same length
    // because they come from the same encoding).
    let expected = compute_mac(highest);
    let a = expected.as_bytes();
    let b = mac_hex.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn anchor_path(data_dir: &Path) -> PathBuf {
    data_dir.join(".time_anchor")
}

fn load_anchor(path: &Path) -> Option<AnchorFile> {
    let s = std::fs::read_to_string(path).ok()?;
    let a: AnchorFile = serde_json::from_str(&s).ok()?;
    if verify_mac(a.highest_seen_unix_secs, &a.mac_hex) {
        Some(a)
    } else {
        None
    }
}

fn save_anchor(path: &Path, highest: u64) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let anchor = AnchorFile {
        highest_seen_unix_secs: highest,
        mac_hex: compute_mac(highest),
    };
    let json = serde_json::to_string_pretty(&anchor)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(path, json)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorVerdict {
    /// Wall clock is at or after the highest previously seen time
    /// (within the grace window). Normal operation.
    Ok,
    /// Wall clock is more than [`GRACE_SECS`] behind the last anchor.
    /// Treat any expiration checks as failed until the clock catches
    /// back up.
    ClockRewound { anchor_secs: u64, now_secs: u64 },
    /// The anchor file was missing, corrupt, or forged. A fresh anchor
    /// is written at `now`. Callers should still trust the license
    /// this cycle (nothing to compare against yet).
    NoAnchor,
}

/// Update the anchor and return a verdict about whether the current
/// wall clock is trustworthy. Called during LicenseGuard::load().
pub fn check_and_advance(data_dir: &Path) -> AnchorVerdict {
    let path = anchor_path(data_dir);
    let now = now_unix_secs();

    match load_anchor(&path) {
        Some(anchor) => {
            let highest = anchor.highest_seen_unix_secs;
            if now + GRACE_SECS < highest {
                AnchorVerdict::ClockRewound {
                    anchor_secs: highest,
                    now_secs: now,
                }
            } else {
                if now > highest {
                    let _ = save_anchor(&path, now);
                }
                AnchorVerdict::Ok
            }
        }
        None => {
            let _ = save_anchor(&path, now);
            AnchorVerdict::NoAnchor
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_dir() -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNT: AtomicUsize = AtomicUsize::new(0);
        let id = COUNT.fetch_add(1, Ordering::SeqCst);
        let p = std::env::temp_dir().join(format!("phantom-anchor-{}-{}", std::process::id(), id));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn mac_roundtrip() {
        let m = compute_mac(1_700_000_000);
        assert!(verify_mac(1_700_000_000, &m));
        assert!(!verify_mac(1_700_000_001, &m));
    }

    #[test]
    fn forged_mac_rejected() {
        let path = scratch_dir().join(".time_anchor");
        let anchor = AnchorFile {
            highest_seen_unix_secs: 9_999_999_999,
            mac_hex: "00".repeat(32),
        };
        std::fs::write(&path, serde_json::to_string(&anchor).unwrap()).unwrap();
        assert!(load_anchor(&path).is_none());
    }

    #[test]
    fn first_call_is_no_anchor_then_ok() {
        let dir = scratch_dir();
        assert_eq!(check_and_advance(&dir), AnchorVerdict::NoAnchor);
        assert_eq!(check_and_advance(&dir), AnchorVerdict::Ok);
    }

    #[test]
    fn future_anchor_triggers_rewound() {
        let dir = scratch_dir();
        // Plant an anchor from the year 2999.
        let path = anchor_path(&dir);
        let far_future: u64 = 32_503_680_000; // 3000-01-01
        save_anchor(&path, far_future).unwrap();

        match check_and_advance(&dir) {
            AnchorVerdict::ClockRewound {
                anchor_secs,
                now_secs,
            } => {
                assert_eq!(anchor_secs, far_future);
                assert!(now_secs < far_future);
            }
            other => panic!("expected ClockRewound, got {other:?}"),
        }
    }

    #[test]
    fn small_backwards_movement_is_ok() {
        let dir = scratch_dir();
        let path = anchor_path(&dir);
        // Anchor is 1 hour in the future (well inside GRACE_SECS).
        save_anchor(&path, now_unix_secs() + 3600).unwrap();
        assert_eq!(check_and_advance(&dir), AnchorVerdict::Ok);
    }
}
