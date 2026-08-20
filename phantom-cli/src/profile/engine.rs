use rand::Rng;
use rand_chacha::ChaCha20Rng;
use rand::SeedableRng;
use sha2::{Sha256, Digest};

use super::schema::*;
use super::vendor_db::*;

pub fn generate_profile(seed: &str, name: &str) -> HardwareProfile {
    let mut rng = seed_to_rng(seed);

    let board_vendor = &BOARD_VENDORS[rng.gen_range(0..BOARD_VENDORS.len())];
    let disk_vendor = &DISK_VENDORS[rng.gen_range(0..DISK_VENDORS.len())];
    let nic_vendor = &NIC_VENDORS[rng.gen_range(0..NIC_VENDORS.len())];
    let gpu_vendor = &GPU_VENDORS[rng.gen_range(0..GPU_VENDORS.len())];
    let tpm_vendor = &TPM_VENDORS[rng.gen_range(0..TPM_VENDORS.len())];
    let display_vendor = &DISPLAY_VENDORS[rng.gen_range(0..DISPLAY_VENDORS.len())];

    let system_uuid = generate_uuid(&mut rng);

    let smbios = generate_smbios(board_vendor, &system_uuid, &mut rng);
    let disks = generate_disks(disk_vendor, &mut rng);
    let network_adapters = generate_nics(nic_vendor, &mut rng);
    let gpus = generate_gpus(gpu_vendor, &mut rng);
    let tpm = Some(generate_tpm(tpm_vendor));
    let displays = generate_displays(display_vendor, &mut rng);
    let os = generate_os_ids(&system_uuid, &mut rng);
    let boot = generate_boot_ids(&mut rng);

    HardwareProfile {
        metadata: ProfileMetadata {
            name: name.to_string(),
            seed: seed.to_string(),
            created_at: current_timestamp(),
            phantom_version: env!("CARGO_PKG_VERSION").to_string(),
        },
        smbios,
        disks,
        network_adapters,
        gpus,
        tpm,
        displays,
        os,
        boot,
    }
}

fn seed_to_rng(seed: &str) -> ChaCha20Rng {
    let mut hasher = Sha256::new();
    hasher.update(seed.as_bytes());
    let hash = hasher.finalize();
    let mut seed_bytes = [0u8; 32];
    seed_bytes.copy_from_slice(&hash);
    ChaCha20Rng::from_seed(seed_bytes)
}

fn generate_uuid<R: Rng>(rng: &mut R) -> String {
    let mut bytes = [0u8; 16];
    rng.fill(&mut bytes);
    bytes[6] = (bytes[6] & 0x0f) | 0x40; // version 4
    bytes[8] = (bytes[8] & 0x3f) | 0x80; // variant 1
    format!(
        "{:08X}-{:04X}-{:04X}-{:04X}-{:012X}",
        u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        u16::from_be_bytes([bytes[4], bytes[5]]),
        u16::from_be_bytes([bytes[6], bytes[7]]),
        u16::from_be_bytes([bytes[8], bytes[9]]),
        u64::from_be_bytes([0, 0, bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]]),
    )
}

fn generate_smbios<R: Rng>(vendor: &BoardVendor, system_uuid: &str, rng: &mut R) -> SmbiosIdentifiers {
    let product = vendor.products[rng.gen_range(0..vendor.products.len())];
    let board_serial = generate_serial(
        vendor.serial_prefix,
        vendor.serial_suffix_len,
        vendor.serial_charset,
        rng,
    );
    let system_serial = generate_serial(
        vendor.serial_prefix,
        vendor.serial_suffix_len,
        vendor.serial_charset,
        rng,
    );
    let chassis_serial = generate_serial(
        vendor.serial_prefix,
        vendor.serial_suffix_len,
        vendor.serial_charset,
        rng,
    );

    let bios_version = format!(
        "{}{}",
        vendor.bios_version_prefix,
        generate_serial("", 4, SerialCharset::Numeric, rng),
    );

    SmbiosIdentifiers {
        board_serial,
        board_manufacturer: vendor.manufacturer.to_string(),
        board_product: product.to_string(),
        system_uuid: system_uuid.to_string(),
        system_serial,
        system_manufacturer: vendor.manufacturer.to_string(),
        system_product: product.to_string(),
        chassis_serial,
        chassis_asset_tag: format!("Asset-{}", generate_serial("", 6, SerialCharset::Numeric, rng)),
        bios_vendor: vendor.bios_vendor.to_string(),
        bios_version,
    }
}

