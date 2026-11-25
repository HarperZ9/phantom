//! Mock-apply / restore roundtrip and planted-failure integration tests.
//!
//! Exercises the Layer-2 3-phase apply protocol (capture -> persist backup ->
//! write), typed revert (`REG_SZ` and `REG_DWORD`), double-apply original
//! preservation, `ActiveConfig` disk persistence, and `driver_ipc` binary
//! serialization error paths without touching host system state.

use phantom_cli::apply::driver_ipc::serialize_kernel_profile;
use phantom_cli::apply::registry::{
    apply_registry_with_store, load_backup_from, revert_registry_with_store, save_backup_to,
    BackupValueType, RegistryBackup, RegistryBackupEntry, RegistryStore,
};
use phantom_cli::apply::{parse_layers, ActiveConfig, Layer};
use phantom_cli::profile::engine::generate_profile;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_ID: AtomicUsize = AtomicUsize::new(1);

fn unique_temp_dir(label: &str) -> PathBuf {
    let n = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "phantom-int-{}-{}-{}",
        label,
        std::process::id(),
        n
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp test dir");
    dir
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum MockValue {
    Sz(String),
    Dword(u32),
}

#[derive(Default)]
struct MockRegistry {
    values: BTreeMap<(String, String), MockValue>,
    fail_reads: BTreeSet<(String, String)>,
    fail_writes: BTreeSet<(String, String)>,
}

impl MockRegistry {
    fn baseline() -> Self {
        let mut reg = Self::default();
        reg.insert_sz(
            r"SOFTWARE\Microsoft\Cryptography",
            "MachineGuid",
            "11111111-2222-3333-4444-555555555555",
        );
        reg.insert_sz(
            r"SYSTEM\CurrentControlSet\Control\IDConfigDB\Hardware Profiles\0001",
            "HwProfileGuid",
            "{aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee}",
        );
        reg.insert_sz(
            r"SOFTWARE\Microsoft\SQMClient",
            "MachineId",
            "{99999999-8888-7777-6666-555555555555}",
        );
        reg.insert_sz(
            r"SOFTWARE\Microsoft\Windows NT\CurrentVersion",
            "ProductId",
            "00330-80000-00000-AA001",
        );
        reg.insert_dword(
            r"SOFTWARE\Microsoft\Windows NT\CurrentVersion",
            "InstallDate",
            1_700_000_000,
        );
        reg
    }

    fn insert_sz(&mut self, path: &str, name: &str, value: &str) {
        self.values.insert(
            (path.to_string(), name.to_string()),
            MockValue::Sz(value.to_string()),
        );
    }

    fn insert_dword(&mut self, path: &str, name: &str, value: u32) {
        self.values.insert(
            (path.to_string(), name.to_string()),
            MockValue::Dword(value),
        );
    }
}

impl RegistryStore for MockRegistry {
    fn read_sz(&self, path: &str, name: &str) -> Result<Option<String>, String> {
        let key = (path.to_string(), name.to_string());
        if self.fail_reads.contains(&key) {
            return Err("planted read access denied".into());
        }
        match self.values.get(&key) {
            Some(MockValue::Sz(s)) => Ok(Some(s.clone())),
            Some(MockValue::Dword(_)) => Err("type mismatch: expected REG_SZ".into()),
            None => Ok(None),
        }
    }

    fn read_dword(&self, path: &str, name: &str) -> Result<Option<u32>, String> {
        let key = (path.to_string(), name.to_string());
        if self.fail_reads.contains(&key) {
            return Err("planted read access denied".into());
        }
        match self.values.get(&key) {
            Some(MockValue::Dword(v)) => Ok(Some(*v)),
            Some(MockValue::Sz(_)) => Err("type mismatch: expected REG_DWORD".into()),
            None => Ok(None),
        }
    }

    fn write_sz(&mut self, path: &str, name: &str, value: &str) -> Result<(), String> {
        let key = (path.to_string(), name.to_string());
        if self.fail_writes.contains(&key) {
            return Err("planted write access denied".into());
        }
        self.values.insert(key, MockValue::Sz(value.to_string()));
        Ok(())
    }

    fn write_dword(&mut self, path: &str, name: &str, value: u32) -> Result<(), String> {
        let key = (path.to_string(), name.to_string());
        if self.fail_writes.contains(&key) {
            return Err("planted write access denied".into());
        }
        self.values.insert(key, MockValue::Dword(value));
        Ok(())
    }
}

#[test]
fn mock_apply_and_revert_roundtrip_restores_exact_originals_and_types() {
    let dir = unique_temp_dir("roundtrip");
    let backup_file = dir.join("backup.json");
    let mut store = MockRegistry::baseline();
    let original_snapshot = store.values.clone();

    let profile = generate_profile("roundtrip-seed-alpha", "alpha");
    let apply_res = apply_registry_with_store(&mut store, &profile, &backup_file);
    assert!(
        apply_res.success(),
        "apply failures: {:?}",
        apply_res.failed
    );
    assert_eq!(apply_res.applied.len(), 5);
    assert_ne!(store.values, original_snapshot);
    assert_eq!(
        store.values.get(&(
            r"SOFTWARE\Microsoft\Cryptography".to_string(),
            "MachineGuid".to_string()
        )),
        Some(&MockValue::Sz(profile.os.machine_guid.clone()))
    );
    assert_eq!(
        store.values.get(&(
            r"SOFTWARE\Microsoft\Windows NT\CurrentVersion".to_string(),
            "InstallDate".to_string()
        )),
        Some(&MockValue::Dword(profile.os.install_date as u32))
    );

    let backup = load_backup_from(&backup_file).expect("backup persisted");
    assert_eq!(backup.entries.len(), 5);

    let revert_res = revert_registry_with_store(&mut store, &backup);
    assert!(
        revert_res.success(),
        "revert failures: {:?}",
        revert_res.failed
    );
    assert_eq!(revert_res.applied.len(), 5);
    assert_eq!(store.values, original_snapshot);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn double_apply_without_revert_preserves_true_originals_in_backup() {
    let dir = unique_temp_dir("double-apply");
    let backup_file = dir.join("backup.json");
    let mut store = MockRegistry::baseline();
    let true_originals = store.values.clone();

    let profile_a = generate_profile("seed-first", "first");
    let profile_b = generate_profile("seed-second", "second");
    assert_ne!(profile_a.os.machine_guid, profile_b.os.machine_guid);

    let res_a = apply_registry_with_store(&mut store, &profile_a, &backup_file);
    assert!(res_a.success());

    let res_b = apply_registry_with_store(&mut store, &profile_b, &backup_file);
    assert!(res_b.success());
    assert_eq!(
        store.values.get(&(
            r"SOFTWARE\Microsoft\Cryptography".to_string(),
            "MachineGuid".to_string()
        )),
        Some(&MockValue::Sz(profile_b.os.machine_guid.clone()))
    );

    let backup = load_backup_from(&backup_file).expect("backup readable");
    let revert_res = revert_registry_with_store(&mut store, &backup);
    assert!(revert_res.success());
    assert_eq!(
        store.values, true_originals,
        "revert after double apply must restore original values, not first spoof"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn planted_backup_save_failure_aborts_before_any_store_mutation() {
    let dir = unique_temp_dir("backup-fail");
    // Plant a regular file where the backup's parent directory must be so
    // `save_backup_to` fails deterministically on every OS.
    let blocker = dir.join("not-a-directory");
    std::fs::write(&blocker, b"blocker").expect("create blocker file");
    let impossible_backup_path = blocker.join("backup.json");

    let mut store = MockRegistry::baseline();
    let original_snapshot = store.values.clone();
    let profile = generate_profile("seed-abort", "abort");

    let res = apply_registry_with_store(&mut store, &profile, &impossible_backup_path);
    assert!(!res.success());
    assert!(
        res.applied.is_empty(),
        "no keys may be written when backup persistence fails"
    );
    assert!(
        res.failed
            .iter()
            .any(|(k, msg)| k == "backup" && msg.contains("no changes written")),
        "expected backup abort failure, got: {:?}",
        res.failed
    );
    assert_eq!(
        store.values, original_snapshot,
        "store must remain untouched when backup fails"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn planted_partial_write_failure_leaves_backup_usable_for_retry_revert() {
    let dir = unique_temp_dir("partial-fail");
    let backup_file = dir.join("backup.json");
    let mut store = MockRegistry::baseline();
    let original_snapshot = store.values.clone();
    let profile = generate_profile("seed-partial", "partial");

    // Plant a write denial on ProductId during apply.
    let blocked_key = (
        r"SOFTWARE\Microsoft\Windows NT\CurrentVersion".to_string(),
        "ProductId".to_string(),
    );
    store.fail_writes.insert(blocked_key.clone());

    let apply_res = apply_registry_with_store(&mut store, &profile, &backup_file);
    assert!(!apply_res.success());
    assert_eq!(apply_res.applied.len(), 4);
    assert_eq!(apply_res.failed.len(), 1);
    assert!(apply_res.failed[0]
        .1
        .contains("planted write access denied"));

    // Clear the transient write denial and verify backup restores every key.
    store.fail_writes.clear();
    let backup = load_backup_from(&backup_file).expect("backup preserved");
    let revert_res = revert_registry_with_store(&mut store, &backup);
    assert!(revert_res.success());
    assert_eq!(store.values, original_snapshot);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn planted_corrupt_dword_in_backup_reports_error_without_panicking() {
    let dir = unique_temp_dir("corrupt-dword");
    let backup_file = dir.join("backup.json");
    let mut store = MockRegistry::baseline();

    let corrupt_backup = RegistryBackup {
        entries: vec![
            RegistryBackupEntry {
                path: r"SOFTWARE\Microsoft\Cryptography".into(),
                value_name: "MachineGuid".into(),
                original_value: "restored-guid".into(),
                value_type: BackupValueType::Sz,
            },
            RegistryBackupEntry {
                path: r"SOFTWARE\Microsoft\Windows NT\CurrentVersion".into(),
                value_name: "InstallDate".into(),
                original_value: "not-a-u32".into(),
                value_type: BackupValueType::Dword,
            },
        ],
        created_at: "2026-11-25T00:00:00Z".into(),
    };
    save_backup_to(&backup_file, &corrupt_backup).expect("write corrupt backup fixture");

    let loaded = load_backup_from(&backup_file).expect("load backup fixture");
    let res = revert_registry_with_store(&mut store, &loaded);
    assert!(!res.success());
    assert_eq!(res.applied.len(), 1);
    assert_eq!(res.failed.len(), 1);
    assert!(
        res.failed[0].1.contains("corrupt DWORD in backup"),
        "unexpected error: {:?}",
        res.failed
    );
    assert_eq!(
        store.values.get(&(
            r"SOFTWARE\Microsoft\Cryptography".to_string(),
            "MachineGuid".to_string()
        )),
        Some(&MockValue::Sz("restored-guid".into()))
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn active_config_and_layer_parsing_and_driver_ipc_error_paths() {
    let dir = unique_temp_dir("active-ipc");
    let cfg_path = dir.join(".config.json");

    let layers = parse_layers("0,kernel,userland").expect("parse valid layers");
    assert_eq!(
        layers,
        vec![Layer::Firmware, Layer::Kernel, Layer::Userland]
    );
    assert!(parse_layers("2,bogus-layer").is_err());

    let cfg = ActiveConfig::for_applied("work-profile", &layers);
    cfg.save_to(&cfg_path).expect("save active config");
    let loaded = ActiveConfig::load_from(&cfg_path)
        .expect("load active config")
        .expect("active config present");
    assert_eq!(loaded.planned(), Some(("work-profile", &[0u8, 1, 2][..])));

    ActiveConfig::clear_at(&cfg_path).expect("clear active config");
    assert_eq!(ActiveConfig::load_from(&cfg_path).unwrap(), None);

    let mut bad_profile = generate_profile("ipc-err-seed", "ipc-err");
    bad_profile.gpus[0].device_id = "ZZZZ".into();
    let ipc_err = serialize_kernel_profile(&bad_profile).unwrap_err();
    assert!(ipc_err.contains("Invalid device_id"), "got: {}", ipc_err);

    let _ = std::fs::remove_dir_all(&dir);
}
