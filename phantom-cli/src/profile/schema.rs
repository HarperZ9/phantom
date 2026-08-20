use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareProfile {
    pub metadata: ProfileMetadata,
    pub smbios: SmbiosIdentifiers,
    pub disks: Vec<DiskIdentifiers>,
    pub network_adapters: Vec<NetworkIdentifiers>,
    pub gpus: Vec<GpuIdentifiers>,
    pub tpm: Option<TpmIdentifiers>,
    pub displays: Vec<DisplayIdentifiers>,
    pub os: OsIdentifiers,
    pub boot: BootIdentifiers,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileMetadata {
    pub name: String,
    pub seed: String,
    pub created_at: String,
    pub phantom_version: String,
    /// Signed provenance mark. Present on every profile Phantom
    /// generates starting Sprint 14. Absent on legacy or hand-authored
    /// profiles; the CLI treats the absence as a legal "unmarked"
    /// state, not as tampering.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin_mark: Option<phantom_license::watermark::OriginMark>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmbiosIdentifiers {
    pub board_serial: String,
    pub board_manufacturer: String,
    pub board_product: String,
    pub system_uuid: String,
    pub system_serial: String,
    pub system_manufacturer: String,
    pub system_product: String,
    pub chassis_serial: String,
    pub chassis_asset_tag: String,
    pub bios_vendor: String,
    pub bios_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskIdentifiers {
    pub index: u32,
    pub serial: String,
    pub model: String,
    pub firmware_rev: String,
    pub volume_serial: String,
    pub volume_guid: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkIdentifiers {
    pub adapter_name: String,
    pub permanent_mac: String,
    pub current_mac: String,
    pub adapter_guid: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuIdentifiers {
    pub vendor_id: String,
    pub device_id: String,
    pub subsystem_id: String,
    pub pnp_instance_id: String,
    pub driver_key_guid: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TpmIdentifiers {
    pub manufacturer_id: String,
    pub manufacturer_name: String,
    pub spec_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayIdentifiers {
    pub manufacturer_code: String,
    pub product_code: String,
    pub serial_number: String,
    pub manufacture_year: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsIdentifiers {
    pub machine_guid: String,
    pub hw_profile_guid: String,
    pub machine_id: String,
    pub product_id: String,
    pub computer_name: String,
    pub install_date: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootIdentifiers {
    pub bcd_guid: String,
    pub disk_signature: String,
}

impl HardwareProfile {
    pub fn identifier_count(&self) -> usize {
        let mut count = 11; // smbios fields
        count += self.disks.len() * 6;
        count += self.network_adapters.len() * 4;
        count += self.gpus.len() * 5;
        if self.tpm.is_some() {
            count += 3;
        }
        count += self.displays.len() * 4;
        count += 6; // os fields
        count += 2; // boot fields
        count
    }
}
