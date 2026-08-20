use crate::profile::schema::HardwareProfile;

const PHANTOM_SMBIOS_MAGIC: u32 = 0x534D4250; // 'PBMS'
const PHANTOM_SMBIOS_VERSION: u32 = 1;
const PHANTOM_SMBIOS_MAX_STR: usize = 128;
const PHANTOM_VENDOR_GUID: &str = "{7B3E8A1C-4F2D-49A5-B1C6-8D0F3E5A72B9}";
const PHANTOM_PROFILE_VAR: &str = "PhantomProfile";
const PHANTOM_STATUS_VAR: &str = "PhantomStatus";

const DXE_STATUS_IDLE: u32 = 0;
const DXE_STATUS_APPLIED: u32 = 1;
const DXE_STATUS_ERROR: u32 = 2;

#[derive(Debug)]
pub struct FirmwareStatus {
    pub secure_boot: SecureBootState,
    pub dxe_module_installed: bool,
    pub profile_active: bool,
    pub tables_modified: u32,
    pub last_error: u32,
}

#[derive(Debug)]
pub enum SecureBootState {
    Enabled,
    Disabled,
    Unknown,
}

pub fn check_firmware() -> FirmwareStatus {
    #[cfg(windows)]
    {
        check_firmware_windows()
    }
    #[cfg(not(windows))]
    {
        FirmwareStatus {
            secure_boot: SecureBootState::Unknown,
            dxe_module_installed: false,
            profile_active: false,
            tables_modified: 0,
            last_error: 0,
        }
    }
}

pub fn format_firmware_status(status: &FirmwareStatus) -> String {
    format!(
        "Secure Boot: {} | DXE Module: {} | Profile: {} | Tables: {}",
        match status.secure_boot {
            SecureBootState::Enabled => "enabled",
            SecureBootState::Disabled => "disabled",
            SecureBootState::Unknown => "unknown",
        },
        if status.dxe_module_installed { "detected" } else { "not detected" },
        if status.profile_active { "applied" } else { "inactive" },
        status.tables_modified,
    )
}

pub fn install_dxe_module(profile: &HardwareProfile) -> Result<(), String> {
    #[cfg(windows)]
    {
        let blob = serialize_smbios_profile(profile)?;
        write_efi_variable(PHANTOM_PROFILE_VAR, &blob)?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = profile;
        Err("Layer 0 (DXE firmware) requires Windows with UEFI boot and Secure Boot disabled.".into())
    }
}

pub fn remove_dxe_module() -> Result<(), String> {
    #[cfg(windows)]
    {
        delete_efi_variable(PHANTOM_PROFILE_VAR)?;
        delete_efi_variable(PHANTOM_STATUS_VAR)?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        Err("Layer 0 (DXE firmware) requires Windows.".into())
    }
}

fn serialize_smbios_profile(profile: &HardwareProfile) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();

    push_u32(&mut buf, PHANTOM_SMBIOS_MAGIC);
    push_u32(&mut buf, PHANTOM_SMBIOS_VERSION);

    // Type 0 fields
    push_fixed_str(&mut buf, &profile.smbios.bios_vendor, PHANTOM_SMBIOS_MAX_STR);
    push_fixed_str(&mut buf, &profile.smbios.bios_version, PHANTOM_SMBIOS_MAX_STR);

    // Type 1 fields
    push_fixed_str(&mut buf, &profile.smbios.system_manufacturer, PHANTOM_SMBIOS_MAX_STR);
    push_fixed_str(&mut buf, &profile.smbios.system_product, PHANTOM_SMBIOS_MAX_STR);
    push_fixed_str(&mut buf, &profile.smbios.system_serial, PHANTOM_SMBIOS_MAX_STR);
    let uuid = parse_uuid(&profile.smbios.system_uuid)?;
    buf.extend_from_slice(&uuid);

    // Type 2 fields
    push_fixed_str(&mut buf, &profile.smbios.board_manufacturer, PHANTOM_SMBIOS_MAX_STR);
    push_fixed_str(&mut buf, &profile.smbios.board_product, PHANTOM_SMBIOS_MAX_STR);
    push_fixed_str(&mut buf, &profile.smbios.board_serial, PHANTOM_SMBIOS_MAX_STR);

    // Type 3 fields
    push_fixed_str(&mut buf, &profile.smbios.chassis_serial, PHANTOM_SMBIOS_MAX_STR);
    push_fixed_str(&mut buf, &profile.smbios.chassis_asset_tag, PHANTOM_SMBIOS_MAX_STR);

    Ok(buf)
}

fn parse_uuid(uuid_str: &str) -> Result<[u8; 16], String> {
    let hex: String = uuid_str.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if hex.len() != 32 {
        return Err(format!("Invalid UUID: {}", uuid_str));
    }

    let mut bytes = [0u8; 16];
    for i in 0..16 {
        bytes[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)
            .map_err(|e| format!("UUID parse error: {}", e))?;
    }

    // SMBIOS UUID uses mixed-endian encoding:
    // bytes 0-3: little-endian (time_low)
    // bytes 4-5: little-endian (time_mid)
    // bytes 6-7: little-endian (time_hi_and_version)
    // bytes 8-15: big-endian (clock_seq, node)
    bytes[0..4].reverse();
    bytes[4..6].reverse();
    bytes[6..8].reverse();

    Ok(bytes)
}

