#[cfg_attr(not(windows), allow(dead_code))]
const RUN_KEY: &str = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Run";
#[cfg_attr(not(windows), allow(dead_code))]
const VALUE_NAME: &str = "PhantomTray";

#[cfg(windows)]
pub fn install() -> Result<(), String> {
    extern "system" {
        fn RegOpenKeyExA(
            hKey: isize, lpSubKey: *const u8, ulOptions: u32,
            samDesired: u32, phkResult: *mut isize,
        ) -> i32;
        fn RegSetValueExA(
            hKey: isize, lpValueName: *const u8, reserved: u32,
            dwType: u32, lpData: *const u8, cbData: u32,
        ) -> i32;
        fn RegCloseKey(hKey: isize) -> i32;
        fn GetModuleFileNameA(hModule: isize, lpFilename: *mut u8, nSize: u32) -> u32;
    }

    const HKEY_CURRENT_USER: isize = -2147483647; // 0x80000001
    const KEY_SET_VALUE: u32 = 0x0002;
    const REG_SZ: u32 = 1;

    let mut path_buf = vec![0u8; 1024];
    let len = unsafe { GetModuleFileNameA(0, path_buf.as_mut_ptr(), path_buf.len() as u32) };
    if len == 0 {
        return Err("failed to get executable path".into());
    }
    path_buf.truncate(len as usize + 1); // include null terminator

    let key_str = format!("{}\0", RUN_KEY);
    let val_str = format!("{}\0", VALUE_NAME);

    let mut hkey: isize = 0;
    let result = unsafe {
        RegOpenKeyExA(
            HKEY_CURRENT_USER,
            key_str.as_ptr(),
            0,
            KEY_SET_VALUE,
            &mut hkey,
        )
    };
    if result != 0 {
        return Err(format!("failed to open registry key: error {}", result));
    }

    let result = unsafe {
        RegSetValueExA(
            hkey,
            val_str.as_ptr(),
            0,
            REG_SZ,
            path_buf.as_ptr(),
            path_buf.len() as u32,
        )
    };
    unsafe { RegCloseKey(hkey) };

    if result != 0 {
        return Err(format!("failed to set registry value: error {}", result));
    }

    Ok(())
}

#[cfg(windows)]
pub fn remove() -> Result<(), String> {
    extern "system" {
        fn RegOpenKeyExA(
            hKey: isize, lpSubKey: *const u8, ulOptions: u32,
            samDesired: u32, phkResult: *mut isize,
        ) -> i32;
        fn RegDeleteValueA(hKey: isize, lpValueName: *const u8) -> i32;
        fn RegCloseKey(hKey: isize) -> i32;
    }

    const HKEY_CURRENT_USER: isize = -2147483647;
    const KEY_SET_VALUE: u32 = 0x0002;

    let key_str = format!("{}\0", RUN_KEY);
    let val_str = format!("{}\0", VALUE_NAME);

    let mut hkey: isize = 0;
    let result = unsafe {
        RegOpenKeyExA(HKEY_CURRENT_USER, key_str.as_ptr(), 0, KEY_SET_VALUE, &mut hkey)
    };
    if result != 0 {
        return Err(format!("failed to open registry key: error {}", result));
    }

    let result = unsafe { RegDeleteValueA(hkey, val_str.as_ptr()) };
    unsafe { RegCloseKey(hkey) };

    if result != 0 {
        return Err(format!("failed to delete registry value: error {}", result));
    }

    Ok(())
}

#[cfg(not(windows))]
pub fn install() -> Result<(), String> {
    Err("auto-start requires Windows".into())
}

#[cfg(not(windows))]
pub fn remove() -> Result<(), String> {
    Err("auto-start requires Windows".into())
}

#[cfg(test)]
mod tests {
    #[cfg(not(windows))]
    #[test]
    fn autostart_not_supported_on_non_windows() {
        assert!(super::install().is_err());
        assert!(super::remove().is_err());
    }
}
