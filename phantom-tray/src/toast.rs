#[cfg_attr(not(windows), allow(dead_code))]
pub enum ToastIcon {
    Info,
    Warning,
    Error,
    None,
}

#[cfg(windows)]
pub fn show(hwnd: isize, nid_uid: u32, title: &str, message: &str, icon: ToastIcon) {
    use crate::tray::NotifyIconDataA;

    extern "system" {
        fn Shell_NotifyIconA(msg: u32, data: *mut NotifyIconDataA) -> i32;
    }
    const NIM_MODIFY: u32 = 0x00000001;
    const NIF_INFO: u32 = 0x00000010;
    const NIIF_INFO: u32 = 0x00000001;
    const NIIF_WARNING: u32 = 0x00000002;
    const NIIF_ERROR: u32 = 0x00000003;
    const NIIF_NONE: u32 = 0x00000000;

    let mut nid: NotifyIconDataA = unsafe { std::mem::zeroed() };
    nid.cb_size = std::mem::size_of::<NotifyIconDataA>() as u32;
    nid.hwnd = hwnd;
    nid.u_id = nid_uid;
    nid.u_flags = NIF_INFO;

    let info_flag = match icon {
        ToastIcon::Info => NIIF_INFO,
        ToastIcon::Warning => NIIF_WARNING,
        ToastIcon::Error => NIIF_ERROR,
        ToastIcon::None => NIIF_NONE,
    };
    nid.dw_info_flags = info_flag;

    let title_bytes = title.as_bytes();
    let msg_bytes = message.as_bytes();
    let tlen = title_bytes.len().min(63);
    let mlen = msg_bytes.len().min(255);
    nid.sz_info_title[..tlen].copy_from_slice(&title_bytes[..tlen]);
    nid.sz_info[..mlen].copy_from_slice(&msg_bytes[..mlen]);

    unsafe {
        Shell_NotifyIconA(NIM_MODIFY, &mut nid);
    }
}

#[cfg(not(windows))]
#[allow(dead_code)]
pub fn show(_hwnd: isize, _nid_uid: u32, _title: &str, _message: &str, _icon: ToastIcon) {}