fn generate_disks<R: Rng>(vendor: &DiskVendor, rng: &mut R) -> Vec<DiskIdentifiers> {
    let disk_count = rng.gen_range(1..=3u32);
    (0..disk_count)
        .map(|i| {
            let model = vendor.models[rng.gen_range(0..vendor.models.len())];
            let serial = generate_serial(
                vendor.serial_prefix,
                vendor.serial_suffix_len,
                vendor.serial_charset,
                rng,
            );
            let firmware_rev = generate_serial(
                vendor.firmware_prefix,
                vendor.firmware_suffix_len,
                SerialCharset::AlphaNumeric,
                rng,
            );
            let volume_serial = format!(
                "{:04X}-{:04X}",
                rng.gen::<u16>(),
                rng.gen::<u16>(),
            );
            let volume_guid = generate_uuid(rng);

            DiskIdentifiers {
                index: i,
                serial,
                model: model.to_string(),
                firmware_rev,
                volume_serial,
                volume_guid,
            }
        })
        .collect()
}

fn generate_nics<R: Rng>(vendor: &NicVendor, rng: &mut R) -> Vec<NetworkIdentifiers> {
    let nic_count = rng.gen_range(1..=2u32);
    (0..nic_count)
        .map(|i| {
            let oui = vendor.oui_prefixes[rng.gen_range(0..vendor.oui_prefixes.len())];
            let mut mac_bytes = [0u8; 6];
            mac_bytes[0] = oui[0];
            mac_bytes[1] = oui[1];
            mac_bytes[2] = oui[2];
            rng.fill(&mut mac_bytes[3..6]);

            let mac = format!(
                "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                mac_bytes[0], mac_bytes[1], mac_bytes[2],
                mac_bytes[3], mac_bytes[4], mac_bytes[5],
            );

            let adapter_name = if i == 0 {
                format!("{} Family Controller", vendor.adapter_name_prefix)
            } else {
                format!("{} Family Controller #{}", vendor.adapter_name_prefix, i + 1)
            };

            NetworkIdentifiers {
                adapter_name,
                permanent_mac: mac.clone(),
                current_mac: mac,
                adapter_guid: format!("{{{}}}", generate_uuid(rng)),
            }
        })
        .collect()
}

fn generate_gpus<R: Rng>(vendor: &GpuDevice, rng: &mut R) -> Vec<GpuIdentifiers> {
    let (device_id, _name) = vendor.devices[rng.gen_range(0..vendor.devices.len())];
    let subsys: u32 = rng.gen();
    let instance_suffix: u32 = rng.gen();

    vec![GpuIdentifiers {
        vendor_id: format!("{:04X}", vendor.vendor_id),
        device_id: format!("{:04X}", device_id),
        subsystem_id: format!("{:04X}{:04X}", subsys >> 16, subsys & 0xFFFF),
        pnp_instance_id: format!(
            "PCI\\VEN_{:04X}&DEV_{:04X}&SUBSYS_{:08X}&REV_A1\\{:08X}",
            vendor.vendor_id, device_id, subsys, instance_suffix,
        ),
        driver_key_guid: format!("{{{}}}", generate_uuid(rng)),
    }]
}

fn generate_tpm(vendor: &TpmVendor) -> TpmIdentifiers {
    TpmIdentifiers {
        manufacturer_id: vendor.manufacturer_id.to_string(),
        manufacturer_name: vendor.manufacturer_name.to_string(),
        spec_version: vendor.spec_version.to_string(),
    }
}

fn generate_displays<R: Rng>(vendor: &DisplayVendor, rng: &mut R) -> Vec<DisplayIdentifiers> {
    let product_code = rng.gen_range(vendor.product_code_range.0..=vendor.product_code_range.1);
    let serial = generate_serial("", 8, SerialCharset::AlphaNumeric, rng);
    let year = rng.gen_range(2020..=2025u16);

    vec![DisplayIdentifiers {
        manufacturer_code: vendor.manufacturer_code.to_string(),
        product_code: format!("{:04X}", product_code),
        serial_number: serial,
        manufacture_year: year,
    }]
}

