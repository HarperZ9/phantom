use crate::profile::schema::HardwareProfile;

#[derive(Debug)]
pub struct ApplyResult {
    pub applied: Vec<String>,
    pub failed: Vec<(String, String)>,
    pub skipped: Vec<String>,
}

impl ApplyResult {
    pub fn success(&self) -> bool {
        self.failed.is_empty()
    }
}

pub fn apply_registry_layer(profile: &HardwareProfile) -> ApplyResult {
    #[cfg(windows)]
    {
        apply_registry_windows(profile)
    }
    #[cfg(not(windows))]
    {
        let _ = profile;
        ApplyResult {
            applied: Vec::new(),
            failed: vec![(
                "registry".into(),
                "Registry spoofing requires Windows".into(),
            )],
            skipped: Vec::new(),
        }
    }
}

pub fn revert_registry_layer(backup: &RegistryBackup) -> ApplyResult {
    #[cfg(windows)]
    {
        revert_registry_windows(backup)
    }
    #[cfg(not(windows))]
    {
        let _ = backup;
        ApplyResult {
            applied: Vec::new(),
            failed: vec![("registry".into(), "Registry revert requires Windows".into())],
            skipped: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RegistryBackup {
    pub entries: Vec<RegistryBackupEntry>,
    pub created_at: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RegistryBackupEntry {
    pub path: String,
    pub value_name: String,
    pub original_value: String,
}

/// Path to the registry backup written on `apply` and read on `revert`
/// / pre-uninstall cleanup.
///
/// Derived from [`crate::profile::data_dir`] so it lives in the SAME
/// machine-wide store as everything else and honors `PHANTOM_DATA_DIR`.
/// Previously this computed its own per-user `%APPDATA%` base, which the
/// LocalSystem uninstall cleanup could not read — the root cause of
/// uninstall failing to restore the original identity.
pub fn backup_path() -> std::path::PathBuf {
    crate::profile::data_dir().join("backup.json")
}

pub fn save_backup(backup: &RegistryBackup) -> std::io::Result<()> {
    let path = backup_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(backup)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(&path, json)
}

pub fn load_backup() -> std::io::Result<RegistryBackup> {
    let path = backup_path();
    let json = std::fs::read_to_string(&path)?;
    serde_json::from_str(&json).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

/// Reads the spoofed string value for one identifier out of a profile.
type ProfileField = fn(&HardwareProfile) -> String;

/// The Layer-2 registry string identifiers Phantom spoofs, as
/// `(key path, value name, profile field)`.
///
/// ComputerName is deliberately ABSENT. Writing only the two ComputerName
/// registry values (`Control\ComputerName\{ComputerName,ActiveComputerName}`)
/// desyncs the machine name from the many other places Windows records it
/// (Netbt, `Tcpip\Parameters\Hostname` / `NV Hostname`, the Kerberos /
/// NetBIOS name). That half-rename breaks `shutdown`, `Restart-Computer`,
/// and WMI's local connection until reverted. A safe rename touches all of
/// those and requires a reboot, which is out of scope for the Layer-2
/// registry tool. Reinstate ComputerName here only alongside a full,
/// reboot-completed rename implementation. `registry_targets_exclude_computer_name`
/// guards against a silent re-add.
const STRING_TARGETS: &[(&str, &str, ProfileField)] = &[
    (r"SOFTWARE\Microsoft\Cryptography", "MachineGuid", |p| {
        p.os.machine_guid.clone()
    }),
    (
        r"SYSTEM\CurrentControlSet\Control\IDConfigDB\Hardware Profiles\0001",
        "HwProfileGuid",
        |p| p.os.hw_profile_guid.clone(),
    ),
    (r"SOFTWARE\Microsoft\SQMClient", "MachineId", |p| {
        p.os.machine_id.clone()
    }),
    (
        r"SOFTWARE\Microsoft\Windows NT\CurrentVersion",
        "ProductId",
        |p| p.os.product_id.clone(),
    ),
];

#[cfg(windows)]
fn apply_registry_windows(profile: &HardwareProfile) -> ApplyResult {
    use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_ALL_ACCESS};
    use winreg::RegKey;

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let mut applied = Vec::new();
    let mut failed = Vec::new();
    let mut backup_entries = Vec::new();

    for (path, name, field) in STRING_TARGETS {
        let new_value = field(profile);
        match hklm.open_subkey_with_flags(path, KEY_ALL_ACCESS) {
            Ok(key) => {
                if let Ok(old_value) = key.get_value::<String, _>(name) {
                    backup_entries.push(RegistryBackupEntry {
                        path: path.to_string(),
                        value_name: name.to_string(),
                        original_value: old_value,
                    });
                }
                match key.set_value(name, &new_value) {
                    Ok(_) => applied.push(format!("{}\\{}", path, name)),
                    Err(e) => failed.push((format!("{}\\{}", path, name), e.to_string())),
                }
            }
            Err(e) => failed.push((format!("{}\\{}", path, name), e.to_string())),
        }
    }

    {
        let path = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";
        let name = "InstallDate";
        match hklm.open_subkey_with_flags(path, KEY_ALL_ACCESS) {
            Ok(key) => {
                if let Ok(old_value) = key.get_value::<u32, _>(name) {
                    backup_entries.push(RegistryBackupEntry {
                        path: path.to_string(),
                        value_name: name.to_string(),
                        original_value: old_value.to_string(),
                    });
                }
                match key.set_value(name, &(profile.os.install_date as u32)) {
                    Ok(_) => applied.push(format!("{}\\{}", path, name)),
                    Err(e) => failed.push((format!("{}\\{}", path, name), e.to_string())),
                }
            }
            Err(e) => failed.push((format!("{}\\{}", path, name), e.to_string())),
        }
    }

    if !backup_entries.is_empty() {
        let backup = RegistryBackup {
            entries: backup_entries,
            created_at: crate::profile::engine::current_timestamp(),
        };
        if let Err(e) = save_backup(&backup) {
            failed.push(("backup".into(), format!("Failed to save backup: {}", e)));
        }
    }

    ApplyResult {
        applied,
        failed,
        skipped: Vec::new(),
    }
}

#[cfg(windows)]
fn revert_registry_windows(backup: &RegistryBackup) -> ApplyResult {
    use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_ALL_ACCESS};
    use winreg::RegKey;

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let mut applied = Vec::new();
    let mut failed = Vec::new();

    for entry in &backup.entries {
        match hklm.open_subkey_with_flags(&entry.path, KEY_ALL_ACCESS) {
            Ok(key) => match key.set_value(&entry.value_name, &entry.original_value) {
                Ok(_) => applied.push(format!("{}\\{}", entry.path, entry.value_name)),
                Err(e) => failed.push((
                    format!("{}\\{}", entry.path, entry.value_name),
                    e.to_string(),
                )),
            },
            Err(e) => failed.push((
                format!("{}\\{}", entry.path, entry.value_name),
                e.to_string(),
            )),
        }
    }

    ApplyResult {
        applied,
        failed,
        skipped: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ComputerName spoofing at Layer 2 breaks reboot and WMI (rc1
    /// dogfood Sev-2). It must never reappear in the applied set without
    /// a full rename implementation.
    #[test]
    fn registry_targets_exclude_computer_name() {
        for (path, name, _) in STRING_TARGETS {
            assert_ne!(
                *name, "ComputerName",
                "ComputerName must not be a Layer-2 spoof target (breaks reboot/WMI)"
            );
            assert!(
                !path.contains("ComputerName"),
                "no target may write under the ComputerName key: {}",
                path
            );
        }
    }

    /// The safe identifiers must stay present, so the table can't be
    /// silently emptied.
    #[test]
    fn registry_targets_cover_core_identifiers() {
        let names: Vec<&str> = STRING_TARGETS.iter().map(|(_, n, _)| *n).collect();
        for expected in ["MachineGuid", "HwProfileGuid", "MachineId", "ProductId"] {
            assert!(names.contains(&expected), "missing target: {}", expected);
        }
    }

    /// The registry backup must live in the SAME machine-wide store as
    /// everything else. If it drifts to a per-user base again, the
    /// LocalSystem uninstall cleanup can't read it and the original
    /// identity is never restored (rc1 dogfood Bug A).
    #[test]
    fn backup_path_is_inside_data_dir() {
        let expected = crate::profile::data_dir().join("backup.json");
        assert_eq!(backup_path(), expected);
        assert_eq!(
            backup_path().parent(),
            Some(crate::profile::data_dir().as_path())
        );
    }
}
