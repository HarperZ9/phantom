use sha2::{Sha256, Digest};
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MachineFingerprint {
    pub hash: [u8; 16],
}

impl MachineFingerprint {
    pub fn collect() -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"phantom-hwid-v1");

        #[cfg(windows)]
        {
            if let Some(val) = read_registry_string(
                "SOFTWARE\\Microsoft\\Cryptography",
                "MachineGuid",
            ) {
                hasher.update(val.as_bytes());
            }

            if let Some(val) = read_registry_string(
                "SOFTWARE\\Microsoft\\SQMClient",
                "MachineId",
            ) {
                hasher.update(val.as_bytes());
            }

            if let Some(val) = read_registry_string(
                "HARDWARE\\DESCRIPTION\\System\\BIOS",
                "SystemProductName",
            ) {
                hasher.update(val.as_bytes());
            }

            if let Some(val) = read_registry_string(
                "HARDWARE\\DESCRIPTION\\System\\BIOS",
                "BaseBoardProduct",
            ) {
                hasher.update(val.as_bytes());
            }
        }

        #[cfg(not(windows))]
        {
            if let Ok(id) = std::fs::read_to_string("/etc/machine-id") {
                hasher.update(id.trim().as_bytes());
            }
            if let Ok(id) = std::fs::read_to_string("/sys/class/dmi/id/product_uuid") {
                hasher.update(id.trim().as_bytes());
            }
            if let Ok(id) = std::fs::read_to_string("/sys/class/dmi/id/board_serial") {
                hasher.update(id.trim().as_bytes());
            }
        }

        let full = hasher.finalize();
        let mut hash = [0u8; 16];
        hash.copy_from_slice(&full[..16]);
        MachineFingerprint { hash }
    }

    pub fn from_bytes(bytes: &[u8; 16]) -> Self {
        MachineFingerprint { hash: *bytes }
    }

    pub fn matches(&self, other: &MachineFingerprint) -> bool {
        self.hash == other.hash
    }

    pub fn hex(&self) -> String {
        self.hash.iter().map(|b| format!("{:02x}", b)).collect()
    }
}

#[cfg(windows)]
fn read_registry_string(subkey: &str, value_name: &str) -> Option<String> {
    extern "system" {
        fn RegOpenKeyExA(
            hKey: isize, lpSubKey: *const u8, ulOptions: u32,
            samDesired: u32, phkResult: *mut isize,
        ) -> i32;
        fn RegQueryValueExA(
            hKey: isize, lpValueName: *const u8, lpReserved: *mut u32,
            lpType: *mut u32, lpData: *mut u8, lpcbData: *mut u32,
        ) -> i32;
        fn RegCloseKey(hKey: isize) -> i32;
    }

    const HKEY_LOCAL_MACHINE: isize = -2147483646i64 as isize;
    const KEY_READ: u32 = 0x20019;

    let subkey_c = format!("{}\0", subkey);
    let value_c = format!("{}\0", value_name);

    let mut hkey: isize = 0;
    let rc = unsafe {
        RegOpenKeyExA(HKEY_LOCAL_MACHINE, subkey_c.as_ptr(), 0, KEY_READ, &mut hkey)
    };
    if rc != 0 {
        return None;
    }

    let mut buf = vec![0u8; 512];
    let mut size = buf.len() as u32;
    let mut reg_type: u32 = 0;

    let rc = unsafe {
        RegQueryValueExA(
            hkey,
            value_c.as_ptr(),
            std::ptr::null_mut(),
            &mut reg_type,
            buf.as_mut_ptr(),
            &mut size,
        )
    };

    unsafe { RegCloseKey(hkey) };

    if rc != 0 || size == 0 {
        return None;
    }

    let len = if buf[size as usize - 1] == 0 {
        size as usize - 1
    } else {
        size as usize
    };
    String::from_utf8(buf[..len].to_vec()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_collect_does_not_panic() {
        let fp = MachineFingerprint::collect();
        assert_eq!(fp.hash.len(), 16);
        assert_eq!(fp.hex().len(), 32);
    }

    #[test]
    fn fingerprint_matches_self() {
        let fp = MachineFingerprint::collect();
        assert!(fp.matches(&fp));
    }

    #[test]
    fn fingerprint_from_bytes_roundtrip() {
        let bytes = [1u8; 16];
        let fp = MachineFingerprint::from_bytes(&bytes);
        assert_eq!(fp.hash, bytes);
    }

    #[test]
    fn different_fingerprints_do_not_match() {
        let a = MachineFingerprint::from_bytes(&[0u8; 16]);
        let b = MachineFingerprint::from_bytes(&[1u8; 16]);
        assert!(!a.matches(&b));
    }
}
