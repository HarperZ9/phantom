//! The art gate settles whether a drawing fits its columns and matches the spec it was
//! rendered from. Both sides of that check read the same JSON, so it cannot settle
//! whether a drawing is TRUE. That is what this file is for: every count, name and rule
//! the three drawings put on the page is asserted here against the code that produces
//! it, so a claim that stops holding fails `cargo test` rather than staying on the page.
//! The byte-level parsers already have their own suites beside the code, and nothing
//! here duplicates them.

use phantom_cli::profile::engine::generate_profile;
use phantom_cli::profile::vendor_db::{
    BOARD_VENDORS, DISK_VENDORS, DISPLAY_VENDORS, GPU_VENDORS, NIC_VENDORS, TPM_VENDORS,
};
use phantom_cli::validator::diff::{validate_profile_against_sources, EntryStatus};
use phantom_cli::validator::sources::{read_all_sources, IdentifierMap, SourceReadResult};
use phantom_license::LicenseTier;
use std::path::{Path, PathBuf};

const DRAWINGS: [&str; 4] = [
    "docs/art/phantom-header.svg",
    "docs/art/apply-lane.svg",
    "docs/art/validate-lane.svg",
    "docs/art/identity-table.svg",
];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn spec() -> serde_json::Value {
    let path = repo_root().join("docs/art/phantom.art.json");
    let text = std::fs::read_to_string(&path).expect("the spec is committed");
    serde_json::from_str(&text).expect("the spec parses")
}

fn readme() -> String {
    std::fs::read_to_string(repo_root().join("README.md")).expect("the README is committed")
}

/// A rendered file nobody embeds is a file nobody sees, so both are checked.
#[test]
fn every_drawing_is_committed_and_reaches_the_page() {
    let page = readme();
    for drawing in DRAWINGS {
        assert!(repo_root().join(drawing).is_file(), "missing: {}", drawing);
        assert!(page.contains(drawing), "not embedded: {}", drawing);
    }
}

/// A screen reader gets the alt text, not the picture, so the two are pinned equal.
#[test]
fn the_alt_text_on_the_page_is_the_alt_text_in_the_spec() {
    let page = readme();
    let doc = spec();
    let mut checked = 0;
    for key in ["flows", "cards"] {
        for item in doc[key].as_array().expect("array") {
            let alt = item["alt"].as_str().expect("alt");
            let file = item["file"].as_str().expect("file");
            let embed = format!("![{}](docs/art/{})", alt, file);
            assert!(page.contains(&embed), "alt drifted for {}", file);
            checked += 1;
        }
    }
    assert_eq!(checked, 3);
}

/// "Seven sources read, and nothing written." Reading is all this does, on any platform.
#[test]
fn seven_sources_are_read_and_five_of_them_are_the_device_readers() {
    let sources = read_all_sources();
    assert_eq!(sources.len(), 7);
    let names: Vec<&str> = sources.iter().map(|s| s.source_name.as_str()).collect();
    for expected in [
        "SMBIOS Firmware Table",
        "Disk Identifiers",
        "Network Adapters",
        "GPU Devices",
        "Display EDID",
        "TPM Module",
    ] {
        assert!(names.contains(&expected), "no reader named {}", expected);
    }
}

/// "One seed hashed into a stream cipher": same seed, same machine identity, every time.
#[test]
fn one_seed_rebuilds_the_same_identity() {
    let first = generate_profile("a-fixed-seed", "alpha");
    let second = generate_profile("a-fixed-seed", "beta");
    assert_eq!(first.smbios.system_uuid, second.smbios.system_uuid);
    assert_eq!(first.smbios.board_serial, second.smbios.board_serial);
    assert_eq!(first.os.machine_guid, second.os.machine_guid);
    assert_eq!(first.disks[0].serial, second.disks[0].serial);
    assert_eq!(
        first.network_adapters[0].permanent_mac,
        second.network_adapters[0].permanent_mac
    );
}

/// The other half of that claim: a different seed has to give a different machine, or
/// the identity would be a constant rather than a generated one.
#[test]
fn a_different_seed_gives_a_different_identity() {
    let first = generate_profile("seed-one", "alpha");
    let second = generate_profile("seed-two", "alpha");
    assert_ne!(first.smbios.system_uuid, second.smbios.system_uuid);
    assert_ne!(first.os.machine_guid, second.os.machine_guid);
}

