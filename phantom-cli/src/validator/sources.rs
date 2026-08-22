use std::collections::BTreeMap;

pub type IdentifierMap = BTreeMap<String, String>;

pub struct SourceReadResult {
    pub source_name: String,
    pub identifiers: IdentifierMap,
    pub errors: Vec<String>,
}

pub fn read_all_sources() -> Vec<SourceReadResult> {
    let mut results = Vec::new();
    results.push(read_smbios_source());
    results.push(read_registry_source());
    results.push(read_disk_source());
    results.push(read_network_source());
    results.push(read_gpu_source());
    results.push(read_display_source());
    results.push(read_tpm_source());
    results
}

/// Format a 32-hex machine-id as a dashed GUID (8-4-4-4-12), the form the
/// profile's `os.machine_guid` uses, so validate can compare them. This is the
/// inverse of the Linux backend's dash-stripping derivation.
#[cfg(any(target_os = "linux", test))]
pub(crate) fn format_machine_id_as_guid(hex: &str) -> Option<String> {
    if hex.len() != 32 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let h = hex.to_ascii_lowercase();
    Some(format!(
        "{}-{}-{}-{}-{}",
        &h[0..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..32]
    ))
}

// --- Cross-platform SMBIOS binary parser ---

#[allow(dead_code)]
struct SmbiosData {
    system_uuid: String,
    board_serial: String,
    board_manufacturer: String,
    board_product: String,
    system_serial: String,
    system_manufacturer: String,
    system_product: String,
    chassis_serial: String,
    chassis_asset_tag: String,
    bios_vendor: String,
    bios_version: String,
}

#[allow(dead_code)]
fn parse_smbios_buffer(buffer: &[u8]) -> Result<SmbiosData, String> {
    if buffer.len() < 8 {
        return Err("buffer too small for SMBIOS header".into());
    }

    let table_len = u32::from_le_bytes([buffer[4], buffer[5], buffer[6], buffer[7]]) as usize;
    let table_data = &buffer[8..];

    if table_data.len() < table_len {
        return Err("buffer truncated".into());
    }

    let mut data = SmbiosData {
        system_uuid: String::new(),
        board_serial: String::new(),
        board_manufacturer: String::new(),
        board_product: String::new(),
        system_serial: String::new(),
        system_manufacturer: String::new(),
        system_product: String::new(),
        chassis_serial: String::new(),
        chassis_asset_tag: String::new(),
        bios_vendor: String::new(),
        bios_version: String::new(),
    };

    let mut offset = 0;
    while offset + 4 <= table_data.len() {
        let entry_type = table_data[offset];
        let entry_len = table_data[offset + 1] as usize;

        if entry_type == 127 {
            break;
        }
        if entry_len < 4 || offset + entry_len > table_data.len() {
            break;
        }

        let formatted = &table_data[offset..offset + entry_len];
        let string_area = &table_data[offset + entry_len..];
        let strings = extract_smbios_strings(string_area);
        let table_end = find_string_table_end(string_area);

        match entry_type {
            0 => {
                if entry_len > 5 {
                    data.bios_vendor = get_smbios_string(&strings, formatted[4]);
                    data.bios_version = get_smbios_string(&strings, formatted[5]);
                }
            }
            1 => {
                if entry_len > 7 {
                    data.system_manufacturer = get_smbios_string(&strings, formatted[4]);
                    data.system_product = get_smbios_string(&strings, formatted[5]);
                    data.system_serial = get_smbios_string(&strings, formatted[7]);
                }
                if entry_len >= 24 {
                    data.system_uuid = format_smbios_uuid(&formatted[8..24]);
                }
            }
            2 => {
                if entry_len > 7 {
                    data.board_manufacturer = get_smbios_string(&strings, formatted[4]);
                    data.board_product = get_smbios_string(&strings, formatted[5]);
                    data.board_serial = get_smbios_string(&strings, formatted[7]);
                }
            }
            3 => {
                if entry_len > 8 {
                    data.chassis_serial = get_smbios_string(&strings, formatted[7]);
                    data.chassis_asset_tag = get_smbios_string(&strings, formatted[8]);
                }
            }
            _ => {}
        }

        offset += entry_len + table_end;
    }

    Ok(data)
}

#[allow(dead_code)]
fn extract_smbios_strings(data: &[u8]) -> Vec<String> {
    let mut strings = Vec::new();
    if data.is_empty() || data[0] == 0 {
        return strings;
    }
    let mut start = 0;
    for i in 0..data.len() {
        if data[i] == 0 {
            if start < i {
                strings.push(String::from_utf8_lossy(&data[start..i]).to_string());
            }
            if i + 1 >= data.len() || data[i + 1] == 0 {
                break;
            }
            start = i + 1;
        }
    }
    strings
}

#[allow(dead_code)]
fn find_string_table_end(data: &[u8]) -> usize {
    if data.is_empty() {
        return 0;
    }
    if data[0] == 0 {
        return if data.len() > 1 && data[1] == 0 { 2 } else { 1 };
    }
    let mut i = 0;
    while i < data.len() {
        if data[i] == 0 {
            if i + 1 >= data.len() || data[i + 1] == 0 {
                return std::cmp::min(i + 2, data.len());
            }
        }
        i += 1;
    }
    data.len()
}

#[allow(dead_code)]
fn get_smbios_string(strings: &[String], index: u8) -> String {
    if index == 0 {
        return String::new();
    }
    strings
        .get((index - 1) as usize)
        .cloned()
        .unwrap_or_default()
}

