use crate::fingerprint::MachineFingerprint;
use crate::integrity;
use crate::key::{validate_license_key, License, LicenseError, LicenseTier};
use crate::keys;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::path::PathBuf;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Serialize, Deserialize)]
struct StoredLicense {
    key: String,
    activated_at: u64,
    /// HMAC-SHA256 over `key` + LE bytes of `activated_at`, using the
    /// STATE_PURPOSE subkey. Prevents field tampering (e.g. rewriting
    /// `activated_at` on a machine where the anchor already advanced,
    /// or pasting in an unrelated key). Older records without this
    /// field are accepted for one migration cycle and re-signed on
    /// next save.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    state_mac: String,
}

fn compute_state_mac(key: &str, activated_at: u64) -> String {
    let sk = keys::derive_key(keys::STATE_PURPOSE);
    let mut mac = HmacSha256::new_from_slice(&sk).expect("HMAC key length is fixed");
    mac.update(key.as_bytes());
    mac.update(&activated_at.to_le_bytes());
    let bytes = mac.finalize().into_bytes();
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn verify_state_mac(stored: &StoredLicense) -> bool {
    if stored.state_mac.is_empty() {
        // Legacy record from before the state MAC existed. Accept once;
        // it will be re-signed on the next activation cycle.
        return true;
    }
    let expected = compute_state_mac(&stored.key, stored.activated_at);
    let a = expected.as_bytes();
    let b = stored.state_mac.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

pub struct LicenseGuard {
    license: Option<License>,
    tier: LicenseTier,
    key_str: Option<String>,
}

impl LicenseGuard {
    pub fn load() -> Self {
        let path = license_file_path();

        // Tripwire gate: if any High-severity tamper event has been
        // recorded on this machine, silently return Free tier from
        // this point on. No error is printed — the reverser has no
        // signal to grep for. The tripwire clears on successful
        // reactivation, so a legitimate operator whose install was
        // flagged in error can recover by re-running
        // `phantom license activate <their-real-key>`.
        if let Some(parent) = path.parent() {
            if crate::tripwire::is_tripped(parent) {
                return LicenseGuard {
                    license: None,
                    tier: LicenseTier::Free,
                    key_str: None,
                };
            }
        }

        // Advance the time anchor first. If the wall clock has been
        // rewound beyond the grace window, refuse to honor any stored
        // license this cycle — the attacker is trying to freeze time
        // to bypass expiration. `NoAnchor` is fine: it just means we
        // have not seen this machine before.
        if let Some(parent) = path.parent() {
            if matches!(
                crate::time_anchor::check_and_advance(parent),
                crate::time_anchor::AnchorVerdict::ClockRewound { .. }
            ) {
                crate::tripwire::record(parent, crate::tripwire::Severity::High, "clock_rewound");
                return LicenseGuard {
                    license: None,
                    tier: LicenseTier::Free,
                    key_str: None,
                };
            }
        }

        if let Ok(data) = std::fs::read_to_string(&path) {
            if let Ok(stored) = serde_json::from_str::<StoredLicense>(&data) {
                if !verify_state_mac(&stored) {
                    // State MAC failure is a High-confidence tamper
                    // signal: nobody edits `.license.json` by hand
                    // for legitimate reasons, and the file cannot get
                    // there by accident with a valid MAC.
                    if let Some(parent) = path.parent() {
                        crate::tripwire::record(
                            parent,
                            crate::tripwire::Severity::High,
                            "state_mac_failed",
                        );
                    }
                } else if let Ok(license) = validate_license_key(&stored.key) {
                    let fp = MachineFingerprint::collect();
                    if license.is_bound_to(&fp) && !license.is_expired() {
                        let tier = license.tier;
                        return LicenseGuard {
                            license: Some(license),
                            tier,
                            key_str: Some(stored.key),
                        };
                    }
                }
            }
        }

        LicenseGuard {
            license: None,
            tier: LicenseTier::Free,
            key_str: None,
        }
    }

    pub fn activate(key_str: &str) -> Result<Self, LicenseError> {
        let path = license_file_path();
        let data_dir = path.parent().map(|p| p.to_path_buf());

        // Honey-key gate: strings-scraped decoy keys from the binary
        // trip a High-severity tripwire on the first attempt. From
        // that point on this install silently downgrades to Free
        // tier. Real users mistyping a real key never hit this — the
        // honey strings are distinctive (PHANTOM-MASTER-UNLOCK-...).
        if crate::tripwire::is_honey_key(key_str) {
            if let Some(dir) = data_dir.as_deref() {
                crate::tripwire::record(dir, crate::tripwire::Severity::High, "honey_key_attempt");
            }
            // Return the same error a plain invalid key would produce
            // so the caller cannot distinguish honey from garbage.
            return Err(LicenseError::InvalidSignature);
        }

        // Rate-limit gate: consult the attempt log first so brute-force
        // attempts are throttled before any key material is exercised.
        if let Some(dir) = data_dir.as_deref() {
            let cooldown = crate::rate_limit::required_cooldown_secs(dir);
            if cooldown > 0 {
                return Err(LicenseError::RateLimited(cooldown));
            }
        }

        // Any failure below records into the attempt log; the rate
        // limiter uses that to gate the *next* call.
        let outcome = Self::try_activate_inner(key_str);
        match &outcome {
            Ok(_) => {
                if let Some(dir) = data_dir.as_deref() {
                    crate::rate_limit::clear(dir);
                    // A verified activation is the ONLY way to clear
                    // the tripwire. Legitimate operators whose
                    // install was flagged in error recover by
                    // re-running activate with their real key.
                    crate::tripwire::clear(dir);
                }
            }
            Err(_) => {
                if let Some(dir) = data_dir.as_deref() {
                    crate::rate_limit::record_failure(dir);
                }
            }
        }
        outcome
    }

    fn try_activate_inner(key_str: &str) -> Result<Self, LicenseError> {
        if !integrity::self_check() {
            return Err(LicenseError::InvalidSignature);
        }

        let license = validate_license_key(key_str)?;

        if license.is_expired() {
            return Err(LicenseError::Expired);
        }

        let fp = MachineFingerprint::collect();
        if !license.is_bound_to(&fp) {
            return Err(LicenseError::MachineMismatch);
        }

        let tier = license.tier;
        let activated_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let stored = StoredLicense {
            key: key_str.to_string(),
            activated_at,
            state_mac: compute_state_mac(key_str, activated_at),
        };

        let path = license_file_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let json =
            serde_json::to_string_pretty(&stored).map_err(|_| LicenseError::InvalidFormat)?;
        std::fs::write(&path, json).map_err(|_| LicenseError::InvalidFormat)?;

        Ok(LicenseGuard {
            license: Some(license),
            tier,
            key_str: Some(key_str.to_string()),
        })
    }

    pub fn deactivate(&mut self) {
        let path = license_file_path();
        let _ = std::fs::remove_file(path);
        self.license = None;
        self.tier = LicenseTier::Free;
        self.key_str = None;
    }

    pub fn tier(&self) -> LicenseTier {
        self.tier
    }

    pub fn is_licensed(&self) -> bool {
        self.license.is_some()
    }

    pub fn check_layer(&self, layer: u8) -> Result<(), LicenseError> {
        // Fanout callsite: any pre-flight gate on privileged operations
        // re-runs the debugger ensemble. Patching just LicenseGuard::
        // activate() out is not enough — an attacker also has to
        // silence every gate.
        if !integrity::self_check() {
            return Err(LicenseError::InvalidSignature);
        }
        if self.tier.allows_layer(layer) {
            Ok(())
        } else {
            Err(LicenseError::InsufficientTier)
        }
    }

    pub fn check_profile_limit(&self, current_count: usize) -> Result<(), LicenseError> {
        if current_count < self.tier.max_profiles() {
            Ok(())
        } else {
            Err(LicenseError::InsufficientTier)
        }
    }

    pub fn check_service(&self) -> Result<(), LicenseError> {
        // Same rationale as `check_layer`: service-mode is a paid-
        // tier operation and the gate re-runs detection.
        if !integrity::self_check() {
            return Err(LicenseError::InvalidSignature);
        }
        if self.tier.allows_service() {
            Ok(())
        } else {
            Err(LicenseError::InsufficientTier)
        }
    }

    pub fn license_info(&self) -> Option<&License> {
        self.license.as_ref()
    }

    pub fn days_remaining(&self) -> Option<u32> {
        self.license.as_ref().and_then(|l| l.days_remaining())
    }
}

fn license_file_path() -> PathBuf {
    // Must resolve to the same machine-wide store as phantom-cli's
    // data_dir(). phantom-license is a lower crate than phantom-cli, so
    // the base logic is duplicated here rather than imported; keep the two
    // in sync. On Windows this is %ProgramData%\Phantom (identical for the
    // user and LocalSystem); per-user %APPDATA% split the store.
    let base = if let Ok(dir) = std::env::var("PHANTOM_DATA_DIR") {
        PathBuf::from(dir)
    } else if cfg!(windows) {
        std::env::var("ProgramData")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(r"C:\ProgramData"))
            .join("Phantom")
    } else {
        std::env::var("HOME")
            .map(|h| PathBuf::from(h).join(".config"))
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("phantom")
    };
    base.join(".license.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unactivated_guard_is_free() {
        let guard = LicenseGuard {
            license: None,
            tier: LicenseTier::Free,
            key_str: None,
        };
        assert!(!guard.is_licensed());
        assert_eq!(guard.tier(), LicenseTier::Free);
    }

    #[test]
    fn free_tier_restricts_layers() {
        let guard = LicenseGuard {
            license: None,
            tier: LicenseTier::Free,
            key_str: None,
        };
        assert!(guard.check_layer(2).is_ok());
        assert!(guard.check_layer(0).is_err());
        assert!(guard.check_layer(1).is_err());
    }

    #[test]
    fn free_tier_restricts_service() {
        let guard = LicenseGuard {
            license: None,
            tier: LicenseTier::Free,
            key_str: None,
        };
        assert!(guard.check_service().is_err());
    }

    #[test]
    fn profile_limit_enforcement() {
        let guard = LicenseGuard {
            license: None,
            tier: LicenseTier::Free,
            key_str: None,
        };
        assert!(guard.check_profile_limit(0).is_ok());
        assert!(guard.check_profile_limit(1).is_ok());
        assert!(guard.check_profile_limit(2).is_err());
    }

    // A record with a MAC that matches the (key, activated_at) pair
    // verifies. Regression guard.
    #[test]
    fn state_mac_valid_record_verifies() {
        let stored = StoredLicense {
            key: "PHNTM-TEST-KEY".into(),
            activated_at: 1_700_000_000,
            state_mac: compute_state_mac("PHNTM-TEST-KEY", 1_700_000_000),
        };
        assert!(verify_state_mac(&stored));
    }

    // Rewriting activated_at without recomputing the MAC must fail.
    // Prevents an attacker from ageing the record forward to escape
    // the time anchor grace window, or backward to earn free days.
    #[test]
    fn state_mac_tampered_activated_at_rejected() {
        let mac = compute_state_mac("PHNTM-TEST-KEY", 1_700_000_000);
        let tampered = StoredLicense {
            key: "PHNTM-TEST-KEY".into(),
            activated_at: 1_800_000_000, // moved forward
            state_mac: mac,
        };
        assert!(!verify_state_mac(&tampered));
    }

    // Swapping the license key while keeping the old MAC must fail.
    #[test]
    fn state_mac_swapped_key_rejected() {
        let mac = compute_state_mac("ORIGINAL-KEY", 1_700_000_000);
        let tampered = StoredLicense {
            key: "REPLACED-KEY".into(),
            activated_at: 1_700_000_000,
            state_mac: mac,
        };
        assert!(!verify_state_mac(&tampered));
    }

    // Legacy records (written before Sprint 13) have no state_mac
    // field and must still load exactly once, so upgrading users
    // don't get bumped to Free tier on first launch. They will be
    // re-signed on their next activation.
    #[test]
    fn state_mac_absent_is_accepted_for_migration() {
        let legacy = StoredLicense {
            key: "LEGACY-KEY".into(),
            activated_at: 1_700_000_000,
            state_mac: String::new(),
        };
        assert!(verify_state_mac(&legacy));
    }
}