/// "Samsung serials in Samsung's format." A generated disk serial has to carry the
/// prefix and length of the vendor it claims, and its model has to come from that same
/// vendor's list, or the identity would be inconsistent under inspection.
#[test]
fn a_generated_disk_serial_carries_its_own_vendor_format() {
    for index in 0..40 {
        let profile = generate_profile(&format!("seed-{}", index), "disk-check");
        for disk in &profile.disks {
            let vendor = DISK_VENDORS
                .iter()
                .find(|v| v.models.contains(&disk.model.as_str()))
                .unwrap_or_else(|| panic!("model outside the vendor table: {}", disk.model));
            assert!(
                disk.serial.starts_with(vendor.serial_prefix),
                "{} is not a {} serial",
                disk.serial,
                vendor.manufacturer
            );
            assert_eq!(
                disk.serial.len(),
                vendor.serial_prefix.len() + vendor.serial_suffix_len
            );
        }
    }
}

/// "four, real OUIs" on the identity card. Every generated MAC has to open with one of
/// the eighteen registered prefixes rather than with random leading bytes.
#[test]
fn every_generated_mac_opens_with_a_registered_prefix() {
    let registered: Vec<String> = NIC_VENDORS
        .iter()
        .flat_map(|v| v.oui_prefixes.iter())
        .map(|p| format!("{:02X}:{:02X}:{:02X}", p[0], p[1], p[2]))
        .collect();
    assert_eq!(registered.len(), 18);
    for index in 0..40 {
        let profile = generate_profile(&format!("seed-{}", index), "mac-check");
        for nic in &profile.network_adapters {
            let head: String = nic.permanent_mac.chars().take(8).collect();
            assert!(
                registered.contains(&head),
                "unregistered OUI: {}",
                nic.permanent_mac
            );
        }
    }
}

/// The vendor counts printed in the "how many" column. These are the numbers a reader
/// sees, so they are asserted rather than described.
#[test]
fn the_vendor_counts_are_the_counts_the_card_prints() {
    assert_eq!(BOARD_VENDORS.len(), 4);
    assert_eq!(DISK_VENDORS.len(), 4);
    assert_eq!(NIC_VENDORS.len(), 4);
    assert_eq!(TPM_VENDORS.len(), 4);
    assert_eq!(DISPLAY_VENDORS.len(), 7);
    let models: usize = GPU_VENDORS.iter().map(|v| v.devices.len()).sum();
    assert_eq!(models, 14);
}

/// "Eight NVIDIA and six AMD, each paired with the real PCI device id it carries under
/// the vendor id that owns it."
#[test]
fn the_gpu_table_splits_eight_nvidia_and_six_amd_under_their_own_vendor_ids() {
    let nvidia = GPU_VENDORS
        .iter()
        .find(|v| v.vendor_id == 0x10DE)
        .expect("NVIDIA");
    let amd = GPU_VENDORS
        .iter()
        .find(|v| v.vendor_id == 0x1002)
        .expect("AMD");
    assert_eq!(GPU_VENDORS.len(), 2);
    assert_eq!(nvidia.devices.len(), 8);
    assert_eq!(amd.devices.len(), 6);
    for (_, name) in nvidia.devices {
        assert!(name.starts_with("NVIDIA"), "{}", name);
    }
    for (_, name) in amd.devices {
        assert!(name.starts_with("AMD"), "{}", name);
    }
}