fn push_u32(buf: &mut Vec<u8>, val: u32) {
    buf.extend_from_slice(&val.to_le_bytes());
}

fn push_fixed_str(buf: &mut Vec<u8>, s: &str, max_len: usize) {
    let bytes = s.as_bytes();
    let copy_len = bytes.len().min(max_len);
    buf.extend_from_slice(&bytes[..copy_len]);
    buf.extend(std::iter::repeat(0u8).take(max_len - copy_len));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::engine::generate_profile;

    #[test]
    fn uuid_mixed_endian_encoding() {
        let bytes = parse_uuid("01020304-0506-0708-090A-0B0C0D0E0F10").unwrap();
        // Bytes 0-3: little-endian (reversed)
        assert_eq!(bytes[0..4], [0x04, 0x03, 0x02, 0x01]);
        // Bytes 4-5: little-endian (reversed)
        assert_eq!(bytes[4..6], [0x06, 0x05]);
        // Bytes 6-7: little-endian (reversed)
        assert_eq!(bytes[6..8], [0x08, 0x07]);
        // Bytes 8-15: big-endian (unchanged)
        assert_eq!(bytes[8..16], [0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x10]);
    }

    #[test]
    fn uuid_rejects_invalid() {
        assert!(parse_uuid("not-a-uuid").is_err());
        assert!(parse_uuid("01020304-0506-0708-090A").is_err());
    }

    #[test]
    fn smbios_profile_serialization() {
        let profile = generate_profile("smbios-ser-test", "test");
        let blob = serialize_smbios_profile(&profile).unwrap();

        let magic = u32::from_le_bytes([blob[0], blob[1], blob[2], blob[3]]);
        assert_eq!(magic, PHANTOM_SMBIOS_MAGIC);

        let version = u32::from_le_bytes([blob[4], blob[5], blob[6], blob[7]]);
        assert_eq!(version, PHANTOM_SMBIOS_VERSION);

        // 8 bytes header + 10 string fields * 128 bytes + 16 bytes UUID
        let expected_len = 8 + (10 * PHANTOM_SMBIOS_MAX_STR) + 16;
        assert_eq!(blob.len(), expected_len);
    }

    #[test]
    fn fixed_str_padding() {
        let mut buf = Vec::new();
        push_fixed_str(&mut buf, "ABC", 8);
        assert_eq!(buf.len(), 8);
        assert_eq!(&buf[0..3], b"ABC");
        assert_eq!(&buf[3..8], &[0, 0, 0, 0, 0]);
    }

    #[test]
    fn fixed_str_truncation() {
        let mut buf = Vec::new();
        push_fixed_str(&mut buf, "ABCDEFGH", 4);
        assert_eq!(buf.len(), 4);
        assert_eq!(&buf[..], b"ABCD");
    }
}

// --- Windows-specific EFI variable access ---

#[cfg(windows)]
fn check_firmware_windows() -> FirmwareStatus {
    let secure_boot = read_secure_boot_state();
    let (profile_active, tables_modified, last_error) = read_dxe_status();
    let dxe_module_installed = profile_active || read_efi_variable(PHANTOM_PROFILE_VAR).is_ok();

    FirmwareStatus {
        secure_boot,
        dxe_module_installed,
        profile_active,
        tables_modified,
        last_error,
    }
}

#[cfg(windows)]
fn read_secure_boot_state() -> SecureBootState {
    use std::ffi::CString;

    extern "system" {
        fn GetFirmwareEnvironmentVariableA(
            lpName: *const u8,
            lpGuid: *const u8,
            pBuffer: *mut u8,
            nSize: u32,
        ) -> u32;
    }

    let name = CString::new("SecureBoot").unwrap();
    let guid = CString::new("{8be4df61-93ca-11d2-aa0d-00e098032b8c}").unwrap();
    let mut value: u8 = 0;

    let result = unsafe {
        GetFirmwareEnvironmentVariableA(
            name.as_ptr() as *const u8,
            guid.as_ptr() as *const u8,
            &mut value as *mut u8,
            1,
        )
    };

    if result == 0 {
        SecureBootState::Unknown
    } else if value == 1 {
        SecureBootState::Enabled
    } else {
        SecureBootState::Disabled
    }
}

#[cfg(windows)]
fn read_dxe_status() -> (bool, u32, u32) {
    match read_efi_variable(PHANTOM_STATUS_VAR) {
        Ok(data) if data.len() >= 12 => {
            let status = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
            let tables = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
            let error = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);
            (status == DXE_STATUS_APPLIED, tables, error)
        }
        _ => (false, 0, 0),
    }
}

