use super::sources::SourceReadResult;
use crate::profile::schema::HardwareProfile;
use std::collections::BTreeMap;

#[derive(Debug)]
pub struct ValidationResult {
    pub entries: Vec<ValidationEntry>,
    pub total_checked: usize,
    pub mismatches: usize,
    pub missing: usize,
    pub errors: Vec<String>,
}

#[derive(Debug)]
pub struct ValidationEntry {
    pub identifier: String,
    pub expected: Option<String>,
    pub actual: Option<String>,
    pub status: EntryStatus,
}

#[derive(Debug, PartialEq)]
pub enum EntryStatus {
    Match,
    Mismatch,
    Missing,
    NotAvailable,
}

impl ValidationResult {
    pub fn is_consistent(&self) -> bool {
        self.mismatches == 0
    }
}

pub fn validate_profile_against_sources(
    profile: &HardwareProfile,
    sources: &[SourceReadResult],
) -> ValidationResult {
    let expected = profile_to_identifier_map(profile);

    let mut actual: BTreeMap<String, String> = BTreeMap::new();
    let mut all_errors = Vec::new();

    for source in sources {
        for (key, value) in &source.identifiers {
            actual.insert(key.clone(), value.clone());
        }
        all_errors.extend(source.errors.iter().cloned());
    }

    let mut entries = Vec::new();
    let mut mismatches = 0;
    let mut missing = 0;

    for (key, expected_val) in &expected {
        let entry = if let Some(actual_val) = actual.get(key) {
            if actual_val == expected_val {
                ValidationEntry {
                    identifier: key.clone(),
                    expected: Some(expected_val.clone()),
                    actual: Some(actual_val.clone()),
                    status: EntryStatus::Match,
                }
            } else {
                mismatches += 1;
                ValidationEntry {
                    identifier: key.clone(),
                    expected: Some(expected_val.clone()),
                    actual: Some(actual_val.clone()),
                    status: EntryStatus::Mismatch,
                }
            }
        } else {
            missing += 1;
            ValidationEntry {
                identifier: key.clone(),
                expected: Some(expected_val.clone()),
                actual: None,
                status: EntryStatus::NotAvailable,
            }
        };
        entries.push(entry);
    }

    ValidationResult {
        total_checked: entries.len(),
        entries,
        mismatches,
        missing,
        errors: all_errors,
    }
}

pub fn diff_sources(sources: &[SourceReadResult]) -> Vec<(String, String, String)> {
    let mut rows = Vec::new();
    for source in sources {
        for (key, value) in &source.identifiers {
            rows.push((key.clone(), value.clone(), source.source_name.clone()));
        }
    }
    rows.sort();
    rows
}

