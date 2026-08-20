use crate::profile::schema::HardwareProfile;

const PHANTOM_PROFILE_MAGIC: u32 = 0x544E4850; // 'PHNT'
const MAX_SERIAL_LEN: usize = 64;
const MAX_MODEL_LEN: usize = 128;
const MAX_DISKS: usize = 8;
const MAX_NICS: usize = 4;
const MAX_GPUS: usize = 2;
const MAX_DISPLAYS: usize = 4;

#[derive(Debug)]
pub struct DriverStatus {
    pub loaded: bool,
    pub version: Option<String>,
    pub profile_active: bool,
    pub attached_disk_count: u32,
    pub attached_nic_count: u32,
    pub attached_gpu_count: u32,
    pub intercepted_ioctl_count: u32,
}

pub fn check_driver() -> DriverStatus {
    #[cfg(windows)]
    {
        check_driver_windows()
    }
    #[cfg(not(windows))]
    {
        DriverStatus {
            loaded: false,
            version: None,
            profile_active: false,
            attached_disk_count: 0,
            attached_nic_count: 0,
            attached_gpu_count: 0,
            intercepted_ioctl_count: 0,
        }
    }
}

pub fn send_profile_to_driver(profile: &HardwareProfile) -> Result<(), String> {
    #[cfg(windows)]
    {
        let blob = serialize_kernel_profile(profile)?;
        send_ioctl(IOCTL_PHANTOM_SET_PROFILE, &blob)
    }
    #[cfg(not(windows))]
    {
        let _ = profile;
        Err("Kernel driver (Layer 1) requires Windows.".into())
    }
}

pub fn clear_driver_profile() -> Result<(), String> {
    #[cfg(windows)]
    {
        send_ioctl(IOCTL_PHANTOM_CLEAR_PROFILE, &[])
    }
    #[cfg(not(windows))]
    {
        Err("Kernel driver (Layer 1) requires Windows.".into())
    }
}

pub fn format_driver_status(status: &DriverStatus) -> String {
    if !status.loaded {
        return "Driver: not loaded".into();
    }
    format!(
        "Driver: loaded | Profile: {} | Filters: {} disk, {} nic, {} gpu | Intercepted: {}",
        if status.profile_active { "active" } else { "inactive" },
        status.attached_disk_count,
        status.attached_nic_count,
        status.attached_gpu_count,
        status.intercepted_ioctl_count,
    )
}

