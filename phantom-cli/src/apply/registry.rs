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

pub fn backup_path() -> std::path::PathBuf {
    let base = if cfg!(windows) {
        std::env::var("APPDATA")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
    } else {
        std::env::var("HOME")
            .map(|h| std::path::PathBuf::from(h).join(".config"))
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
    };
    base.join("phantom").join("backup.json")
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

#[cfg(windows)]
fn apply_registry_windows(profile: &HardwareProfile) -> ApplyResult {
    use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_ALL_ACCESS};
    use winreg::RegKey;

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let mut applied = Vec::new();
    let mut failed = Vec::new();
    let mut backup_entries = Vec::new();

    let string_entries = [
        (
            r"SOFTWARE\Microsoft\Cryptography",
            "MachineGuid",
            &profile.os.machine_guid,
        ),
        (
            r"SYSTEM\CurrentControlSet\Control\IDConfigDB\Hardware Profiles\0001",
            "HwProfileGuid",
            &profile.os.hw_profile_guid,
        ),
        (
            r"SOFTWARE\Microsoft\SQMClient",
            "MachineId",
            &profile.os.machine_id,
        ),
        (
            r"SOFTWARE\Microsoft\Windows NT\CurrentVersion",
            "ProductId",
            &profile.os.product_id,
        ),
        (
            r"SYSTEM\CurrentControlSet\Control\ComputerName\ComputerName",
            "ComputerName",
            &profile.os.computer_name,
        ),
        (
            r"SYSTEM\CurrentControlSet\Control\ComputerName\ActiveComputerName",
            "ComputerName",
            &profile.os.computer_name,
        ),
    ];

    for (path, name, new_value) in &string_entries {
        match hklm.open_subkey_with_flags(path, KEY_ALL_ACCESS) {
            Ok(key) => {
                if let Ok(old_value) = key.get_value::<String, _>(name) {
                    backup_entries.push(RegistryBackupEntry {
                        path: path.to_string(),
                        value_name: name.to_string(),
                        original_value: old_value,
                    });
                }
                match key.set_value(name, new_value) {
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
