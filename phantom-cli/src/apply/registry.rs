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
    #[cfg(target_os = "linux")]
    {
        crate::apply::userland_linux::apply_linux(profile)
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = profile;
        ApplyResult {
            applied: Vec::new(),
            failed: vec![(
                "userland".into(),
                "Layer-2 spoofing supports Windows and Linux".into(),
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
    #[cfg(target_os = "linux")]
    {
        crate::apply::userland_linux::revert_linux(backup)
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = backup;
        ApplyResult {
            applied: Vec::new(),
            failed: vec![("userland".into(), "Layer-2 revert supports Windows and Linux".into())],
            skipped: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RegistryBackup {
    pub entries: Vec<RegistryBackupEntry>,
    pub created_at: String,
}

/// The registry type of a backed-up value. Restoring a value with the wrong
/// type corrupts it: `InstallDate` is a `REG_DWORD`, and writing its number
/// back as a `REG_SZ` string leaves software that reads it as a DWORD looking
/// at the wrong type. The tag rides in the backup so revert restores each
/// value as what it was.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackupValueType {
    /// `REG_SZ` string value (MachineGuid, HwProfileGuid, MachineId, ProductId).
    #[default]
    Sz,
    /// `REG_DWORD` 32-bit value (InstallDate).
    Dword,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RegistryBackupEntry {
    pub path: String,
    pub value_name: String,
    pub original_value: String,
    /// Absent in backups written before this field existed; those all held
    /// string values, so the default `Sz` restores them exactly as before.
    #[serde(default)]
    pub value_type: BackupValueType,
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

/// Merge freshly captured originals onto an existing backup, letting the
/// existing entry win for any key already recorded.
///
/// The first `apply` records the true originals, before anything is spoofed.
/// A later `apply` (or the service re-applying the active profile on boot)
/// only ever sees the already-spoofed values in the live registry. Capturing
/// those as "originals" would overwrite the real identity in the backup, and
/// then revert or uninstall would restore a spoof instead of the machine's
/// own identity. Keeping the existing entry means the true original, captured
/// once, is never lost to a second apply.
pub(crate) fn merge_preserving_originals(
    existing: Vec<RegistryBackupEntry>,
    captured: Vec<RegistryBackupEntry>,
) -> Vec<RegistryBackupEntry> {
    use std::collections::BTreeMap;
    let mut by_key: BTreeMap<(String, String), RegistryBackupEntry> = BTreeMap::new();
    for entry in existing {
        by_key.insert((entry.path.clone(), entry.value_name.clone()), entry);
    }
    for entry in captured {
        by_key
            .entry((entry.path.clone(), entry.value_name.clone()))
            .or_insert(entry);
    }
    by_key.into_values().collect()
}

#[cfg(windows)]
fn apply_registry_windows(profile: &HardwareProfile) -> ApplyResult {
    use std::collections::BTreeSet;
    use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_ALL_ACCESS};
    use winreg::RegKey;

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let mut failed: Vec<(String, String)> = Vec::new();

    // Every Layer-2 target: the four REG_SZ identifiers plus the one REG_DWORD,
    // each paired with the value to write.
    let mut targets: Vec<(String, String, BackupValueType, String)> =
        Vec::with_capacity(STRING_TARGETS.len() + 1);
    for (path, name, field) in STRING_TARGETS {
        targets.push((
            (*path).to_string(),
            (*name).to_string(),
            BackupValueType::Sz,
            field(profile),
        ));
    }
    targets.push((
        r"SOFTWARE\Microsoft\Windows NT\CurrentVersion".to_string(),
        "InstallDate".to_string(),
        BackupValueType::Dword,
        (profile.os.install_date as u32).to_string(),
    ));

    // Keys an earlier apply already recorded. Their stored originals are the
    // true ones, so do not re-read them: the live value is now a spoof.
    let existing_entries = load_backup().map(|b| b.entries).unwrap_or_default();
    let already_backed_up: BTreeSet<(String, String)> = existing_entries
        .iter()
        .map(|e| (e.path.clone(), e.value_name.clone()))
        .collect();

    // Phase 1: capture the original of every key not seen before, and decide
    // what we can safely write. A key we cannot open is not spoofed, because
    // we could not first make it revertible.
    let mut captured: Vec<RegistryBackupEntry> = Vec::new();
    let mut writes: Vec<(String, String, BackupValueType, String)> = Vec::new();
    for (path, name, vtype, new_value) in targets {
        if already_backed_up.contains(&(path.clone(), name.clone())) {
            writes.push((path, name, vtype, new_value));
            continue;
        }
        match hklm.open_subkey_with_flags(&path, KEY_ALL_ACCESS) {
            Ok(key) => {
                let original = match vtype {
                    BackupValueType::Sz => key.get_value::<String, _>(&name).ok(),
                    BackupValueType::Dword => {
                        key.get_value::<u32, _>(&name).ok().map(|v| v.to_string())
                    }
                };
                if let Some(original_value) = original {
                    captured.push(RegistryBackupEntry {
                        path: path.clone(),
                        value_name: name.clone(),
                        original_value,
                        value_type: vtype,
                    });
                }
                writes.push((path, name, vtype, new_value));
            }
            Err(e) => failed.push((format!("{}\\{}", path, name), e.to_string())),
        }
    }

    // Phase 2: persist the backup BEFORE changing any registry value, so a
    // crash mid-apply still leaves a recoverable backup on disk. If the backup
    // will not save, write nothing.
    let merged = merge_preserving_originals(existing_entries, captured);
    if !merged.is_empty() {
        let backup = RegistryBackup {
            entries: merged,
            created_at: crate::profile::engine::current_timestamp(),
        };
        if let Err(e) = save_backup(&backup) {
            failed.push((
                "backup".into(),
                format!("Failed to save backup, no changes written: {}", e),
            ));
            return ApplyResult {
                applied: Vec::new(),
                failed,
                skipped: Vec::new(),
            };
        }
    }

    // Phase 3: write each spoofed value as its real registry type.
    let mut applied = Vec::new();
    for (path, name, vtype, new_value) in writes {
        let label = format!("{}\\{}", path, name);
        match hklm.open_subkey_with_flags(&path, KEY_ALL_ACCESS) {
            Ok(key) => {
                let result = match vtype {
                    BackupValueType::Sz => key.set_value(&name, &new_value),
                    BackupValueType::Dword => match new_value.parse::<u32>() {
                        Ok(n) => key.set_value(&name, &n),
                        Err(e) => {
                            failed.push((label, format!("invalid DWORD value: {}", e)));
                            continue;
                        }
                    },
                };
                match result {
                    Ok(_) => applied.push(label),
                    Err(e) => failed.push((label, e.to_string())),
                }
            }
            Err(e) => failed.push((label, e.to_string())),
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
        let label = format!("{}\\{}", entry.path, entry.value_name);
        match hklm.open_subkey_with_flags(&entry.path, KEY_ALL_ACCESS) {
            Ok(key) => {
                let result = match entry.value_type {
                    BackupValueType::Sz => key.set_value(&entry.value_name, &entry.original_value),
                    BackupValueType::Dword => match entry.original_value.parse::<u32>() {
                        Ok(n) => key.set_value(&entry.value_name, &n),
                        Err(e) => {
                            failed.push((label, format!("corrupt DWORD in backup: {}", e)));
                            continue;
                        }
                    },
                };
                match result {
                    Ok(_) => applied.push(label),
                    Err(e) => failed.push((label, e.to_string())),
                }
            }
            Err(e) => failed.push((label, e.to_string())),
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

    fn entry(name: &str, original: &str, vtype: BackupValueType) -> RegistryBackupEntry {
        RegistryBackupEntry {
            path: format!(r"SOFTWARE\Test\{}", name),
            value_name: name.to_string(),
            original_value: original.to_string(),
            value_type: vtype,
        }
    }

    /// The core reversibility guarantee: applying a second profile without
    /// reverting first (or the service re-applying on boot) must NOT overwrite
    /// the true original captured by the first apply. Otherwise revert and
    /// uninstall restore a spoof, and the machine's real identity is lost.
    #[test]
    fn merge_preserving_originals_keeps_the_true_original() {
        let existing = vec![entry("MachineGuid", "REAL-ORIGINAL", BackupValueType::Sz)];
        let captured = vec![entry("MachineGuid", "SPOOFED-A", BackupValueType::Sz)];
        let merged = merge_preserving_originals(existing, captured);
        assert_eq!(merged.len(), 1);
        assert_eq!(
            merged[0].original_value, "REAL-ORIGINAL",
            "a second apply must never overwrite the true original in the backup"
        );
    }

    #[test]
    fn merge_preserving_originals_adds_new_keys_and_unions() {
        let existing = vec![entry("MachineGuid", "REAL-GUID", BackupValueType::Sz)];
        let captured = vec![
            entry("MachineGuid", "SPOOF", BackupValueType::Sz),
            entry("ProductId", "REAL-PID", BackupValueType::Sz),
        ];
        let merged = merge_preserving_originals(existing, captured);
        let by_name: std::collections::BTreeMap<&str, &str> = merged
            .iter()
            .map(|e| (e.value_name.as_str(), e.original_value.as_str()))
            .collect();
        assert_eq!(merged.len(), 2);
        assert_eq!(by_name.get("MachineGuid"), Some(&"REAL-GUID"));
        assert_eq!(by_name.get("ProductId"), Some(&"REAL-PID"));
    }

    /// A backup written before the `value_type` field existed must still load,
    /// and its string values must restore as REG_SZ (the old behavior).
    #[test]
    fn old_backup_without_value_type_defaults_to_sz() {
        let json = r#"{"entries":[{"path":"SOFTWARE\\Microsoft\\Cryptography","value_name":"MachineGuid","original_value":"abc"}],"created_at":"2026-01-01T00:00:00Z"}"#;
        let backup: RegistryBackup = serde_json::from_str(json).unwrap();
        assert_eq!(backup.entries.len(), 1);
        assert_eq!(backup.entries[0].value_type, BackupValueType::Sz);
    }

    /// InstallDate is a REG_DWORD. Its backup entry must round-trip as a DWORD
    /// so revert restores the right type instead of a REG_SZ string.
    #[test]
    fn dword_backup_entry_round_trips_as_dword() {
        let e = entry("InstallDate", "1609459200", BackupValueType::Dword);
        let json = serde_json::to_string(&e).unwrap();
        let back: RegistryBackupEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back.value_type, BackupValueType::Dword);
        assert_eq!(back.original_value.parse::<u32>().unwrap(), 1_609_459_200);
    }
}