/// The four board, four disk, four network and four TPM vendors the card names, plus the
/// seven display vendors under the three letter EDID codes the standard assigns them.
#[test]
fn the_card_names_the_vendors_the_tables_actually_hold() {
    for (named, actual) in [
        ("ASUSTeK", BOARD_VENDORS[0].manufacturer),
        ("Gigabyte", BOARD_VENDORS[1].manufacturer),
        ("Micro-Star", BOARD_VENDORS[2].manufacturer),
        ("ASRock", BOARD_VENDORS[3].manufacturer),
        ("Samsung", DISK_VENDORS[0].manufacturer),
        ("Western Digital", DISK_VENDORS[1].manufacturer),
        ("Seagate", DISK_VENDORS[2].manufacturer),
        ("Crucial", DISK_VENDORS[3].manufacturer),
        ("Intel", NIC_VENDORS[0].manufacturer),
        ("Realtek", NIC_VENDORS[1].manufacturer),
        ("Broadcom", NIC_VENDORS[2].manufacturer),
        ("Qualcomm", NIC_VENDORS[3].manufacturer),
        ("Infineon", TPM_VENDORS[0].manufacturer_name),
        ("STMicroelectronics", TPM_VENDORS[1].manufacturer_name),
        ("Nuvoton", TPM_VENDORS[2].manufacturer_name),
        ("Intel", TPM_VENDORS[3].manufacturer_name),
    ] {
        assert!(actual.starts_with(named), "{} is not {}", actual, named);
    }
    let codes: Vec<&str> = DISPLAY_VENDORS
        .iter()
        .map(|v| v.manufacturer_code)
        .collect();
    assert_eq!(codes, ["DEL", "GSM", "SAM", "ACI", "ACR", "BNQ", "HWP"]);
}

/// "Match, mismatch, or unavailable, per field." A profile checked against nothing has
/// every field unavailable, and a field that reads back its own value matches. The
/// fourth variant the enum declares is never produced, which is why the page names three.
#[test]
fn a_checked_field_lands_on_match_mismatch_or_unavailable() {
    let profile = generate_profile("status-seed", "status");

    let nothing = validate_profile_against_sources(&profile, &[]);
    assert!(nothing.total_checked > 0);
    assert!(nothing
        .entries
        .iter()
        .all(|e| e.status == EntryStatus::NotAvailable));

    let mut ids = IdentifierMap::new();
    ids.insert(
        "smbios.system_uuid".into(),
        profile.smbios.system_uuid.clone(),
    );
    ids.insert("os.machine_guid".into(), "not-the-profile-value".into());
    let read = [SourceReadResult {
        source_name: "a reader".into(),
        identifiers: ids,
        errors: Vec::new(),
    }];
    let checked = validate_profile_against_sources(&profile, &read);
    let status = |key: &str| {
        checked
            .entries
            .iter()
            .find(|e| e.identifier == key)
            .map(|e| &e.status)
            .expect(key)
    };
    assert_eq!(status("smbios.system_uuid"), &EntryStatus::Match);
    assert_eq!(status("os.machine_guid"), &EntryStatus::Mismatch);
    assert_eq!(checked.mismatches, 1);
    assert!(checked
        .entries
        .iter()
        .all(|e| e.status != EntryStatus::Missing));
}

/// "the computer name / modeled, not applied" is the one accented row on the card, and
/// the accent is the claim: the profile carries a computer name that apply leaves alone.
#[test]
fn the_computer_name_is_modeled_and_the_card_carries_one_mark() {
    let profile = generate_profile("name-seed", "names");
    assert!(!profile.os.computer_name.is_empty());
    let card = spec()["cards"][0].clone();
    let fields = card["fields"].as_array().expect("fields").clone();
    let accented: Vec<&serde_json::Value> =
        fields.iter().filter(|f| f.get("tone").is_some()).collect();
    assert_eq!(accented.len(), 1);
    assert_eq!(accented[0]["key"], "the computer name");
    assert_eq!(fields.len(), 12);
    assert!(card["alt"]
        .as_str()
        .expect("alt")
        .starts_with("Twelve rows"));
}

/// "profiles per tier / two, fifty, unlimited", and the free tier reaching Layer 2 only.
#[test]
fn the_tier_limits_are_two_fifty_and_unlimited() {
    assert_eq!(LicenseTier::Free.max_profiles(), 2);
    assert_eq!(LicenseTier::Pro.max_profiles(), 50);
    assert_eq!(LicenseTier::Enterprise.max_profiles(), usize::MAX);
    assert!(LicenseTier::Free.allows_layer(2));
    assert!(!LicenseTier::Free.allows_layer(1));
    assert!(!LicenseTier::Free.allows_layer(0));
}