fn generate_os_ids<R: Rng>(system_uuid: &str, rng: &mut R) -> OsIdentifiers {
    let machine_guid = derive_machine_guid(system_uuid, rng);

    let adj = COMPUTER_NAME_ADJECTIVES[rng.gen_range(0..COMPUTER_NAME_ADJECTIVES.len())];
    let suffix = generate_serial("", 7, SerialCharset::AlphaNumeric, rng);
    let computer_name = format!("{}-{}", adj, suffix);

    let pid_prefix = PRODUCT_ID_PREFIXES[rng.gen_range(0..PRODUCT_ID_PREFIXES.len())];
    let product_id = format!(
        "{}-{}-{}-{}",
        pid_prefix,
        generate_serial("", 5, SerialCharset::Numeric, rng),
        generate_serial("", 5, SerialCharset::Numeric, rng),
        generate_serial("", 5, SerialCharset::Numeric, rng),
    );

    let install_date = rng.gen_range(1640000000u64..=1724000000u64);

    OsIdentifiers {
        machine_guid,
        hw_profile_guid: format!("{{{}}}", generate_uuid(rng)),
        machine_id: format!("{{{}}}", generate_uuid(rng)),
        product_id,
        computer_name,
        install_date,
    }
}

fn derive_machine_guid<R: Rng>(system_uuid: &str, rng: &mut R) -> String {
    let mut hasher = Sha256::new();
    hasher.update(system_uuid.as_bytes());
    hasher.update(rng.gen::<u64>().to_le_bytes());
    let hash = hasher.finalize();

    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        hash[0], hash[1], hash[2], hash[3],
        hash[4], hash[5],
        hash[6], hash[7],
        hash[8], hash[9],
        hash[10], hash[11], hash[12], hash[13], hash[14], hash[15],
    )
}

fn generate_boot_ids<R: Rng>(rng: &mut R) -> BootIdentifiers {
    BootIdentifiers {
        bcd_guid: format!("{{{}}}", generate_uuid(rng)),
        disk_signature: format!("{:08X}", rng.gen::<u32>()),
    }
}

pub fn current_timestamp() -> String {
    use std::time::SystemTime;
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let days = secs / 86400;
    let time_secs = secs % 86400;
    let hours = time_secs / 3600;
    let mins = (time_secs % 3600) / 60;
    let s = time_secs % 60;

    let (y, m, d) = days_to_ymd(days);
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, m, d, hours, mins, s)
}

fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    let mut y = 1970;
    loop {
        let year_days = if is_leap(y) { 366 } else { 365 };
        if days < year_days {
            break;
        }
        days -= year_days;
        y += 1;
    }
    let month_days: [u64; 12] = if is_leap(y) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut m = 0;
    for (i, &md) in month_days.iter().enumerate() {
        if days < md {
            m = i as u64 + 1;
            break;
        }
        days -= md;
    }
    (y, m, days + 1)
}

