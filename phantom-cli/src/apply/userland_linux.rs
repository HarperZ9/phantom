//! Linux userland Layer 2 (Phase 1: machine-id).
//!
//! On Linux the analog of the Windows `MachineGuid` is the systemd machine ID
//! at `/etc/machine-id` (with a D-Bus copy at `/var/lib/dbus/machine-id`). It is
//! a stable per-install identifier that ordinary software reads for
//! fingerprinting, and it is a plain file, so Phantom backs it up and restores
//! it exactly, the same guarantee the Windows backend gives. hostname and MAC
//! arrive in a later increment.

#[cfg(target_os = "linux")]
use super::registry::{
    load_backup, merge_preserving_originals, save_backup, ApplyResult, BackupValueType,
    RegistryBackup, RegistryBackupEntry,
};
#[cfg(target_os = "linux")]
use crate::profile::schema::HardwareProfile;

/// The files that hold the machine ID. Only the ones that already exist are
/// touched, so a system without the D-Bus copy is handled cleanly.
#[cfg(target_os = "linux")]
const MACHINE_ID_PATHS: &[&str] = &["/etc/machine-id", "/var/lib/dbus/machine-id"];

/// Derive a Linux machine-id from a profile's `machine_guid`.
///
/// `/etc/machine-id` is 32 lowercase hex characters with no dashes. A Windows
/// GUID is that same 32 hex once its dashes are stripped, so the generated
/// identity carries straight across. Returns `None` when the value is not 32 hex
/// after stripping dashes, so a malformed profile never writes a bad ID.
pub(crate) fn derive_machine_id(machine_guid: &str) -> Option<String> {
    let hex: String = machine_guid.chars().filter(|c| *c != '-').collect();
    let hex = hex.to_ascii_lowercase();
    if hex.len() == 32 && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        Some(hex)
    } else {
        None
    }
}

/// Apply the profile's machine ID to every machine-id file that exists. Each
/// original is backed up first, the backup is written before any file changes
/// (so a crash mid-apply still leaves a recoverable backup), and the true
/// original is preserved across a re-apply. This mirrors the Windows backend.
#[cfg(target_os = "linux")]
pub(crate) fn apply_linux(profile: &HardwareProfile) -> ApplyResult {
    use std::collections::BTreeSet;

    let mut failed: Vec<(String, String)> = Vec::new();

    let new_id = match derive_machine_id(&profile.os.machine_guid) {
        Some(id) => id,
        None => {
            failed.push((
                "machine-id".into(),
                "profile machine_guid is not 32 hex".into(),
            ));
            return ApplyResult {
                applied: Vec::new(),
                failed,
                skipped: Vec::new(),
            };
        }
    };
    // A machine-id file is the 32 hex characters followed by a single newline.
    let content = format!("{}\n", new_id);

    let existing = load_backup().map(|b| b.entries).unwrap_or_default();
    let already: BTreeSet<(String, String)> = existing
        .iter()
        .map(|e| (e.path.clone(), e.value_name.clone()))
        .collect();

    let mut captured: Vec<RegistryBackupEntry> = Vec::new();
    let mut writes: Vec<String> = Vec::new();
    for path in MACHINE_ID_PATHS {
        let path = (*path).to_string();
        if !std::path::Path::new(&path).exists() {
            continue;
        }
        if already.contains(&(path.clone(), String::new())) {
            writes.push(path);
            continue;
        }
        match std::fs::read_to_string(&path) {
            Ok(original_value) => {
                captured.push(RegistryBackupEntry {
                    path: path.clone(),
                    value_name: String::new(),
                    original_value,
                    value_type: BackupValueType::Sz,
                });
                writes.push(path);
            }
            Err(e) => failed.push((path, format!("read failed: {}", e))),
        }
    }

    // Persist the backup before touching any file.
    let merged = merge_preserving_originals(existing, captured);
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

    let mut applied = Vec::new();
    for path in writes {
        match std::fs::write(&path, content.as_bytes()) {
            Ok(()) => applied.push(path),
            Err(e) => failed.push((path, format!("write failed: {}", e))),
        }
    }

    ApplyResult {
        applied,
        failed,
        skipped: Vec::new(),
    }
}

/// Restore each backed-up file to its original contents.
#[cfg(target_os = "linux")]
pub(crate) fn revert_linux(backup: &RegistryBackup) -> ApplyResult {
    let mut applied = Vec::new();
    let mut failed = Vec::new();
    for entry in &backup.entries {
        match std::fs::write(&entry.path, entry.original_value.as_bytes()) {
            Ok(()) => applied.push(entry.path.clone()),
            Err(e) => failed.push((entry.path.clone(), format!("write failed: {}", e))),
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
    use super::derive_machine_id;

    #[test]
    fn derive_machine_id_strips_dashes_and_lowercases() {
        assert_eq!(
            derive_machine_id("BADC0FFE-1234-5678-9ABC-DEF012345678"),
            Some("badc0ffe123456789abcdef012345678".to_string())
        );
    }

    #[test]
    fn derive_machine_id_accepts_bare_32_hex() {
        assert_eq!(
            derive_machine_id("badc0ffe123456789abcdef012345678"),
            Some("badc0ffe123456789abcdef012345678".to_string())
        );
    }

    #[test]
    fn derive_machine_id_rejects_non_hex_and_wrong_length() {
        assert_eq!(derive_machine_id(""), None);
        assert_eq!(derive_machine_id("not-a-guid"), None);
        // 31 hex, one short
        assert_eq!(derive_machine_id("badc0ffe123456789abcdef01234567"), None);
        // 32 characters but one is not hex
        assert_eq!(derive_machine_id("badc0ffe123456789abcdef01234567g"), None);
    }
}
