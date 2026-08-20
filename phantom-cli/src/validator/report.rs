use super::diff::{ValidationResult, EntryStatus};
use super::sources::SourceReadResult;

pub fn print_validation_report(result: &ValidationResult) {
    println!("\n  Phantom Validation Report");
    println!("  {}\n", "=".repeat(60));

    let pass = result.entries.iter().filter(|e| e.status == EntryStatus::Match).count();
    let fail = result.mismatches;
    let unavail = result.entries.iter().filter(|e| e.status == EntryStatus::NotAvailable).count();

    println!("  Checked:       {}", result.total_checked);
    println!("  Matching:      {}", pass);
    println!("  Mismatched:    {}", fail);
    println!("  Unavailable:   {}", unavail);
    println!();

    if fail > 0 {
        println!("  MISMATCHES:");
        println!("  {}", "-".repeat(60));
        for entry in &result.entries {
            if entry.status == EntryStatus::Mismatch {
                println!("  {}", entry.identifier);
                println!("    expected: {}", entry.expected.as_deref().unwrap_or("?"));
                println!("    actual:   {}", entry.actual.as_deref().unwrap_or("?"));
                println!();
            }
        }
    }

    if !result.errors.is_empty() {
        println!("  ERRORS:");
        println!("  {}", "-".repeat(60));
        for err in &result.errors {
            println!("  - {}", err);
        }
        println!();
    }

    if result.is_consistent() && fail == 0 {
        println!("  Result: CONSISTENT");
        println!("  All readable identifiers match the active profile.");
    } else if fail > 0 {
        println!("  Result: INCONSISTENT");
        println!("  {} identifier(s) do not match the expected profile.", fail);
        println!("  This may be detectable by fingerprinting software.");
    }

    println!();
}

pub fn print_audit_report(sources: &[SourceReadResult]) {
    println!("\n  Phantom Hardware Identity Audit");
    println!("  {}\n", "=".repeat(60));
    println!("  Current hardware identifiers as seen by software on this machine.\n");

    let mut total = 0;

    for source in sources {
        println!("  [{}]", source.source_name);

        if source.identifiers.is_empty() && source.errors.is_empty() {
            println!("    (no data)");
        }

        for (key, value) in &source.identifiers {
            let display_val = if value.len() > 50 {
                format!("{}...", &value[..47])
            } else {
                value.clone()
            };
            println!("    {:<35} {}", key, display_val);
            total += 1;
        }

        for err in &source.errors {
            println!("    ! {}", err);
        }

        println!();
    }

    println!("  Total identifiers read: {}", total);
    println!("  These values uniquely identify this machine to any software that queries them.");
    println!();
}

pub fn print_profile_summary(profile: &crate::profile::schema::HardwareProfile) {
    println!("\n  Profile: {}", profile.metadata.name);
    println!("  {}", "-".repeat(60));
    println!("  Seed:      {}", profile.metadata.seed);
    println!("  Created:   {}", profile.metadata.created_at);
    println!("  Version:   {}", profile.metadata.phantom_version);
    println!("  Vectors:   {}", profile.identifier_count());
    println!();

    println!("  SMBIOS:");
    println!("    Manufacturer:  {}", profile.smbios.board_manufacturer);
    println!("    Product:       {}", profile.smbios.board_product);
    println!("    Board Serial:  {}", profile.smbios.board_serial);
    println!("    System UUID:   {}", profile.smbios.system_uuid);
    println!("    BIOS Vendor:   {}", profile.smbios.bios_vendor);
    println!();

    println!("  Disks ({}):", profile.disks.len());
    for disk in &profile.disks {
        println!("    [{}] {} | Serial: {} | FW: {}",
            disk.index, disk.model, disk.serial, disk.firmware_rev);
        println!("        Vol: {} | GUID: {}", disk.volume_serial, disk.volume_guid);
    }
    println!();

    println!("  Network Adapters ({}):", profile.network_adapters.len());
    for nic in &profile.network_adapters {
        println!("    {} | MAC: {}", nic.adapter_name, nic.permanent_mac);
    }
    println!();

    println!("  GPU ({}):", profile.gpus.len());
    for gpu in &profile.gpus {
        println!("    VEN_{} DEV_{} | {}", gpu.vendor_id, gpu.device_id, gpu.pnp_instance_id);
    }
    println!();

    if let Some(tpm) = &profile.tpm {
        println!("  TPM:");
        println!("    {} ({}) TPM {}", tpm.manufacturer_name, tpm.manufacturer_id, tpm.spec_version);
        println!();
    }

    println!("  Display ({}):", profile.displays.len());
    for disp in &profile.displays {
        println!("    {} | Product: {} | Serial: {} | Year: {}",
            disp.manufacturer_code, disp.product_code, disp.serial_number, disp.manufacture_year);
    }
    println!();

    println!("  Windows:");
    println!("    MachineGuid:   {}", profile.os.machine_guid);
    println!("    HwProfileGuid: {}", profile.os.hw_profile_guid);
    println!("    ProductId:     {}", profile.os.product_id);
    println!("    ComputerName:  {}", profile.os.computer_name);
    println!();

    println!("  Boot:");
    println!("    BCD GUID:       {}", profile.boot.bcd_guid);
    println!("    Disk Signature: {}", profile.boot.disk_signature);
    println!();
}