fn is_leap(y: u64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_generation() {
        let p1 = generate_profile("test-seed-123", "test");
        let p2 = generate_profile("test-seed-123", "test");
        assert_eq!(p1.smbios.system_uuid, p2.smbios.system_uuid);
        assert_eq!(p1.smbios.board_serial, p2.smbios.board_serial);
        assert_eq!(p1.os.machine_guid, p2.os.machine_guid);
        assert_eq!(p1.network_adapters[0].permanent_mac, p2.network_adapters[0].permanent_mac);
    }

    #[test]
    fn different_seeds_produce_different_profiles() {
        let p1 = generate_profile("seed-a", "a");
        let p2 = generate_profile("seed-b", "b");
        assert_ne!(p1.smbios.system_uuid, p2.smbios.system_uuid);
    }

    #[test]
    fn mac_addresses_match() {
        let p = generate_profile("mac-test", "test");
        for nic in &p.network_adapters {
            assert_eq!(nic.permanent_mac, nic.current_mac);
        }
    }

    #[test]
    fn uuid_format_valid() {
        let p = generate_profile("uuid-test", "test");
        let parts: Vec<&str> = p.smbios.system_uuid.split('-').collect();
        assert_eq!(parts.len(), 5);
        assert_eq!(parts[0].len(), 8);
        assert_eq!(parts[1].len(), 4);
        assert_eq!(parts[2].len(), 4);
        assert_eq!(parts[3].len(), 4);
        assert_eq!(parts[4].len(), 12);
    }

    #[test]
    fn profile_has_at_least_one_of_each() {
        let p = generate_profile("coverage-test", "test");
        assert!(!p.disks.is_empty());
        assert!(!p.network_adapters.is_empty());
        assert!(!p.gpus.is_empty());
        assert!(p.tpm.is_some());
        assert!(!p.displays.is_empty());
    }

    #[test]
    fn vendor_format_disk_serials() {
        let p = generate_profile("vendor-test", "test");
        for disk in &p.disks {
            assert!(!disk.serial.is_empty());
            assert!(!disk.model.is_empty());
            assert!(!disk.firmware_rev.is_empty());
            assert!(disk.volume_serial.contains('-'), "volume serial should be XXXX-XXXX");
        }
    }

    #[test]
    fn vendor_format_mac_oui() {
        let p = generate_profile("oui-test", "test");
        for nic in &p.network_adapters {
            let parts: Vec<&str> = nic.permanent_mac.split(':').collect();
            assert_eq!(parts.len(), 6);
            for part in &parts {
                assert_eq!(part.len(), 2);
                assert!(u8::from_str_radix(part, 16).is_ok());
            }
        }
    }

    #[test]
    fn gpu_ids_are_valid_hex() {
        let p = generate_profile("gpu-hex-test", "test");
        for gpu in &p.gpus {
            assert!(u16::from_str_radix(&gpu.vendor_id, 16).is_ok());
            assert!(u16::from_str_radix(&gpu.device_id, 16).is_ok());
            assert!(u32::from_str_radix(&gpu.subsystem_id, 16).is_ok());
            assert!(gpu.pnp_instance_id.starts_with("PCI\\VEN_"));
            assert!(gpu.driver_key_guid.starts_with('{'));
        }
    }

    #[test]
    fn os_identifiers_format() {
        let p = generate_profile("os-test", "test");
        assert!(!p.os.computer_name.is_empty());
        assert!(p.os.computer_name.contains('-'));
        assert!(p.os.machine_guid.contains('-'));
        assert!(p.os.hw_profile_guid.starts_with('{'));
        assert!(p.os.machine_id.starts_with('{'));
        assert!(p.os.install_date >= 1640000000);
    }

    #[test]
    fn boot_identifiers_format() {
        let p = generate_profile("boot-test", "test");
        assert!(p.boot.bcd_guid.starts_with('{'));
        assert_eq!(p.boot.disk_signature.len(), 8);
        assert!(u32::from_str_radix(&p.boot.disk_signature, 16).is_ok());
    }

    #[test]
    fn tpm_fields_populated() {
        let p = generate_profile("tpm-test", "test");
        let tpm = p.tpm.as_ref().unwrap();
        assert!(!tpm.manufacturer_id.is_empty());
        assert!(!tpm.manufacturer_name.is_empty());
        assert!(!tpm.spec_version.is_empty());
    }

    #[test]
    fn display_fields_populated() {
        let p = generate_profile("display-test", "test");
        assert!(!p.displays.is_empty());
        for display in &p.displays {
            assert_eq!(display.manufacturer_code.len(), 3);
            assert!(u16::from_str_radix(&display.product_code, 16).is_ok());
            assert!(!display.serial_number.is_empty());
            assert!(display.manufacture_year >= 2020);
        }
    }

    #[test]
    fn profile_json_roundtrip() {
        let p = generate_profile("json-roundtrip", "test");
        let json = serde_json::to_string(&p).unwrap();
        let p2: HardwareProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(p.smbios.system_uuid, p2.smbios.system_uuid);
        assert_eq!(p.smbios.board_serial, p2.smbios.board_serial);
        assert_eq!(p.os.machine_guid, p2.os.machine_guid);
        assert_eq!(p.os.install_date, p2.os.install_date);
        assert_eq!(p.boot.bcd_guid, p2.boot.bcd_guid);
        assert_eq!(p.disks.len(), p2.disks.len());
        assert_eq!(p.network_adapters.len(), p2.network_adapters.len());
    }

    #[test]
    fn identifier_count_covers_all_fields() {
        let p = generate_profile("count-test", "test");
        let count = p.identifier_count();
        assert!(count >= 30, "expected at least 30 identifiers, got {}", count);
    }
}