#[cfg(windows)]
fn read_efi_variable(name: &str) -> Result<Vec<u8>, String> {
    use std::ffi::CString;

    extern "system" {
        fn GetFirmwareEnvironmentVariableA(
            lpName: *const u8,
            lpGuid: *const u8,
            pBuffer: *mut u8,
            nSize: u32,
        ) -> u32;
    }

    let var_name = CString::new(name).map_err(|e| format!("Invalid var name: {}", e))?;
    let guid = CString::new(PHANTOM_VENDOR_GUID).map_err(|e| format!("Invalid GUID: {}", e))?;
    let mut buffer = vec![0u8; 4096];

    let bytes_read = unsafe {
        GetFirmwareEnvironmentVariableA(
            var_name.as_ptr() as *const u8,
            guid.as_ptr() as *const u8,
            buffer.as_mut_ptr(),
            buffer.len() as u32,
        )
    };

    if bytes_read == 0 {
        return Err(format!("Failed to read EFI variable '{}'", name));
    }

    buffer.truncate(bytes_read as usize);
    Ok(buffer)
}

#[cfg(windows)]
fn write_efi_variable(name: &str, data: &[u8]) -> Result<(), String> {
    use std::ffi::CString;

    extern "system" {
        fn SetFirmwareEnvironmentVariableA(
            lpName: *const u8,
            lpGuid: *const u8,
            pValue: *const u8,
            nSize: u32,
        ) -> i32;
    }

    enable_firmware_privilege()?;

    let var_name = CString::new(name).map_err(|e| format!("Invalid var name: {}", e))?;
    let guid = CString::new(PHANTOM_VENDOR_GUID).map_err(|e| format!("Invalid GUID: {}", e))?;

    let result = unsafe {
        SetFirmwareEnvironmentVariableA(
            var_name.as_ptr() as *const u8,
            guid.as_ptr() as *const u8,
            data.as_ptr(),
            data.len() as u32,
        )
    };

    if result == 0 {
        Err("Failed to write EFI variable. Requires administrator privileges and UEFI boot.".into())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn delete_efi_variable(name: &str) -> Result<(), String> {
    use std::ffi::CString;

    extern "system" {
        fn SetFirmwareEnvironmentVariableA(
            lpName: *const u8,
            lpGuid: *const u8,
            pValue: *const u8,
            nSize: u32,
        ) -> i32;
    }

    enable_firmware_privilege()?;

    let var_name = CString::new(name).map_err(|e| format!("Invalid var name: {}", e))?;
    let guid = CString::new(PHANTOM_VENDOR_GUID).map_err(|e| format!("Invalid GUID: {}", e))?;

    let result = unsafe {
        SetFirmwareEnvironmentVariableA(
            var_name.as_ptr() as *const u8,
            guid.as_ptr() as *const u8,
            std::ptr::null(),
            0,
        )
    };

    if result == 0 {
        Err(format!("Failed to delete EFI variable '{}'", name))
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn enable_firmware_privilege() -> Result<(), String> {
    use std::ffi::CString;

    extern "system" {
        fn OpenProcessToken(
            ProcessHandle: isize,
            DesiredAccess: u32,
            TokenHandle: *mut isize,
        ) -> i32;
        fn LookupPrivilegeValueA(
            lpSystemName: *const u8,
            lpName: *const u8,
            lpLuid: *mut u64,
        ) -> i32;
        fn AdjustTokenPrivileges(
            TokenHandle: isize,
            DisableAllPrivileges: i32,
            NewState: *const u8,
            BufferLength: u32,
            PreviousState: *mut u8,
            ReturnLength: *mut u32,
        ) -> i32;
        fn GetCurrentProcess() -> isize;
        fn CloseHandle(hObject: isize) -> i32;
    }

    let mut token_handle: isize = 0;
    let result = unsafe {
        OpenProcessToken(
            GetCurrentProcess(),
            0x0020 | 0x0008, // TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY
            &mut token_handle,
        )
    };
    if result == 0 {
        return Err("Failed to open process token".into());
    }

    let priv_name = CString::new("SeSystemEnvironmentPrivilege").unwrap();
    let mut luid: u64 = 0;
    let result = unsafe {
        LookupPrivilegeValueA(
            std::ptr::null(),
            priv_name.as_ptr() as *const u8,
            &mut luid,
        )
    };
    if result == 0 {
        unsafe { CloseHandle(token_handle); }
        return Err("Failed to look up firmware privilege".into());
    }

    // TOKEN_PRIVILEGES structure: Count(u32) + LUID(u64) + Attributes(u32)
    let mut tp = [0u8; 16];
    tp[0..4].copy_from_slice(&1u32.to_le_bytes()); // PrivilegeCount = 1
    tp[4..12].copy_from_slice(&luid.to_le_bytes()); // LUID
    tp[12..16].copy_from_slice(&0x00000002u32.to_le_bytes()); // SE_PRIVILEGE_ENABLED

    let result = unsafe {
        AdjustTokenPrivileges(
            token_handle,
            0, // don't disable all
            tp.as_ptr(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };

    unsafe { CloseHandle(token_handle); }

    if result == 0 {
        Err("Failed to enable firmware environment privilege. Run as Administrator.".into())
    } else {
        Ok(())
    }
}