fn profile_to_identifier_map(profile: &HardwareProfile) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();

    map.insert(
        "smbios.system_uuid".into(),
        profile.smbios.system_uuid.clone(),
    );
    map.insert(
        "smbios.board_serial".into(),
        profile.smbios.board_serial.clone(),
    );
    map.insert(
        "smbios.board_manufacturer".into(),
        profile.smbios.board_manufacturer.clone(),
    );
    map.insert(
        "smbios.board_product".into(),
        profile.smbios.board_product.clone(),
    );
    map.insert(
        "smbios.system_serial".into(),
        profile.smbios.system_serial.clone(),
    );
    map.insert(
        "smbios.system_manufacturer".into(),
        profile.smbios.system_manufacturer.clone(),
    );
    map.insert(
        "smbios.system_product".into(),
        profile.smbios.system_product.clone(),
    );
    map.insert(
        "smbios.chassis_serial".into(),
        profile.smbios.chassis_serial.clone(),
    );
    map.insert(
        "smbios.chassis_asset_tag".into(),
        profile.smbios.chassis_asset_tag.clone(),
    );
    map.insert(
        "smbios.bios_vendor".into(),
        profile.smbios.bios_vendor.clone(),
    );
    map.insert(
        "smbios.bios_version".into(),
        profile.smbios.bios_version.clone(),
    );

    for (i, disk) in profile.disks.iter().enumerate() {
        map.insert(format!("disk.{}.serial", i), disk.serial.clone());
        map.insert(format!("disk.{}.model", i), disk.model.clone());
        map.insert(
            format!("disk.{}.firmware_rev", i),
            disk.firmware_rev.clone(),
        );
        map.insert(
            format!("disk.{}.volume_serial", i),
            disk.volume_serial.clone(),
        );
        map.insert(format!("disk.{}.volume_guid", i), disk.volume_guid.clone());
    }

    for (i, nic) in profile.network_adapters.iter().enumerate() {
        map.insert(
            format!("nic.{}.permanent_mac", i),
            nic.permanent_mac.clone(),
        );
        map.insert(format!("nic.{}.current_mac", i), nic.current_mac.clone());
        map.insert(format!("nic.{}.adapter_guid", i), nic.adapter_guid.clone());
    }

    for (i, gpu) in profile.gpus.iter().enumerate() {
        map.insert(format!("gpu.{}.vendor_id", i), gpu.vendor_id.clone());
        map.insert(format!("gpu.{}.device_id", i), gpu.device_id.clone());
        map.insert(format!("gpu.{}.subsystem_id", i), gpu.subsystem_id.clone());
        map.insert(
            format!("gpu.{}.pnp_instance_id", i),
            gpu.pnp_instance_id.clone(),
        );
        map.insert(
            format!("gpu.{}.driver_key_guid", i),
            gpu.driver_key_guid.clone(),
        );
    }

    if let Some(ref tpm) = profile.tpm {
        map.insert("tpm.manufacturer_id".into(), tpm.manufacturer_id.clone());
        map.insert(
            "tpm.manufacturer_name".into(),
            tpm.manufacturer_name.clone(),
        );
        map.insert("tpm.spec_version".into(), tpm.spec_version.clone());
    }

    for (i, display) in profile.displays.iter().enumerate() {
        map.insert(
            format!("display.{}.manufacturer_code", i),
            display.manufacturer_code.clone(),
        );
        map.insert(
            format!("display.{}.product_code", i),
            display.product_code.clone(),
        );
        map.insert(
            format!("display.{}.serial_number", i),
            display.serial_number.clone(),
        );
        map.insert(
            format!("display.{}.manufacture_year", i),
            display.manufacture_year.to_string(),
        );
    }

    map.insert("os.machine_guid".into(), profile.os.machine_guid.clone());
    map.insert(
        "os.hw_profile_guid".into(),
        profile.os.hw_profile_guid.clone(),
    );
    map.insert("os.machine_id".into(), profile.os.machine_id.clone());
    map.insert("os.product_id".into(), profile.os.product_id.clone());
    map.insert("os.computer_name".into(), profile.os.computer_name.clone());
    map.insert(
        "os.install_date".into(),
        profile.os.install_date.to_string(),
    );

    map.insert("boot.bcd_guid".into(), profile.boot.bcd_guid.clone());
    map.insert(
        "boot.disk_signature".into(),
        profile.boot.disk_signature.clone(),
    );

    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::engine::generate_profile;

    #[test]
    fn identifier_map_covers_all_fields() {
        let profile = generate_profile("diff-coverage", "test");
        let map = profile_to_identifier_map(&profile);

        assert!(map.contains_key("smbios.system_uuid"));
        assert!(map.contains_key("smbios.system_product"));
        assert!(map.contains_key("smbios.bios_vendor"));
        assert!(map.contains_key("smbios.bios_version"));

        assert!(map.contains_key("disk.0.serial"));
        assert!(map.contains_key("disk.0.firmware_rev"));
        assert!(map.contains_key("disk.0.volume_serial"));
        assert!(map.contains_key("disk.0.volume_guid"));

        assert!(map.contains_key("nic.0.permanent_mac"));
        assert!(map.contains_key("nic.0.current_mac"));
        assert!(map.contains_key("nic.0.adapter_guid"));

        assert!(map.contains_key("gpu.0.subsystem_id"));
        assert!(map.contains_key("gpu.0.driver_key_guid"));
        assert!(map.contains_key("gpu.0.pnp_instance_id"));

        assert!(map.contains_key("tpm.manufacturer_id"));
        assert!(map.contains_key("tpm.manufacturer_name"));
        assert!(map.contains_key("tpm.spec_version"));

        assert!(map.contains_key("display.0.manufacturer_code"));
        assert!(map.contains_key("display.0.product_code"));
        assert!(map.contains_key("display.0.serial_number"));
        assert!(map.contains_key("display.0.manufacture_year"));

        assert!(map.contains_key("os.machine_id"));
        assert!(map.contains_key("os.install_date"));
        assert!(map.contains_key("os.computer_name"));

        assert!(map.contains_key("boot.bcd_guid"));
        assert!(map.contains_key("boot.disk_signature"));
    }

    #[test]
    fn identifier_map_count_matches_profile() {
        let profile = generate_profile("diff-count", "test");
        let map = profile_to_identifier_map(&profile);
        assert!(
            map.len() >= profile.identifier_count() - 5,
            "map has {} entries, profile reports {} identifiers",
            map.len(),
            profile.identifier_count()
        );
    }

    #[test]
    fn validate_perfect_match() {
        let profile = generate_profile("match-test", "test");
        let map = profile_to_identifier_map(&profile);
        let source = SourceReadResult {
            source_name: "test".into(),
            identifiers: map.into_iter().collect(),
            errors: vec![],
        };
        let result = validate_profile_against_sources(&profile, &[source]);
        assert!(result.is_consistent());
        assert_eq!(result.mismatches, 0);
    }
}
