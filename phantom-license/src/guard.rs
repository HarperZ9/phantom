use crate::fingerprint::MachineFingerprint;
use crate::integrity;
use crate::key::{validate_license_key, License, LicenseError, LicenseTier};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
struct StoredLicense {
    key: String,
    activated_at: u64,
}

pub struct LicenseGuard {
    license: Option<License>,
    tier: LicenseTier,
    key_str: Option<String>,
}

impl LicenseGuard {
    pub fn load() -> Self {
        let path = license_file_path();

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
                return LicenseGuard {
                    license: None,
                    tier: LicenseTier::Free,
                    key_str: None,
                };
            }
        }

        if let Ok(data) = std::fs::read_to_string(&path) {
            if let Ok(stored) = serde_json::from_str::<StoredLicense>(&data) {
                if let Ok(license) = validate_license_key(&stored.key) {
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

        let stored = StoredLicense {
            key: key_str.to_string(),
            activated_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
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
    let base = if let Ok(dir) = std::env::var("PHANTOM_DATA_DIR") {
        PathBuf::from(dir)
    } else if cfg!(windows) {
        std::env::var("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("phantom")
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
}
