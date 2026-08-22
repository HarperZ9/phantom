//! Linux userland Layer 2 (Phase 1: machine-id, hostname, MAC).
//!
//! On Linux the analog of the Windows `MachineGuid` is the systemd machine ID
//! at `/etc/machine-id` (with a D-Bus copy at `/var/lib/dbus/machine-id`), the
//! analog of `ComputerName` is the hostname at `/etc/hostname`, and each network
//! adapter has a MAC. All are backed up before the first change and restored
//! exactly on revert, the same guarantee the Windows backend gives. The machine
//! ID and hostname are files; the hostname is also set live via `sethostname(2)`,
//! and the MAC is set through iproute2 so both take effect without a reboot.

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

/// Backup entries for a MAC use this prefix plus the interface name, so revert
/// can tell them apart from file entries.
#[cfg(target_os = "linux")]
const MAC_BACKUP_PREFIX: &str = "mac:";

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

/// Validate a MAC address and normalize it to lowercase colon form.
pub(crate) fn normalize_mac(mac: &str) -> Option<String> {
    let parts: Vec<&str> = mac.split(':').collect();
    if parts.len() != 6 {
        return None;
    }
    let mut out = String::with_capacity(17);
    for (i, part) in parts.iter().enumerate() {
        if part.len() != 2 || !part.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        if i > 0 {
            out.push(':');
        }
        out.push_str(&part.to_ascii_lowercase());
    }
    Some(out)
}

/// Pair each physical interface with a spoofed MAC from the profile, by index.
/// `zip` stops at the shorter list, so interfaces past the profile's adapter
/// count are left alone; a malformed MAC is dropped. No two interfaces ever get
/// the same MAC.
pub(crate) fn plan_mac_assignments(
    interfaces: &[String],
    adapter_macs: &[String],
) -> Vec<(String, String)> {
    interfaces
        .iter()
        .zip(adapter_macs.iter())
        .filter_map(|(iface, mac)| normalize_mac(mac).map(|m| (iface.clone(), m)))
        .collect()
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

/// The physical network interfaces, sorted. A physical NIC has a `device`
/// symlink under `/sys/class/net`; loopback and virtual interfaces (veth,
/// docker, bridges) do not, so they are skipped.
#[cfg(target_os = "linux")]
pub(crate) fn physical_interfaces() -> Vec<String> {
    let mut ifaces = Vec::new();
    if let Ok(entries) = std::fs::read_dir("/sys/class/net") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name == "lo" {
                continue;
            }
            if std::path::Path::new(&format!("/sys/class/net/{}/device", name)).exists() {
                ifaces.push(name);
            }
        }
    }
    ifaces.sort();
    ifaces
}

/// Read an interface's current MAC from `/sys`, normalized.
#[cfg(target_os = "linux")]
fn read_current_mac(iface: &str) -> Option<String> {
    std::fs::read_to_string(format!("/sys/class/net/{}/address", iface))
        .ok()
        .and_then(|s| normalize_mac(s.trim()))
}

/// Set an interface's MAC through iproute2. Some drivers reject the change while
/// the link is up, so this brings it down, sets the address, and brings it up,
/// which briefly drops the link and any connection running over it.
#[cfg(target_os = "linux")]
fn set_mac(iface: &str, mac: &str) -> std::io::Result<()> {
    use std::process::Command;
    let run = |args: &[&str]| -> std::io::Result<()> {
        let status = Command::new("ip").args(args).status()?;
        if status.success() {
            Ok(())
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("ip {} exited with {}", args.join(" "), status),
            ))
        }
    };
    run(&["link", "set", "dev", iface, "down"])?;
    let set = run(&["link", "set", "dev", iface, "address", mac]);
    // Always try to bring the link back up, even if the address set failed.
    let up = run(&["link", "set", "dev", iface, "up"]);
    set.and(up)
}