#[allow(dead_code)]
fn format_smbios_uuid(bytes: &[u8]) -> String {
    if bytes.len() < 16 {
        return String::new();
    }
    format!(
        "{:02X}{:02X}{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
        bytes[3], bytes[2], bytes[1], bytes[0],
        bytes[5], bytes[4],
        bytes[7], bytes[6],
        bytes[8], bytes[9],
        bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]
    )
}

// --- Cross-platform EDID binary parser ---

#[allow(dead_code)]
struct RawDisplayInfo {
    manufacturer_code: String,
    product_code: String,
    serial_number: String,
    manufacture_year: u16,
}

#[allow(dead_code)]
fn parse_edid(edid: &[u8]) -> Result<RawDisplayInfo, String> {
    if edid.len() < 128 {
        return Err("EDID data too short".into());
    }

    if edid[0..8] != [0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00] {
        return Err("invalid EDID header".into());
    }

    let mfr_raw = ((edid[8] as u16) << 8) | edid[9] as u16;
    let c1 = ((mfr_raw >> 10) & 0x1F) as u8 + b'A' - 1;
    let c2 = ((mfr_raw >> 5) & 0x1F) as u8 + b'A' - 1;
    let c3 = (mfr_raw & 0x1F) as u8 + b'A' - 1;
    let manufacturer_code = format!("{}{}{}", c1 as char, c2 as char, c3 as char);

    let product_code = u16::from_le_bytes([edid[10], edid[11]]);
    let product_code_str = format!("{:04X}", product_code);

    let serial_u32 = u32::from_le_bytes([edid[12], edid[13], edid[14], edid[15]]);
    let serial_number = if serial_u32 != 0 {
        format!("{}", serial_u32)
    } else {
        find_edid_descriptor_string(edid, 0xFF)
    };

    let year = edid[17] as u16 + 1990;

    Ok(RawDisplayInfo {
        manufacturer_code,
        product_code: product_code_str,
        serial_number,
        manufacture_year: year,
    })
}

#[allow(dead_code)]
fn find_edid_descriptor_string(edid: &[u8], tag: u8) -> String {
    for block_start in (54..126).step_by(18) {
        if block_start + 18 > edid.len() {
            break;
        }
        if edid[block_start] == 0
            && edid[block_start + 1] == 0
            && edid[block_start + 2] == 0
            && edid[block_start + 3] == tag
        {
            let text = &edid[block_start + 5..block_start + 18];
            let s: String = text
                .iter()
                .take_while(|&&b| b != 0x0A && b != 0x00)
                .map(|&b| b as char)
                .collect();
            return s.trim().to_string();
        }
    }
    String::new()
}

// --- Cross-platform utility ---

#[allow(dead_code)]
fn extract_pci_field(device_id: &str, prefix: &str, len: usize) -> String {
    if let Some(pos) = device_id.find(prefix) {
        let start = pos + prefix.len();
        if start + len <= device_id.len() {
            return device_id[start..start + len].to_string();
        }
    }
    String::new()
}

// --- Source readers ---

#[cfg(windows)]
fn read_smbios_source() -> SourceReadResult {
    let mut ids = IdentifierMap::new();
    let mut errors = Vec::new();

    match read_smbios_firmware_table() {
        Ok(table) => {
            ids.insert("smbios.system_uuid".into(), table.system_uuid);
            ids.insert("smbios.board_serial".into(), table.board_serial);
            ids.insert("smbios.board_manufacturer".into(), table.board_manufacturer);
            ids.insert("smbios.board_product".into(), table.board_product);
            ids.insert("smbios.system_serial".into(), table.system_serial);
            ids.insert(
                "smbios.system_manufacturer".into(),
                table.system_manufacturer,
            );
            ids.insert("smbios.system_product".into(), table.system_product);
            ids.insert("smbios.chassis_serial".into(), table.chassis_serial);
            ids.insert("smbios.chassis_asset_tag".into(), table.chassis_asset_tag);
            ids.insert("smbios.bios_vendor".into(), table.bios_vendor);
            ids.insert("smbios.bios_version".into(), table.bios_version);
        }
        Err(e) => errors.push(format!("SMBIOS read failed: {}", e)),
    }

    SourceReadResult {
        source_name: "SMBIOS Firmware Table".into(),
        identifiers: ids,
        errors,
    }
}

#[cfg(not(windows))]
fn read_smbios_source() -> SourceReadResult {
    SourceReadResult {
        source_name: "SMBIOS Firmware Table".into(),
        identifiers: IdentifierMap::new(),
        errors: vec!["SMBIOS reading requires Windows".into()],
    }
}

#[cfg(windows)]
fn read_registry_source() -> SourceReadResult {
    let mut ids = IdentifierMap::new();
    let mut errors = Vec::new();

    let string_keys = [
        (
            "os.machine_guid",
            r"SOFTWARE\Microsoft\Cryptography",
            "MachineGuid",
        ),
        (
            "os.hw_profile_guid",
            r"SYSTEM\CurrentControlSet\Control\IDConfigDB\Hardware Profiles\0001",
            "HwProfileGuid",
        ),
        (
            "os.product_id",
            r"SOFTWARE\Microsoft\Windows NT\CurrentVersion",
            "ProductId",
        ),
        (
            "os.machine_id",
            r"SOFTWARE\Microsoft\SQMClient",
            "MachineId",
        ),
    ];

    for (id_key, reg_path, value_name) in &string_keys {
        match read_registry_string(reg_path, value_name) {
            Ok(val) => {
                ids.insert((*id_key).into(), val);
            }
            Err(e) => errors.push(format!("Registry {}\\{}: {}", reg_path, value_name, e)),
        }
    }

    match read_registry_dword(
        r"SOFTWARE\Microsoft\Windows NT\CurrentVersion",
        "InstallDate",
    ) {
        Ok(val) => {
            ids.insert("os.install_date".into(), val.to_string());
        }
        Err(e) => errors.push(format!("Registry InstallDate: {}", e)),
    }

    match read_computer_name() {
        Ok(name) => {
            ids.insert("os.computer_name".into(), name);
        }
        Err(e) => errors.push(format!("ComputerName: {}", e)),
    }

    SourceReadResult {
        source_name: "Windows Registry".into(),
        identifiers: ids,
        errors,
    }
}

