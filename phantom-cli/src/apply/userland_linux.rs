//! Linux userland Layer 2 (Phase 1: machine-id and hostname).
//!
//! On Linux the analog of the Windows `MachineGuid` is the systemd machine ID
//! at `/etc/machine-id` (with a D-Bus copy at `/var/lib/dbus/machine-id`), and
//! the analog of `ComputerName` is the hostname at `/etc/hostname`. Both are
//! plain files, so Phantom backs them up and restores them exactly, the same
//! guarantee the Windows backend gives. The hostname is also set live via
//! `sethostname(2)` so the change shows without a reboot. MAC arrives next.

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

/// The file that holds the configured hostname.
#[cfg(target_os = "linux")]
const HOSTNAME_PATH: &str = "/etc/hostname";

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

/// Validate a hostname derived from the profile's `computer_name`.
///
/// A hostname set via `sethostname` is a short name of ASCII letters, digits,
/// and hyphens, at most 64 bytes, not starting or ending with a hyphen. The
/// generated `computer_name` already fits, and the check rejects anything that
/// does not, so a malformed profile never sets a bad name.
pub(crate) fn derive_hostname(computer_name: &str) -> Option<String> {
    let name = computer_name.trim();
    if name.is_empty() || name.len() > 64 {
        return None;
    }
    if !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
        return None;
    }
    if name.starts_with('-') || name.ends_with('-') {
        return None;
    }
    Some(name.to_string())
}

/// Set the running system hostname via `sethostname(2)`, so the change takes
/// effect without a reboot. Needs root (CAP_SYS_ADMIN).
#[cfg(target_os = "linux")]
fn set_live_hostname(name: &str) -> std::io::Result<()> {
    let bytes = name.as_bytes();
    // SAFETY: sethostname reads `len` bytes from `ptr`. Both come from a live
    // slice that outlives the call, and derive_hostname bounds the length well
    // under the kernel's HOST_NAME_MAX.
    let rc = unsafe { libc::sethostname(bytes.as_ptr() as *const libc::c_char, bytes.len()) };
    if rc == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

/// Apply the profile's machine ID and hostname. Each original is backed up
/// first, the backup is written before any file changes (so a crash mid-apply
/// still leaves a recoverable backup), and the true original is preserved across
/// a re-apply. This mirrors the Windows backend.
#[cfg(target_os = "linux")]
pub(crate) fn apply_linux(profile: &HardwareProfile) -> ApplyResult {
    use std::collections::BTreeSet;

    let mut failed: Vec<(String, String)> = Vec::new();

    // Build every (file path, new content) target. A field that does not derive
    // to a valid value is recorded as a failure and skipped, so the rest still
    // apply.
    let mut targets: Vec<(String, String)> = Vec::new();

    match derive_machine_id(&profile.os.machine_guid) {
        Some(id) => {
            let content = format!("{}\n", id);
            for path in MACHINE_ID_PATHS {
                if std::path::Path::new(path).exists() {
                    targets.push(((*path).to_string(), content.clone()));
                }
            }
        }
        None => failed.push((
            "machine-id".into(),
            "profile machine_guid is not 32 hex".into(),
        )),
    }

    let live_hostname = match derive_hostname(&profile.os.computer_name) {
        Some(name) => {
            targets.push((HOSTNAME_PATH.to_string(), format!("{}\n", name)));
            Some(name)
        }
        None => {
            failed.push((
                "hostname".into(),
                "profile computer_name is not a valid hostname".into(),
            ));
            None
        }
    };

    // Capture the original of every target not backed up before, then persist
    // the backup before changing any file. A target we cannot read is not
    // written, so nothing changes that could not be reverted.
    let existing = load_backup().map(|b| b.entries).unwrap_or_default();
    let already: BTreeSet<(String, String)> = existing
        .iter()
        .map(|e| (e.path.clone(), e.value_name.clone()))
        .collect();

    let mut captured: Vec<RegistryBackupEntry> = Vec::new();
    let mut writes: Vec<(String, String)> = Vec::new();
    for (path, content) in targets {
        if already.contains(&(path.clone(), String::new())) {
            writes.push((path, content));
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
                writes.push((path, content));
            }
            Err(e) => failed.push((path, format!("read failed: {}", e))),
        }
    }

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
    let mut wrote_hostname_file = false;
    for (path, content) in writes {
        match std::fs::write(&path, content.as_bytes()) {
            Ok(()) => {
                if path == HOSTNAME_PATH {
                    wrote_hostname_file = true;
                }
                applied.push(path);
            }
            Err(e) => failed.push((path, format!("write failed: {}", e))),
        }
    }

    // Set the hostname live, but only once its file (and therefore its backup)
    // is in place, so revert can always undo it.
    if wrote_hostname_file {
        if let Some(name) = live_hostname {
            match set_live_hostname(&name) {
                Ok(()) => applied.push("hostname (live)".into()),
                Err(e) => failed.push((
                    "hostname (live)".into(),
                    format!("sethostname failed: {}", e),
                )),
            }
        }
    }

    ApplyResult {
        applied,
        failed,
        skipped: Vec::new(),
    }
}

/// Restore each backed-up file to its original contents, and put the live
/// hostname back to match the restored `/etc/hostname`.
#[cfg(target_os = "linux")]
pub(crate) fn revert_linux(backup: &RegistryBackup) -> ApplyResult {
    let mut applied = Vec::new();
    let mut failed = Vec::new();
    for entry in &backup.entries {
        match std::fs::write(&entry.path, entry.original_value.as_bytes()) {
            Ok(()) => {
                if entry.path == HOSTNAME_PATH {
                    if let Err(e) = set_live_hostname(entry.original_value.trim()) {
                        failed.push((
                            "hostname (live)".into(),
                            format!("sethostname failed: {}", e),
                        ));
                    }
                }
                applied.push(entry.path.clone());
            }
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
    use super::{derive_hostname, derive_machine_id};

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

    #[test]
    fn derive_hostname_accepts_a_normal_name() {
        assert_eq!(
            derive_hostname("DESKTOP-A1B2C3"),
            Some("DESKTOP-A1B2C3".to_string())
        );
        assert_eq!(derive_hostname("  host-01  "), Some("host-01".to_string()));
    }

    #[test]
    fn derive_hostname_rejects_bad_names() {
        assert_eq!(derive_hostname(""), None);
        assert_eq!(derive_hostname("-leading"), None);
        assert_eq!(derive_hostname("trailing-"), None);
        assert_eq!(derive_hostname("has space"), None);
        assert_eq!(derive_hostname("under_score"), None);
        assert_eq!(derive_hostname(&"a".repeat(65)), None);
    }
}