/// Apply the profile's machine ID, hostname, and MACs. Every original is backed
/// up first, the backup is written before any change (so a crash mid-apply still
/// leaves a recoverable backup), and the true original is preserved across a
/// re-apply. This mirrors the Windows backend.
#[cfg(target_os = "linux")]
pub(crate) fn apply_linux(profile: &HardwareProfile) -> ApplyResult {
    use std::collections::BTreeSet;

    let mut failed: Vec<(String, String)> = Vec::new();

    // File targets: machine-id and hostname.
    let mut file_targets: Vec<(String, String)> = Vec::new();
    match derive_machine_id(&profile.os.machine_guid) {
        Some(id) => {
            let content = format!("{}\n", id);
            for path in MACHINE_ID_PATHS {
                if std::path::Path::new(path).exists() {
                    file_targets.push(((*path).to_string(), content.clone()));
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
            file_targets.push((HOSTNAME_PATH.to_string(), format!("{}\n", name)));
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

    // MAC targets: each physical interface paired with a profile adapter's MAC.
    let adapter_macs: Vec<String> = profile
        .network_adapters
        .iter()
        .map(|a| a.current_mac.clone())
        .collect();
    let mac_targets = plan_mac_assignments(&physical_interfaces(), &adapter_macs);

    // Capture the original of every target not backed up before, then persist
    // the backup before changing anything. A target we cannot read is not
    // changed, so nothing happens that could not be reverted.
    let existing = load_backup().map(|b| b.entries).unwrap_or_default();
    let already: BTreeSet<(String, String)> = existing
        .iter()
        .map(|e| (e.path.clone(), e.value_name.clone()))
        .collect();

    let mut captured: Vec<RegistryBackupEntry> = Vec::new();
    let mut file_writes: Vec<(String, String)> = Vec::new();
    for (path, content) in file_targets {
        if already.contains(&(path.clone(), String::new())) {
            file_writes.push((path, content));
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
                file_writes.push((path, content));
            }
            Err(e) => failed.push((path, format!("read failed: {}", e))),
        }
    }
    let mut mac_writes: Vec<(String, String)> = Vec::new();
    for (iface, new_mac) in mac_targets {
        let key = format!("{}{}", MAC_BACKUP_PREFIX, iface);
        if already.contains(&(key.clone(), String::new())) {
            mac_writes.push((iface, new_mac));
            continue;
        }
        match read_current_mac(&iface) {
            Some(original_value) => {
                captured.push(RegistryBackupEntry {
                    path: key,
                    value_name: String::new(),
                    original_value,
                    value_type: BackupValueType::Sz,
                });
                mac_writes.push((iface, new_mac));
            }
            None => failed.push((
                format!("mac:{}", iface),
                "could not read current MAC".into(),
            )),
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
    for (path, content) in file_writes {
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
    for (iface, new_mac) in mac_writes {
        match set_mac(&iface, &new_mac) {
            Ok(()) => applied.push(format!("mac:{}", iface)),
            Err(e) => failed.push((format!("mac:{}", iface), format!("set MAC failed: {}", e))),
        }
    }

    ApplyResult {
        applied,
        failed,
        skipped: Vec::new(),
    }
}

/// Restore everything from the backup: file contents (and the live hostname to
/// match) and each interface's original MAC.
#[cfg(target_os = "linux")]
pub(crate) fn revert_linux(backup: &RegistryBackup) -> ApplyResult {
    let mut applied = Vec::new();
    let mut failed = Vec::new();
    for entry in &backup.entries {
        if let Some(iface) = entry.path.strip_prefix(MAC_BACKUP_PREFIX) {
            match set_mac(iface, entry.original_value.trim()) {
                Ok(()) => applied.push(entry.path.clone()),
                Err(e) => failed.push((entry.path.clone(), format!("set MAC failed: {}", e))),
            }
            continue;
        }
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
    use super::{derive_hostname, derive_machine_id, normalize_mac, plan_mac_assignments};

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

    #[test]
    fn normalize_mac_accepts_and_lowercases() {
        assert_eq!(
            normalize_mac("AA:BB:CC:DD:EE:FF"),
            Some("aa:bb:cc:dd:ee:ff".to_string())
        );
        assert_eq!(
            normalize_mac("00:1a:2b:3c:4d:5e"),
            Some("00:1a:2b:3c:4d:5e".to_string())
        );
    }

    #[test]
    fn normalize_mac_rejects_malformed() {
        assert_eq!(normalize_mac(""), None);
        assert_eq!(normalize_mac("AA:BB:CC:DD:EE"), None); // 5 parts
        assert_eq!(normalize_mac("AA:BB:CC:DD:EE:FF:00"), None); // 7 parts
        assert_eq!(normalize_mac("AA-BB-CC-DD-EE-FF"), None); // wrong separator
        assert_eq!(normalize_mac("AA:BB:CC:DD:EE:GG"), None); // non-hex
        assert_eq!(normalize_mac("A:BB:CC:DD:EE:FF"), None); // one-digit group
    }

    #[test]
    fn plan_mac_assignments_pairs_by_index_and_drops_invalid() {
        let ifaces = vec!["eth0".to_string(), "eth1".to_string(), "eth2".to_string()];
        // eth1's MAC is malformed and dropped; eth2 has no adapter and is left alone.
        let macs = vec!["AA:BB:CC:DD:EE:01".to_string(), "bad".to_string()];
        let plan = plan_mac_assignments(&ifaces, &macs);
        assert_eq!(
            plan,
            vec![("eth0".to_string(), "aa:bb:cc:dd:ee:01".to_string())]
        );
    }
}