/// Serialize a HardwareProfile into the packed binary format the kernel driver expects.
fn serialize_kernel_profile(profile: &HardwareProfile) -> Result<Vec<u8>, String> {
    let disk_count = profile.disks.len().min(MAX_DISKS);
    let nic_count = profile.network_adapters.len().min(MAX_NICS);
    let gpu_count = profile.gpus.len().min(MAX_GPUS);

    let mut buf = Vec::new();

    let has_tpm: u32 = if profile.tpm.is_some() { 1 } else { 0 };
    let display_count = profile.displays.len().min(MAX_DISPLAYS);

    push_u32(&mut buf, PHANTOM_PROFILE_MAGIC);
    push_u32(&mut buf, 1); // version
    push_u32(&mut buf, disk_count as u32);
    push_u32(&mut buf, nic_count as u32);
    push_u32(&mut buf, gpu_count as u32);
    push_u32(&mut buf, has_tpm);
    push_u32(&mut buf, display_count as u32);

    for i in 0..MAX_DISKS {
        if i < disk_count {
            let disk = &profile.disks[i];
            push_u32(&mut buf, disk.index);
            push_fixed_str(&mut buf, &disk.serial, MAX_SERIAL_LEN);
            push_u32(&mut buf, disk.serial.len().min(MAX_SERIAL_LEN) as u32);
            push_fixed_str(&mut buf, &disk.model, MAX_MODEL_LEN);
            push_u32(&mut buf, disk.model.len().min(MAX_MODEL_LEN) as u32);
            push_fixed_str(&mut buf, &disk.firmware_rev, MAX_SERIAL_LEN);
            push_u32(&mut buf, disk.firmware_rev.len().min(MAX_SERIAL_LEN) as u32);
        } else {
            let empty_disk_size = 4 + MAX_SERIAL_LEN + 4 + MAX_MODEL_LEN + 4 + MAX_SERIAL_LEN + 4;
            buf.extend(std::iter::repeat(0u8).take(empty_disk_size));
        }
    }

    for i in 0..MAX_NICS {
        if i < nic_count {
            let nic = &profile.network_adapters[i];
            let mac = parse_mac(&nic.permanent_mac)?;
            buf.extend_from_slice(&mac);
            let cmac = parse_mac(&nic.current_mac)?;
            buf.extend_from_slice(&cmac);
        } else {
            buf.extend(std::iter::repeat(0u8).take(12));
        }
    }

    for i in 0..MAX_GPUS {
        if i < gpu_count {
            let gpu = &profile.gpus[i];
            let vid = u16::from_str_radix(&gpu.vendor_id, 16)
                .map_err(|e| format!("Invalid vendor_id: {}", e))?;
            let did = u16::from_str_radix(&gpu.device_id, 16)
                .map_err(|e| format!("Invalid device_id: {}", e))?;
            let subsys = u32::from_str_radix(&gpu.subsystem_id, 16)
                .map_err(|e| format!("Invalid subsystem_id: {}", e))?;
            push_u16(&mut buf, vid);
            push_u16(&mut buf, did);
            push_u32(&mut buf, subsys);
            push_fixed_str(&mut buf, &gpu.pnp_instance_id, MAX_MODEL_LEN);
            push_u32(&mut buf, gpu.pnp_instance_id.len().min(MAX_MODEL_LEN) as u32);
        } else {
            let empty_gpu_size = 2 + 2 + 4 + MAX_MODEL_LEN + 4;
            buf.extend(std::iter::repeat(0u8).take(empty_gpu_size));
        }
    }

    // TPM profile: ManufacturerId[4]
    if let Some(ref tpm) = profile.tpm {
        push_fixed_str(&mut buf, &tpm.manufacturer_id, 4);
    } else {
        buf.extend(std::iter::repeat(0u8).take(4));
    }

    // Display profiles: ManufacturerCode[3] + Padding[1] + ProductCode(u16) + SerialNumber(u32)
    for i in 0..MAX_DISPLAYS {
        if i < display_count {
            let display = &profile.displays[i];
            push_fixed_str(&mut buf, &display.manufacturer_code, 3);
            buf.push(0); // padding
            let product_code = u16::from_str_radix(&display.product_code, 16)
                .map_err(|e| format!("Invalid display product_code: {}", e))?;
            push_u16(&mut buf, product_code);
            let serial = u32::from_str_radix(&display.serial_number, 16).unwrap_or(0);
            push_u32(&mut buf, serial);
        } else {
            buf.extend(std::iter::repeat(0u8).take(10)); // 3+1+2+4
        }
    }

    Ok(buf)
}