#[cfg(target_os = "linux")]
fn read_registry_source() -> SourceReadResult {
    let mut ids = IdentifierMap::new();
    let mut errors = Vec::new();

    // machine-id (systemd), the analog of MachineGuid. Report it in the
    // profile's dashed-GUID form so validate compares it to os.machine_guid.
    match std::fs::read_to_string("/etc/machine-id") {
        Ok(raw) => match format_machine_id_as_guid(raw.trim()) {
            Some(guid) => {
                ids.insert("os.machine_guid".into(), guid);
            }
            None => errors.push("machine-id is not 32 hex".into()),
        },
        Err(e) => errors.push(format!("/etc/machine-id: {}", e)),
    }

    // hostname, the analog of ComputerName.
    match std::fs::read_to_string("/etc/hostname") {
        Ok(raw) => {
            ids.insert("os.computer_name".into(), raw.trim().to_string());
        }
        Err(e) => errors.push(format!("/etc/hostname: {}", e)),
    }

    SourceReadResult {
        source_name: "Linux identity (machine-id, hostname)".into(),
        identifiers: ids,
        errors,
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
fn read_registry_source() -> SourceReadResult {
    SourceReadResult {
        source_name: "Windows Registry".into(),
        identifiers: IdentifierMap::new(),
        errors: vec!["Registry reading requires Windows".into()],
    }
}

#[cfg(windows)]
fn read_disk_source() -> SourceReadResult {
    let mut ids = IdentifierMap::new();
    let mut errors = Vec::new();

    match read_disk_identifiers() {
        Ok(disks) => {
            for (i, disk) in disks.iter().enumerate() {
                ids.insert(format!("disk.{}.serial", i), disk.serial.clone());
                ids.insert(format!("disk.{}.model", i), disk.model.clone());
                ids.insert(
                    format!("disk.{}.firmware_rev", i),
                    disk.firmware_rev.clone(),
                );
            }
        }
        Err(e) => errors.push(format!("Disk enumeration: {}", e)),
    }

    match read_volume_serial() {
        Ok(serial) => {
            ids.insert("disk.0.volume_serial".into(), serial);
        }
        Err(e) => errors.push(format!("Volume serial: {}", e)),
    }

    match read_volume_guid() {
        Ok(guid) => {
            ids.insert("disk.0.volume_guid".into(), guid);
        }
        Err(e) => errors.push(format!("Volume GUID: {}", e)),
    }

    SourceReadResult {
        source_name: "Disk Identifiers".into(),
        identifiers: ids,
        errors,
    }
}

#[cfg(not(windows))]
fn read_disk_source() -> SourceReadResult {
    SourceReadResult {
        source_name: "Disk Identifiers".into(),
        identifiers: IdentifierMap::new(),
        errors: vec!["Disk identifier reading requires Windows".into()],
    }
}

#[cfg(windows)]
fn read_network_source() -> SourceReadResult {
    let mut ids = IdentifierMap::new();
    let mut errors = Vec::new();

    match read_network_adapters() {
        Ok(adapters) => {
            for (i, adapter) in adapters.iter().enumerate() {
                ids.insert(
                    format!("nic.{}.permanent_mac", i),
                    adapter.permanent_mac.clone(),
                );
                ids.insert(
                    format!("nic.{}.current_mac", i),
                    adapter.current_mac.clone(),
                );
                ids.insert(
                    format!("nic.{}.adapter_guid", i),
                    adapter.adapter_guid.clone(),
                );
            }
        }
        Err(e) => errors.push(format!("Network enumeration: {}", e)),
    }

    SourceReadResult {
        source_name: "Network Adapters".into(),
        identifiers: ids,
        errors,
    }
}

#[cfg(target_os = "linux")]
fn read_network_source() -> SourceReadResult {
    let mut ids = IdentifierMap::new();
    let mut errors = Vec::new();

    // Same physical interfaces, in the same sorted order, that the apply backend
    // spoofs, so index i here is nic.i in the profile. Report the MAC uppercased
    // to match the generated profile's format.
    for (i, iface) in crate::apply::userland_linux::physical_interfaces()
        .iter()
        .enumerate()
    {
        match std::fs::read_to_string(format!("/sys/class/net/{}/address", iface)) {
            Ok(raw) => {
                ids.insert(
                    format!("nic.{}.current_mac", i),
                    raw.trim().to_ascii_uppercase(),
                );
            }
            Err(e) => errors.push(format!("{} MAC: {}", iface, e)),
        }
    }

    SourceReadResult {
        source_name: "Network Adapters".into(),
        identifiers: ids,
        errors,
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
fn read_network_source() -> SourceReadResult {
    SourceReadResult {
        source_name: "Network Adapters".into(),
        identifiers: IdentifierMap::new(),
        errors: vec!["Network adapter reading requires Windows".into()],
    }
}

#[cfg(windows)]
fn read_gpu_source() -> SourceReadResult {
    let mut ids = IdentifierMap::new();
    let mut errors = Vec::new();

    match read_gpu_devices() {
        Ok(gpus) => {
            for (i, gpu) in gpus.iter().enumerate() {
                ids.insert(format!("gpu.{}.vendor_id", i), gpu.vendor_id.clone());
                ids.insert(format!("gpu.{}.device_id", i), gpu.device_id.clone());
                ids.insert(format!("gpu.{}.subsystem_id", i), gpu.subsystem_id.clone());
                ids.insert(
                    format!("gpu.{}.pnp_instance_id", i),
                    gpu.pnp_instance_id.clone(),
                );
                ids.insert(
                    format!("gpu.{}.driver_key_guid", i),
                    gpu.driver_key_guid.clone(),
                );
            }
        }
        Err(e) => errors.push(format!("GPU enumeration: {}", e)),
    }

    SourceReadResult {
        source_name: "GPU Devices".into(),
        identifiers: ids,
        errors,
    }
}

#[cfg(not(windows))]
fn read_gpu_source() -> SourceReadResult {
    SourceReadResult {
        source_name: "GPU Devices".into(),
        identifiers: IdentifierMap::new(),
        errors: vec!["GPU reading requires Windows".into()],
    }
}

#[cfg(windows)]
fn read_display_source() -> SourceReadResult {
    let mut ids = IdentifierMap::new();
    let mut errors = Vec::new();

    match read_display_devices() {
        Ok(displays) => {
            for (i, display) in displays.iter().enumerate() {
                ids.insert(
                    format!("display.{}.manufacturer_code", i),
                    display.manufacturer_code.clone(),
                );
                ids.insert(
                    format!("display.{}.product_code", i),
                    display.product_code.clone(),
                );
                ids.insert(
                    format!("display.{}.serial_number", i),
                    display.serial_number.clone(),
                );
                ids.insert(
                    format!("display.{}.manufacture_year", i),
                    display.manufacture_year.to_string(),
                );
            }
        }
        Err(e) => errors.push(format!("Display enumeration: {}", e)),
    }

    SourceReadResult {
        source_name: "Display EDID".into(),
        identifiers: ids,
        errors,
    }
}

#[cfg(not(windows))]
fn read_display_source() -> SourceReadResult {
    SourceReadResult {
        source_name: "Display EDID".into(),
        identifiers: IdentifierMap::new(),
        errors: vec!["Display EDID reading requires Windows".into()],
    }
}

#[cfg(windows)]
fn read_tpm_source() -> SourceReadResult {
    let mut ids = IdentifierMap::new();
    let mut errors = Vec::new();

    match read_tpm_info() {
        Ok(tpm) => {
            ids.insert("tpm.manufacturer_id".into(), tpm.manufacturer_id);
            ids.insert("tpm.manufacturer_name".into(), tpm.manufacturer_name);
            ids.insert("tpm.spec_version".into(), tpm.spec_version);
        }
        Err(e) => errors.push(format!("TPM: {}", e)),
    }

    SourceReadResult {
        source_name: "TPM Module".into(),
        identifiers: ids,
        errors,
    }
}

#[cfg(not(windows))]
fn read_tpm_source() -> SourceReadResult {
    SourceReadResult {
        source_name: "TPM Module".into(),
        identifiers: IdentifierMap::new(),
        errors: vec!["TPM reading requires Windows".into()],
    }
}

// --- Windows-only FFI declarations ---

#[cfg(windows)]
extern "system" {
    fn GetSystemFirmwareTable(
        FirmwareTableProviderSignature: u32,
        FirmwareTableID: u32,
        pFirmwareTableBuffer: *mut std::ffi::c_void,
        BufferSize: u32,
    ) -> u32;

    fn CreateFileA(
        lpFileName: *const u8,
        dwDesiredAccess: u32,
        dwShareMode: u32,
        lpSecurityAttributes: *const u8,
        dwCreationDisposition: u32,
        dwFlagsAndAttributes: u32,
        hTemplateFile: isize,
    ) -> isize;

    fn DeviceIoControl(
        hDevice: isize,
        dwIoControlCode: u32,
        lpInBuffer: *const u8,
        nInBufferSize: u32,
        lpOutBuffer: *mut u8,
        nOutBufferSize: u32,
        lpBytesReturned: *mut u32,
        lpOverlapped: *mut u8,
    ) -> i32;

    fn CloseHandle(hObject: isize) -> i32;

    fn GetVolumeInformationA(
        lpRootPathName: *const u8,
        lpVolumeNameBuffer: *mut u8,
        nVolumeNameSize: u32,
        lpVolumeSerialNumber: *mut u32,
        lpMaximumComponentLength: *mut u32,
        lpFileSystemFlags: *mut u32,
        lpFileSystemNameBuffer: *mut u8,
        nFileSystemNameSize: u32,
    ) -> i32;

    fn GetVolumeNameForVolumeMountPointA(
        lpszVolumeMountPoint: *const u8,
        lpszVolumeName: *mut u8,
        cchBufferLength: u32,
    ) -> i32;
}

#[cfg(windows)]
const IOCTL_STORAGE_QUERY_PROPERTY: u32 = 0x002D1400;
#[cfg(windows)]
const INVALID_HANDLE_VALUE: isize = -1;
#[cfg(windows)]
const OPEN_EXISTING: u32 = 3;
#[cfg(windows)]
const FILE_SHARE_READ: u32 = 1;
#[cfg(windows)]
const FILE_SHARE_WRITE: u32 = 2;

#[cfg(windows)]
#[repr(C)]
struct StoragePropertyQuery {
    property_id: u32,
    query_type: u32,
    additional: [u8; 1],
}

#[cfg(windows)]
#[repr(C)]
struct StorageDeviceDescriptor {
    version: u32,
    size: u32,
    device_type: u8,
    device_type_modifier: u8,
    removable_media: u8,
    command_queueing: u8,
    vendor_id_offset: u32,
    product_id_offset: u32,
    product_revision_offset: u32,
    serial_number_offset: u32,
    bus_type: u32,
    raw_properties_length: u32,
    raw_device_properties: [u8; 1],
}

// --- Windows SMBIOS reader ---

#[cfg(windows)]
fn read_smbios_firmware_table() -> Result<SmbiosData, String> {
    use std::ptr;

    unsafe {
        let size = GetSystemFirmwareTable(u32::from_be_bytes(*b"RSMB"), 0, ptr::null_mut(), 0);
        if size == 0 {
            return Err("GetSystemFirmwareTable returned 0".into());
        }

        let mut buffer = vec![0u8; size as usize];
        let written = GetSystemFirmwareTable(
            u32::from_be_bytes(*b"RSMB"),
            0,
            buffer.as_mut_ptr() as *mut _,
            size,
        );
        if written != size {
            return Err("GetSystemFirmwareTable incomplete read".into());
        }

        parse_smbios_buffer(&buffer)
    }
}

// --- Windows disk reader ---

#[cfg(windows)]
struct RawDiskInfo {
    serial: String,
    model: String,
    firmware_rev: String,
}

#[cfg(windows)]
fn read_descriptor_string(buf: &[u8], offset: u32) -> String {
    if offset == 0 || offset as usize >= buf.len() {
        return String::new();
    }
    let start = offset as usize;
    let end = buf[start..]
        .iter()
        .position(|&b| b == 0)
        .map(|p| start + p)
        .unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[start..end]).trim().to_string()
}

#[cfg(windows)]
fn read_disk_identifiers() -> Result<Vec<RawDiskInfo>, String> {
    let mut disks = Vec::new();

    for i in 0..16u32 {
        let path = format!("\\\\.\\PhysicalDrive{}\0", i);
        let handle = unsafe {
            CreateFileA(
                path.as_ptr(),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                0,
            )
        };

        if handle == INVALID_HANDLE_VALUE {
            continue;
        }

        let query = StoragePropertyQuery {
            property_id: 0,
            query_type: 0,
            additional: [0],
        };

        let mut buf = vec![0u8; 1024];
        let mut returned: u32 = 0;

        let ok = unsafe {
            DeviceIoControl(
                handle,
                IOCTL_STORAGE_QUERY_PROPERTY,
                &query as *const _ as *const u8,
                std::mem::size_of::<StoragePropertyQuery>() as u32,
                buf.as_mut_ptr(),
                buf.len() as u32,
                &mut returned,
                std::ptr::null_mut(),
            )
        };

        unsafe { CloseHandle(handle) };

        if ok != 0 && returned as usize >= std::mem::size_of::<StorageDeviceDescriptor>() {
            let desc = unsafe { &*(buf.as_ptr() as *const StorageDeviceDescriptor) };
            disks.push(RawDiskInfo {
                serial: read_descriptor_string(&buf, desc.serial_number_offset),
                model: read_descriptor_string(&buf, desc.product_id_offset),
                firmware_rev: read_descriptor_string(&buf, desc.product_revision_offset),
            });
        }
    }

    if disks.is_empty() {
        Err("no physical drives found".into())
    } else {
        Ok(disks)
    }
}

#[cfg(windows)]
fn read_volume_serial() -> Result<String, String> {
    let mut serial: u32 = 0;
    let ok = unsafe {
        GetVolumeInformationA(
            b"C:\\\0".as_ptr(),
            std::ptr::null_mut(),
            0,
            &mut serial,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            0,
        )
    };
    if ok != 0 {
        Ok(format!("{:08X}", serial))
    } else {
        Err("GetVolumeInformationA failed".into())
    }
}

#[cfg(windows)]
fn read_volume_guid() -> Result<String, String> {
    let mut buf = vec![0u8; 50];
    let ok = unsafe {
        GetVolumeNameForVolumeMountPointA(b"C:\\\0".as_ptr(), buf.as_mut_ptr(), buf.len() as u32)
    };
    if ok != 0 {
        let s = String::from_utf8_lossy(&buf);
        let s = s.trim_end_matches('\0');
        if let Some(start) = s.find('{') {
            if let Some(end) = s.find('}') {
                return Ok(s[start..=end].to_string());
            }
        }
        Err("unexpected volume GUID format".into())
    } else {
        Err("GetVolumeNameForVolumeMountPointA failed".into())
    }
}

// --- Windows network reader ---

#[cfg(windows)]
struct RawNetworkInfo {
    permanent_mac: String,
    current_mac: String,
    adapter_guid: String,
}

#[cfg(windows)]
fn read_network_adapters() -> Result<Vec<RawNetworkInfo>, String> {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    use winreg::RegKey;

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let class_key = hklm
        .open_subkey(
            r"SYSTEM\CurrentControlSet\Control\Class\{4d36e972-e325-11ce-bfc1-08002be10318}",
        )
        .map_err(|e| format!("cannot open network class key: {}", e))?;

    let mut adapters = Vec::new();

    for name in class_key.enum_keys().filter_map(|k| k.ok()) {
        if name == "Properties" {
            continue;
        }
        let subkey = match class_key.open_subkey(&name) {
            Ok(k) => k,
            Err(_) => continue,
        };

        let guid: String = match subkey.get_value("NetCfgInstanceId") {
            Ok(v) => v,
            Err(_) => continue,
        };

        let component_id: String = subkey.get_value("ComponentId").unwrap_or_default();
        let lower = component_id.to_lowercase();
        if lower.contains("vmware")
            || lower.contains("virtual")
            || lower.contains("vpn")
            || lower.contains("tunnel")
            || lower.contains("loopback")
        {
            continue;
        }

        let mac: String = subkey.get_value("NetworkAddress").unwrap_or_default();

        if !mac.is_empty() || lower.contains("pci") || lower.contains("usb") {
            adapters.push(RawNetworkInfo {
                permanent_mac: mac.clone(),
                current_mac: mac,
                adapter_guid: guid,
            });
        }
    }

    Ok(adapters)
}

// --- Windows GPU reader ---

#[cfg(windows)]
struct RawGpuInfo {
    vendor_id: String,
    device_id: String,
    subsystem_id: String,
    pnp_instance_id: String,
    driver_key_guid: String,
}

#[cfg(windows)]
fn read_gpu_devices() -> Result<Vec<RawGpuInfo>, String> {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    use winreg::RegKey;

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let enum_key = hklm
        .open_subkey(r"SYSTEM\CurrentControlSet\Enum\PCI")
        .map_err(|e| format!("cannot open PCI enum: {}", e))?;

    let mut gpus = Vec::new();

    for device_name in enum_key.enum_keys().filter_map(|k| k.ok()) {
        if !device_name.contains("VEN_") {
            continue;
        }

        let device_key = match enum_key.open_subkey(&device_name) {
            Ok(k) => k,
            Err(_) => continue,
        };

        for instance_name in device_key.enum_keys().filter_map(|k| k.ok()) {
            let instance = match device_key.open_subkey(&instance_name) {
                Ok(k) => k,
                Err(_) => continue,
            };

            let class: String = instance.get_value("Class").unwrap_or_default();
            if class != "Display" {
                continue;
            }

            let vendor_id = extract_pci_field(&device_name, "VEN_", 4);
            let device_id = extract_pci_field(&device_name, "DEV_", 4);
            let subsystem_id = extract_pci_field(&device_name, "SUBSYS_", 8);

            let pnp_instance_id = format!("PCI\\{}\\{}", device_name, instance_name);
            let driver_key: String = instance.get_value("Driver").unwrap_or_default();

            gpus.push(RawGpuInfo {
                vendor_id,
                device_id,
                subsystem_id,
                pnp_instance_id,
                driver_key_guid: driver_key,
            });
        }
    }

    Ok(gpus)
}

// --- Windows display EDID reader ---

#[cfg(windows)]
fn read_display_devices() -> Result<Vec<RawDisplayInfo>, String> {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    use winreg::RegKey;

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let display_key = hklm
        .open_subkey(r"SYSTEM\CurrentControlSet\Enum\DISPLAY")
        .map_err(|e| format!("cannot open DISPLAY enum: {}", e))?;

    let mut displays = Vec::new();

    for monitor_name in display_key.enum_keys().filter_map(|k| k.ok()) {
        let monitor_key = match display_key.open_subkey(&monitor_name) {
            Ok(k) => k,
            Err(_) => continue,
        };

        for instance_name in monitor_key.enum_keys().filter_map(|k| k.ok()) {
            let instance = match monitor_key.open_subkey(&instance_name) {
                Ok(k) => k,
                Err(_) => continue,
            };

            let params = match instance.open_subkey("Device Parameters") {
                Ok(k) => k,
                Err(_) => continue,
            };

            let edid: Vec<u8> = match params.get_raw_value("EDID") {
                Ok(v) => v.bytes,
                Err(_) => continue,
            };

            if let Ok(info) = parse_edid(&edid) {
                displays.push(info);
            }
        }
    }

    Ok(displays)
}

// --- Windows TPM reader ---

#[cfg(windows)]
struct RawTpmInfo {
    manufacturer_id: String,
    manufacturer_name: String,
    spec_version: String,
}

#[cfg(windows)]
fn read_tpm_info() -> Result<RawTpmInfo, String> {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    use winreg::RegKey;

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);

    let tpm_key = hklm
        .open_subkey(r"SYSTEM\CurrentControlSet\Services\TPM\WMI")
        .or_else(|_| hklm.open_subkey(r"SOFTWARE\Microsoft\Tpm"))
        .map_err(|e| format!("cannot open TPM key: {}", e))?;

    let manufacturer_id: u32 = tpm_key.get_value("ManufacturerId").unwrap_or(0);
    let manufacturer_id_str = if manufacturer_id != 0 {
        let bytes = manufacturer_id.to_be_bytes();
        String::from_utf8_lossy(&bytes)
            .trim_end_matches('\0')
            .to_string()
    } else {
        tpm_key
            .get_value::<String, _>("ManufacturerIdTxt")
            .unwrap_or_default()
    };

    let manufacturer_name: String = tpm_key
        .get_value("ManufacturerDisplayName")
        .or_else(|_| tpm_key.get_value("ManufacturerVersion"))
        .unwrap_or_default();

    let spec_version: String = tpm_key.get_value("SpecVersion").unwrap_or_default();

    Ok(RawTpmInfo {
        manufacturer_id: manufacturer_id_str,
        manufacturer_name,
        spec_version,
    })
}

// --- Windows registry helpers ---

#[cfg(windows)]
fn read_registry_string(path: &str, name: &str) -> Result<String, String> {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    use winreg::RegKey;

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let key = hklm.open_subkey(path).map_err(|e| e.to_string())?;
    key.get_value::<String, _>(name).map_err(|e| e.to_string())
}

#[cfg(windows)]
fn read_registry_dword(path: &str, name: &str) -> Result<u32, String> {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    use winreg::RegKey;

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let key = hklm.open_subkey(path).map_err(|e| e.to_string())?;
    key.get_value::<u32, _>(name).map_err(|e| e.to_string())
}

#[cfg(windows)]
fn read_computer_name() -> Result<String, String> {
    std::env::var("COMPUTERNAME").map_err(|e| e.to_string())
}

// --- Tests ---

#[cfg(test)]
mod tests {
    use super::*;

    fn build_smbios_entry(entry_type: u8, data: &[u8], strings: &[&str]) -> Vec<u8> {
        let header_len = 4 + data.len();
        let mut entry = Vec::new();
        entry.push(entry_type);
        entry.push(header_len as u8);
        entry.push(0x00);
        entry.push(0x00);
        entry.extend_from_slice(data);
        for s in strings {
            entry.extend_from_slice(s.as_bytes());
            entry.push(0);
        }
        entry.push(0);
        entry
    }

    fn build_smbios_buffer(entries: &[Vec<u8>]) -> Vec<u8> {
        let mut table = Vec::new();
        for e in entries {
            table.extend_from_slice(e);
        }
        let table_len = table.len() as u32;
        let mut buffer = vec![0u8; 8];
        buffer[1] = 3;
        buffer[4..8].copy_from_slice(&table_len.to_le_bytes());
        buffer.extend_from_slice(&table);
        buffer
    }

    #[test]
    fn format_machine_id_as_guid_round_trips_and_validates() {
        // A generated profile's GUID, stripped of dashes and reformatted, comes
        // back byte for byte, so a live machine-id validates against the profile.
        let profile = crate::profile::engine::generate_profile("audit-seed", "test");
        let guid = &profile.os.machine_guid;
        let hex: String = guid.chars().filter(|c| *c != '-').collect();
        assert_eq!(
            format_machine_id_as_guid(&hex).as_deref(),
            Some(guid.as_str())
        );
        assert_eq!(format_machine_id_as_guid("abc"), None);
        assert_eq!(format_machine_id_as_guid(&"z".repeat(32)), None);
    }

    #[test]
    fn smbios_parser_bios_entry() {
        let bios = build_smbios_entry(0, &[1, 2], &["TestVendor", "1.0.0"]);
        let end = build_smbios_entry(127, &[], &[]);
        let buf = build_smbios_buffer(&[bios, end]);

        let data = parse_smbios_buffer(&buf).unwrap();
        assert_eq!(data.bios_vendor, "TestVendor");
        assert_eq!(data.bios_version, "1.0.0");
    }

    #[test]
    fn smbios_parser_system_entry() {
        let uuid_bytes: [u8; 16] = [
            0x78, 0x56, 0x34, 0x12, 0xBC, 0x9A, 0xF0, 0xDE, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66,
            0x77, 0x88,
        ];
        let mut data_bytes: Vec<u8> = vec![1, 2, 3, 4];
        data_bytes.extend_from_slice(&uuid_bytes);

        let system = build_smbios_entry(1, &data_bytes, &["SysMfr", "SysProduct", "v1", "SN123"]);
        let end = build_smbios_entry(127, &[], &[]);
        let buf = build_smbios_buffer(&[system, end]);

        let parsed = parse_smbios_buffer(&buf).unwrap();
        assert_eq!(parsed.system_manufacturer, "SysMfr");
        assert_eq!(parsed.system_product, "SysProduct");
        assert_eq!(parsed.system_serial, "SN123");
        assert_eq!(parsed.system_uuid, "12345678-9ABC-DEF0-1122-334455667788");
    }

    #[test]
    fn smbios_parser_baseboard_entry() {
        let board = build_smbios_entry(2, &[1, 2, 3, 4], &["BoardMfr", "BoardProd", "v2", "BSRL"]);
        let end = build_smbios_entry(127, &[], &[]);
        let buf = build_smbios_buffer(&[board, end]);

        let parsed = parse_smbios_buffer(&buf).unwrap();
        assert_eq!(parsed.board_manufacturer, "BoardMfr");
        assert_eq!(parsed.board_product, "BoardProd");
        assert_eq!(parsed.board_serial, "BSRL");
    }

    #[test]
    fn smbios_parser_chassis_entry() {
        let chassis = build_smbios_entry(
            3,
            &[1, 2, 3, 4, 5],
            &["ChassisMfr", "t", "v3", "CSRL", "ASSET1"],
        );
        let end = build_smbios_entry(127, &[], &[]);
        let buf = build_smbios_buffer(&[chassis, end]);

        let parsed = parse_smbios_buffer(&buf).unwrap();
        assert_eq!(parsed.chassis_serial, "CSRL");
        assert_eq!(parsed.chassis_asset_tag, "ASSET1");
    }

    #[test]
    fn smbios_parser_multiple_entries() {
        let bios = build_smbios_entry(0, &[1, 2], &["BIOS Corp", "2.0"]);
        let mut sys_data: Vec<u8> = vec![1, 2, 3, 4];
        sys_data.extend_from_slice(&[0u8; 16]);
        let system = build_smbios_entry(1, &sys_data, &["Dell", "PowerEdge", "v1", "ABC123"]);
        let board = build_smbios_entry(2, &[1, 2, 3, 4], &["Dell", "0XYZ", "v2", "BSN456"]);
        let chassis = build_smbios_entry(
            3,
            &[1, 2, 3, 4, 5],
            &["Dell", "t", "v3", "CSN789", "ASSET0"],
        );
        let end = build_smbios_entry(127, &[], &[]);
        let buf = build_smbios_buffer(&[bios, system, board, chassis, end]);

        let parsed = parse_smbios_buffer(&buf).unwrap();
        assert_eq!(parsed.bios_vendor, "BIOS Corp");
        assert_eq!(parsed.bios_version, "2.0");
        assert_eq!(parsed.system_manufacturer, "Dell");
        assert_eq!(parsed.system_product, "PowerEdge");
        assert_eq!(parsed.system_serial, "ABC123");
        assert_eq!(parsed.board_manufacturer, "Dell");
        assert_eq!(parsed.board_product, "0XYZ");
        assert_eq!(parsed.board_serial, "BSN456");
        assert_eq!(parsed.chassis_serial, "CSN789");
        assert_eq!(parsed.chassis_asset_tag, "ASSET0");
    }

    #[test]
    fn smbios_uuid_mixed_endian() {
        let bytes: [u8; 16] = [
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E,
            0x0F, 0x10,
        ];
        let uuid = format_smbios_uuid(&bytes);
        assert_eq!(uuid, "04030201-0605-0807-090A-0B0C0D0E0F10");
    }

    #[test]
    fn smbios_string_extraction() {
        let data = b"Hello\0World\0\0";
        let strings = extract_smbios_strings(data);
        assert_eq!(strings, vec!["Hello", "World"]);

        let empty = b"\0\0";
        let strings = extract_smbios_strings(empty);
        assert!(strings.is_empty());
    }

    #[test]
    fn smbios_string_table_end() {
        let data = b"ABC\0DEF\0\0rest";
        let end = find_string_table_end(data);
        assert_eq!(end, 9);

        let empty = b"\0\0";
        let end = find_string_table_end(empty);
        assert_eq!(end, 2);
    }

    #[test]
    fn edid_parser_basic() {
        let mut edid = vec![0u8; 128];
        edid[0] = 0x00;
        edid[1..7].copy_from_slice(&[0xFF; 6]);
        edid[7] = 0x00;

        // DEL: D=4, E=5, L=12 → (4<<10)|(5<<5)|12 = 0x10AC
        edid[8] = 0x10;
        edid[9] = 0xAC;

        edid[10] = 0x34;
        edid[11] = 0x12;

        let serial: u32 = 12345678;
        edid[12..16].copy_from_slice(&serial.to_le_bytes());

        edid[17] = 30; // 2020

        let result = parse_edid(&edid).unwrap();
        assert_eq!(result.manufacturer_code, "DEL");
        assert_eq!(result.product_code, "1234");
        assert_eq!(result.serial_number, "12345678");
        assert_eq!(result.manufacture_year, 2020);
    }

    #[test]
    fn edid_descriptor_serial() {
        let mut edid = vec![0u8; 128];
        edid[0] = 0x00;
        edid[1..7].copy_from_slice(&[0xFF; 6]);
        edid[7] = 0x00;
        edid[8] = 0x10;
        edid[9] = 0xAC;
        edid[10] = 0x34;
        edid[11] = 0x12;
        edid[17] = 30;

        edid[54] = 0x00;
        edid[55] = 0x00;
        edid[56] = 0x00;
        edid[57] = 0xFF;
        edid[58] = 0x00;
        let serial = b"XYZ789ABC\n   ";
        edid[59..72].copy_from_slice(serial);

        let result = parse_edid(&edid).unwrap();
        assert_eq!(result.serial_number, "XYZ789ABC");
    }

    #[test]
    fn pci_field_extraction() {
        let id = "VEN_10DE&DEV_1B80&SUBSYS_11803842&REV_A1";
        assert_eq!(extract_pci_field(id, "VEN_", 4), "10DE");
        assert_eq!(extract_pci_field(id, "DEV_", 4), "1B80");
        assert_eq!(extract_pci_field(id, "SUBSYS_", 8), "11803842");
        assert_eq!(extract_pci_field(id, "REV_", 2), "A1");
        assert_eq!(extract_pci_field(id, "MISSING_", 4), "");
    }

    #[test]
    fn read_all_sources_returns_seven() {
        let results = read_all_sources();
        assert_eq!(results.len(), 7);
    }

    #[test]
    fn non_windows_sources_report_errors() {
        // On a platform with no source backend at all, every source reports an
        // error rather than silently returning nothing.
        #[cfg(all(not(windows), not(target_os = "linux")))]
        {
            let results = read_all_sources();
            for result in &results {
                assert!(
                    !result.errors.is_empty(),
                    "source '{}' should have errors on this platform",
                    result.source_name
                );
            }
        }
        // On Linux the identity (machine-id, hostname) and network sources are
        // implemented; the five deeper sources (SMBIOS, disk, GPU, display, TPM)
        // are still stubs and report errors.
        #[cfg(target_os = "linux")]
        {
            let results = read_all_sources();
            let with_errors = results.iter().filter(|r| !r.errors.is_empty()).count();
            assert!(
                with_errors >= 5,
                "expected the five deeper sources to still report errors, saw {}",
                with_errors
            );
        }
    }
}
