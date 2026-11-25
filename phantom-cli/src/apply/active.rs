//! The active-profile record: which profile the operator applied, so a
//! reboot can restore it.
//!
//! On Linux a spoofed MAC does not survive a reboot: the NIC comes up on
//! its hardware address. The systemd unit reads this record at boot and
//! reapplies the profile. On Windows the registry values persist on their
//! own, and the service reads the same record at start; reapplying is
//! idempotent there.
//!
//! `apply` writes the record, `revert` clears it. It lives next to the
//! profiles at `<data_dir>/profiles/.config.json`, the one machine-wide
//! store the elevated CLI and the service share. The service crate
//! re-exports this type, so there is a single on-disk shape, not two that
//! can drift.

use super::Layer;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ActiveConfig {
    pub active_profile: Option<String>,
    pub auto_apply: bool,
    pub layers: Vec<u8>,
}

impl ActiveConfig {
    /// Path of the on-disk record.
    pub fn path() -> PathBuf {
        crate::profile::profiles_dir().join(".config.json")
    }

    /// Build the record for a just-applied profile, without touching
    /// disk. Layers are stored as their numeric tags (0/1/2) so the
    /// record is platform-neutral.
    pub fn for_applied(profile: &str, layers: &[Layer]) -> Self {
        ActiveConfig {
            active_profile: Some(profile.to_string()),
            auto_apply: true,
            layers: layers.iter().map(|l| *l as u8).collect(),
        }
    }

    /// The profile and layer tags to reapply, or `None` when the record
    /// is inactive (no protected profile). Pure, so boot logic is
    /// testable without disk.
    pub fn planned(&self) -> Option<(&str, &[u8])> {
        if !self.auto_apply {
            return None;
        }
        self.active_profile
            .as_deref()
            .map(|name| (name, self.layers.as_slice()))
    }

    pub fn load() -> Option<Self> {
        Self::load_from(&Self::path()).ok().flatten()
    }

    /// Load an active-profile record from an explicit path.
    ///
    /// Returns `Ok(None)` when the file does not exist and `Err(...)` when the
    /// file cannot be read or contains corrupt JSON.
    pub fn load_from(path: &Path) -> Result<Option<Self>, String> {
        match std::fs::read_to_string(path) {
            Ok(data) => serde_json::from_str(&data)
                .map(Some)
                .map_err(|e| format!("parse active config: {}", e)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("read active config: {}", e)),
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let dir = crate::profile::ensure_profiles_dir()
            .map_err(|e| format!("ensure profiles dir: {}", e))?;
        self.save_to(&dir.join(".config.json"))
    }

    /// Save this record to an explicit path, creating parent directories as needed.
    pub fn save_to(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("ensure active config dir: {}", e))?;
        }
        let data =
            serde_json::to_string_pretty(self).map_err(|e| format!("serialize active: {}", e))?;
        std::fs::write(path, data).map_err(|e| format!("write active: {}", e))
    }

    /// Record `profile` as the applied profile to restore on boot.
    pub fn record_applied(profile: &str, layers: &[Layer]) -> Result<(), String> {
        Self::for_applied(profile, layers).save()
    }

    /// Clear the record so a reboot does not reapply. Removing the file
    /// is the unprotected state; a missing file is not an error.
    pub fn clear() -> Result<(), String> {
        Self::clear_at(&Self::path())
    }

    /// Clear an active-profile record at an explicit path. Missing files succeed.
    pub fn clear_at(path: &Path) -> Result<(), String> {
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("remove active: {}", e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_inactive() {
        let c = ActiveConfig::default();
        assert!(c.active_profile.is_none());
        assert!(!c.auto_apply);
        assert!(c.layers.is_empty());
        assert!(c.planned().is_none());
    }

    #[test]
    fn for_applied_maps_layer_tags() {
        let c = ActiveConfig::for_applied("work", &[Layer::Userland]);
        assert_eq!(c.active_profile.as_deref(), Some("work"));
        assert!(c.auto_apply);
        assert_eq!(c.layers, vec![2]);

        let c =
            ActiveConfig::for_applied("all", &[Layer::Firmware, Layer::Kernel, Layer::Userland]);
        assert_eq!(c.layers, vec![0, 1, 2]);
    }

    #[test]
    fn planned_returns_name_and_layers_when_active() {
        let c = ActiveConfig::for_applied("work", &[Layer::Userland]);
        let (name, layers) = c.planned().expect("active config should plan a reapply");
        assert_eq!(name, "work");
        assert_eq!(layers, &[2]);
    }

    #[test]
    fn planned_is_none_when_auto_apply_off() {
        let c = ActiveConfig {
            active_profile: Some("work".into()),
            auto_apply: false,
            layers: vec![2],
        };
        assert!(c.planned().is_none());
    }

    #[test]
    fn serde_round_trip() {
        let c = ActiveConfig::for_applied("p", &[Layer::Userland]);
        let json = serde_json::to_string(&c).unwrap();
        let back: ActiveConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn save_load_and_clear_round_trip() {
        let dir = std::env::temp_dir().join(format!(
            "phantom-active-rt-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let path = dir.join(".config.json");

        assert_eq!(ActiveConfig::load_from(&path).unwrap(), None);

        let cfg = ActiveConfig::for_applied("roundtrip", &[Layer::Kernel, Layer::Userland]);
        cfg.save_to(&path).unwrap();
        let loaded = ActiveConfig::load_from(&path).unwrap();
        assert_eq!(loaded, Some(cfg));

        ActiveConfig::clear_at(&path).unwrap();
        assert_eq!(ActiveConfig::load_from(&path).unwrap(), None);
        // Clearing an already-absent file is idempotent.
        ActiveConfig::clear_at(&path).unwrap();

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_from_corrupt_json_returns_error() {
        let dir = std::env::temp_dir().join(format!(
            "phantom-active-corrupt-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(".config.json");
        std::fs::write(&path, "{not valid json").unwrap();

        let err = ActiveConfig::load_from(&path).unwrap_err();
        assert!(
            err.contains("parse active config"),
            "unexpected error: {}",
            err
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