fn push_u16(buf: &mut Vec<u8>, val: u16) {
    buf.extend_from_slice(&val.to_le_bytes());
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

fn parse_mac(mac_str: &str) -> Result<[u8; 6], String> {
    let parts: Vec<&str> = mac_str.split(':').collect();
    if parts.len() != 6 {
        return Err(format!("Invalid MAC format: {}", mac_str));
    }
    let mut mac = [0u8; 6];
    for (i, part) in parts.iter().enumerate() {
        mac[i] = u8::from_str_radix(part, 16)
            .map_err(|e| format!("Invalid MAC octet '{}': {}", part, e))?;
    }
    Ok(mac)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::engine::generate_profile;

    #[test]
    fn kernel_profile_magic_and_version() {
        let profile = generate_profile("drv-ser-test", "test");
        let blob = serialize_kernel_profile(&profile).unwrap();

        let magic = u32::from_le_bytes([blob[0], blob[1], blob[2], blob[3]]);
        assert_eq!(magic, PHANTOM_PROFILE_MAGIC);

        let version = u32::from_le_bytes([blob[4], blob[5], blob[6], blob[7]]);
        assert_eq!(version, 1);
    }

    #[test]
    fn kernel_profile_counts() {
        let profile = generate_profile("drv-counts", "test");
        let blob = serialize_kernel_profile(&profile).unwrap();

        let disk_count = u32::from_le_bytes([blob[8], blob[9], blob[10], blob[11]]);
        let nic_count = u32::from_le_bytes([blob[12], blob[13], blob[14], blob[15]]);
        let gpu_count = u32::from_le_bytes([blob[16], blob[17], blob[18], blob[19]]);
        let has_tpm = u32::from_le_bytes([blob[20], blob[21], blob[22], blob[23]]);
        let display_count = u32::from_le_bytes([blob[24], blob[25], blob[26], blob[27]]);

        assert_eq!(disk_count as usize, profile.disks.len().min(MAX_DISKS));
        assert_eq!(nic_count as usize, profile.network_adapters.len().min(MAX_NICS));
        assert_eq!(gpu_count as usize, profile.gpus.len().min(MAX_GPUS));
        assert_eq!(has_tpm, if profile.tpm.is_some() { 1 } else { 0 });
        assert_eq!(display_count as usize, profile.displays.len().min(MAX_DISPLAYS));
    }

    #[test]
    fn parse_mac_valid() {
        let mac = parse_mac("AA:BB:CC:DD:EE:FF").unwrap();
        assert_eq!(mac, [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);
    }

    #[test]
    fn parse_mac_invalid_format() {
        assert!(parse_mac("AA:BB:CC").is_err());
        assert!(parse_mac("not-a-mac").is_err());
    }

    #[test]
    fn kernel_profile_serialization_consistent() {
        let profile = generate_profile("consistency-test", "test");
        let blob1 = serialize_kernel_profile(&profile).unwrap();
        let blob2 = serialize_kernel_profile(&profile).unwrap();
        assert_eq!(blob1, blob2);
    }

    #[test]
    fn format_driver_status_loaded() {
        let status = DriverStatus {
            loaded: true,
            version: Some("0.2.0".into()),
            profile_active: true,
            attached_disk_count: 2,
            attached_nic_count: 1,
            attached_gpu_count: 1,
            intercepted_ioctl_count: 42,
        };
        let formatted = format_driver_status(&status);
        assert!(formatted.contains("active"));
        assert!(formatted.contains("2 disk"));
    }

    #[test]
    fn format_driver_status_not_loaded() {
        let status = DriverStatus {
            loaded: false,
            version: None,
            profile_active: false,
            attached_disk_count: 0,
            attached_nic_count: 0,
            attached_gpu_count: 0,
            intercepted_ioctl_count: 0,
        };
        assert_eq!(format_driver_status(&status), "Driver: not loaded");
    }
}

// --- Windows-specific driver communication ---

#[cfg(windows)]
const PHANTOM_DEVICE_PATH: &str = r"\\.\PhantomSpoof";

#[cfg(windows)]
const IOCTL_PHANTOM_SET_PROFILE: u32 = ctl_code(0x8000, 0x800);
#[cfg(windows)]
const IOCTL_PHANTOM_CLEAR_PROFILE: u32 = ctl_code(0x8000, 0x801);
#[cfg(windows)]
const IOCTL_PHANTOM_GET_STATUS: u32 = ctl_code(0x8000, 0x802);

#[cfg(windows)]
const fn ctl_code(device_type: u32, function: u32) -> u32 {
    (device_type << 16) | (0x01 << 14) | (function << 2) | 0
}

#[cfg(windows)]
fn check_driver_windows() -> DriverStatus {
    use std::os::windows::io::RawHandle;
    use std::ptr;

    let handle = open_driver_device();
    match handle {
        Ok(h) => {
            let mut status_buf = [0u8; 32]; // PHANTOM_STATUS is 32 bytes
            match device_io_control(h, IOCTL_PHANTOM_GET_STATUS, &[], &mut status_buf) {
                Ok(bytes_returned) if bytes_returned >= 20 => {
                    let profile_active = status_buf[4] != 0;
                    let disk_count = u32::from_le_bytes([status_buf[5], status_buf[6], status_buf[7], status_buf[8]]);
                    let nic_count = u32::from_le_bytes([status_buf[9], status_buf[10], status_buf[11], status_buf[12]]);
                    let gpu_count = u32::from_le_bytes([status_buf[13], status_buf[14], status_buf[15], status_buf[16]]);
                    let intercepted = u32::from_le_bytes([status_buf[21], status_buf[22], status_buf[23], status_buf[24]]);

                    close_handle(h);
                    DriverStatus {
                        loaded: true,
                        version: Some("0.2.0".into()),
                        profile_active,
                        attached_disk_count: disk_count,
                        attached_nic_count: nic_count,
                        attached_gpu_count: gpu_count,
                        intercepted_ioctl_count: intercepted,
                    }
                }
                _ => {
                    close_handle(h);
                    DriverStatus {
                        loaded: true,
                        version: Some("0.2.0".into()),
                        profile_active: false,
                        attached_disk_count: 0,
                        attached_nic_count: 0,
                        attached_gpu_count: 0,
                        intercepted_ioctl_count: 0,
                    }
                }
            }
        }
        Err(_) => DriverStatus {
            loaded: false,
            version: None,
            profile_active: false,
            attached_disk_count: 0,
            attached_nic_count: 0,
            attached_gpu_count: 0,
            intercepted_ioctl_count: 0,
        },
    }
}

#[cfg(windows)]
fn send_ioctl(ioctl: u32, data: &[u8]) -> Result<(), String> {
    let handle = open_driver_device()?;
    let mut out_buf = [0u8; 4];
    let result = device_io_control(handle, ioctl, data, &mut out_buf);
    close_handle(handle);
    result.map(|_| ())
}

#[cfg(windows)]
fn open_driver_device() -> Result<isize, String> {
    use std::ffi::CString;

    extern "system" {
        fn CreateFileA(
            lpFileName: *const u8,
            dwDesiredAccess: u32,
            dwShareMode: u32,
            lpSecurityAttributes: *const std::ffi::c_void,
            dwCreationDisposition: u32,
            dwFlagsAndAttributes: u32,
            hTemplateFile: isize,
        ) -> isize;
    }

    let path = CString::new(PHANTOM_DEVICE_PATH).unwrap();
    let handle = unsafe {
        CreateFileA(
            path.as_ptr() as *const u8,
            0xC0000000, // GENERIC_READ | GENERIC_WRITE
            0,
            std::ptr::null(),
            3, // OPEN_EXISTING
            0,
            0,
        )
    };

    if handle == -1 {
        Err("Failed to open phantom driver device. Is the driver loaded?".into())
    } else {
        Ok(handle)
    }
}

#[cfg(windows)]
fn device_io_control(
    handle: isize,
    ioctl: u32,
    in_buf: &[u8],
    out_buf: &mut [u8],
) -> Result<usize, String> {
    extern "system" {
        fn DeviceIoControl(
            hDevice: isize,
            dwIoControlCode: u32,
            lpInBuffer: *const u8,
            nInBufferSize: u32,
            lpOutBuffer: *mut u8,
            nOutBufferSize: u32,
            lpBytesReturned: *mut u32,
            lpOverlapped: *const std::ffi::c_void,
        ) -> i32;
    }

    let mut bytes_returned: u32 = 0;
    let result = unsafe {
        DeviceIoControl(
            handle,
            ioctl,
            if in_buf.is_empty() { std::ptr::null() } else { in_buf.as_ptr() },
            in_buf.len() as u32,
            out_buf.as_mut_ptr(),
            out_buf.len() as u32,
            &mut bytes_returned,
            std::ptr::null(),
        )
    };

    if result == 0 {
        Err("DeviceIoControl failed".into())
    } else {
        Ok(bytes_returned as usize)
    }
}

#[cfg(windows)]
fn close_handle(handle: isize) {
    extern "system" {
        fn CloseHandle(hObject: isize) -> i32;
    }
    unsafe { CloseHandle(handle); }
}
