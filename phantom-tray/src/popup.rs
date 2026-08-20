use phantom_ipc::message::ServiceStatus;

#[cfg_attr(not(windows), allow(dead_code))]
pub struct PopupData {
    pub protected: bool,
    pub profile: String,
    pub layer_status: [(& 'static str, LayerState); 3],
    pub identifier_count: usize,
    pub uptime: String,
}

#[derive(Clone, Copy)]
#[cfg_attr(not(windows), allow(dead_code))]
pub enum LayerState {
    Active,
    Inactive,
    Missing,
}

#[cfg_attr(not(windows), allow(dead_code))]
impl PopupData {
    pub fn from_status(status: &ServiceStatus) -> Self {
        let l0 = if status.firmware_detected {
            if status.active_layers.contains(&0) {
                LayerState::Active
            } else {
                LayerState::Inactive
            }
        } else {
            LayerState::Missing
        };
        let l1 = if status.driver_connected {
            if status.active_layers.contains(&1) {
                LayerState::Active
            } else {
                LayerState::Inactive
            }
        } else {
            LayerState::Missing
        };
        let l2 = if status.active_layers.contains(&2) {
            LayerState::Active
        } else {
            LayerState::Inactive
        };

        let uptime = if status.uptime_secs >= 3600 {
            format!(
                "{}h {}m",
                status.uptime_secs / 3600,
                (status.uptime_secs % 3600) / 60
            )
        } else if status.uptime_secs >= 60 {
            format!("{}m {}s", status.uptime_secs / 60, status.uptime_secs % 60)
        } else {
            format!("{}s", status.uptime_secs)
        };

        PopupData {
            protected: status.protected,
            profile: status.active_profile.clone().unwrap_or_default(),
            layer_status: [
                ("Firmware (L0)", l0),
                ("Kernel (L1)", l1),
                ("Registry (L2)", l2),
            ],
            identifier_count: status.identifier_count,
            uptime,
        }
    }

    pub fn disconnected() -> Self {
        PopupData {
            protected: false,
            profile: String::new(),
            layer_status: [
                ("Firmware (L0)", LayerState::Missing),
                ("Kernel (L1)", LayerState::Missing),
                ("Registry (L2)", LayerState::Missing),
            ],
            identifier_count: 0,
            uptime: String::new(),
        }
    }
}

#[cfg(windows)]
static mut POPUP_HWND: isize = 0;

#[cfg(windows)]
static mut POPUP_STATE: Option<PopupData> = None;

#[cfg(windows)]
static mut POPUP_CLASS_REGISTERED: bool = false;

#[cfg(windows)]
const POPUP_WIDTH: i32 = 260;

#[cfg(windows)]
const POPUP_HEIGHT: i32 = 240;

#[cfg(windows)]
const POPUP_CLASS: &[u8] = b"PhantomPopup\0";

#[cfg(windows)]
extern "system" {
    fn RegisterClassExA(wc: *const WndClassExA) -> u16;
    fn CreateWindowExA(
        ex_style: u32, class: *const u8, name: *const u8, style: u32,
        x: i32, y: i32, w: i32, h: i32,
        parent: isize, menu: isize, instance: isize, param: *mut u8,
    ) -> isize;
    fn ShowWindow(hwnd: isize, cmd: i32) -> i32;
    fn DestroyWindow(hwnd: isize) -> i32;
    fn DefWindowProcA(hwnd: isize, msg: u32, wp: usize, lp: isize) -> isize;
    fn GetModuleHandleA(name: *const u8) -> isize;
    fn BeginPaint(hwnd: isize, ps: *mut PaintStruct) -> isize;
    fn EndPaint(hwnd: isize, ps: *const PaintStruct) -> i32;
    fn GetClientRect(hwnd: isize, rect: *mut Rect) -> i32;
    fn FillRect(hdc: isize, rect: *const Rect, brush: isize) -> i32;
    fn CreateSolidBrush(color: u32) -> isize;
    fn DeleteObject(obj: isize) -> i32;
    fn SetTextColor(hdc: isize, color: u32) -> u32;
    fn SetBkMode(hdc: isize, mode: i32) -> i32;
    fn TextOutA(hdc: isize, x: i32, y: i32, s: *const u8, len: i32) -> i32;
    fn SelectObject(hdc: isize, obj: isize) -> isize;
    fn CreateFontA(
        h: i32, w: i32, esc: i32, orient: i32, weight: i32,
        italic: u32, underline: u32, strike: u32, charset: u32,
        out_prec: u32, clip_prec: u32, quality: u32, pitch_family: u32,
        face: *const u8,
    ) -> isize;
    fn SetForegroundWindow(hwnd: isize) -> i32;
    fn MoveToEx(hdc: isize, x: i32, y: i32, prev: *mut u8) -> i32;
    fn LineTo(hdc: isize, x: i32, y: i32) -> i32;
    fn CreatePen(style: i32, width: i32, color: u32) -> isize;
}

#[cfg(windows)]
#[repr(C)]
struct WndClassExA {
    cb_size: u32,
    style: u32,
    wnd_proc: extern "system" fn(isize, u32, usize, isize) -> isize,
    cls_extra: i32,
    wnd_extra: i32,
    instance: isize,
    icon: isize,
    cursor: isize,
    background: isize,
    menu_name: *const u8,
    class_name: *const u8,
    icon_sm: isize,
}

#[cfg(windows)]
#[repr(C)]
struct PaintStruct {
    hdc: isize,
    f_erase: i32,
    rc_paint: Rect,
    f_restore: i32,
    f_inc_update: i32,
    reserved: [u8; 32],
}

#[cfg(windows)]
#[repr(C)]
#[derive(Clone, Copy)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[cfg(windows)]
fn rgb(r: u8, g: u8, b: u8) -> u32 {
    r as u32 | ((g as u32) << 8) | ((b as u32) << 16)
}

#[cfg(windows)]
pub fn show(cursor_x: i32, cursor_y: i32, data: PopupData) {
    const WS_POPUP: u32 = 0x80000000;
    const WS_EX_TOPMOST: u32 = 0x00000008;
    const WS_EX_TOOLWINDOW: u32 = 0x00000080;
    const WS_BORDER: u32 = 0x00800000;
    const SW_SHOWNA: i32 = 8;

    unsafe {
        if POPUP_HWND != 0 {
            DestroyWindow(POPUP_HWND);
            POPUP_HWND = 0;
        }

        if !POPUP_CLASS_REGISTERED {
            let instance = GetModuleHandleA(std::ptr::null());
            let wc = WndClassExA {
                cb_size: std::mem::size_of::<WndClassExA>() as u32,
                style: 0,
                wnd_proc: popup_wnd_proc,
                cls_extra: 0,
                wnd_extra: 0,
                instance,
                icon: 0,
                cursor: 0,
                background: 0,
                menu_name: std::ptr::null(),
                class_name: POPUP_CLASS.as_ptr(),
                icon_sm: 0,
            };
            RegisterClassExA(&wc);
            POPUP_CLASS_REGISTERED = true;
        }

        POPUP_STATE = Some(data);

        let px = cursor_x - POPUP_WIDTH / 2;
        let py = cursor_y - POPUP_HEIGHT - 8;
        let instance = GetModuleHandleA(std::ptr::null());

        POPUP_HWND = CreateWindowExA(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            POPUP_CLASS.as_ptr(),
            b"Phantom Status\0".as_ptr(),
            WS_POPUP | WS_BORDER,
            px,
            py,
            POPUP_WIDTH,
            POPUP_HEIGHT,
            0,
            0,
            instance,
            std::ptr::null_mut(),
        );

        if POPUP_HWND != 0 {
            ShowWindow(POPUP_HWND, SW_SHOWNA);
            SetForegroundWindow(POPUP_HWND);
        }
    }
}

#[cfg(windows)]
pub fn hide() {
    unsafe {
        if POPUP_HWND != 0 {
            DestroyWindow(POPUP_HWND);
            POPUP_HWND = 0;
        }
        POPUP_STATE = None;
    }
}

#[cfg(windows)]
extern "system" fn popup_wnd_proc(hwnd: isize, msg: u32, wp: usize, lp: isize) -> isize {
    const WM_PAINT: u32 = 0x000F;
    const WM_ACTIVATE: u32 = 0x0006;
    const WM_KEYDOWN: u32 = 0x0100;
    const WM_DESTROY: u32 = 0x0002;
    const WA_INACTIVE: usize = 0;
    const VK_ESCAPE: usize = 0x1B;
    const TRANSPARENT: i32 = 1;
    const FW_BOLD: i32 = 700;
    const FW_NORMAL: i32 = 400;

    match msg {
        WM_PAINT => {
            let mut ps: PaintStruct = unsafe { std::mem::zeroed() };
            let hdc = unsafe { BeginPaint(hwnd, &mut ps) };
            if hdc != 0 {
                paint_status(hdc, hwnd);
            }
            unsafe { EndPaint(hwnd, &ps) };
            0
        }
        WM_ACTIVATE => {
            if wp == WA_INACTIVE {
                hide();
            }
            0
        }
        WM_KEYDOWN => {
            if wp == VK_ESCAPE {
                hide();
            }
            0
        }
        WM_DESTROY => {
            unsafe {
                POPUP_HWND = 0;
                POPUP_STATE = None;
            }
            0
        }
        _ => unsafe { DefWindowProcA(hwnd, msg, wp, lp) },
    }
}

#[cfg(windows)]
fn paint_status(hdc: isize, hwnd: isize) {
    const FW_BOLD: i32 = 700;
    const FW_NORMAL: i32 = 400;
    const TRANSPARENT: i32 = 1;

    let mut rc: Rect = unsafe { std::mem::zeroed() };
    unsafe { GetClientRect(hwnd, &mut rc) };

    // Background
    let bg_brush = unsafe { CreateSolidBrush(rgb(24, 26, 32)) };
    unsafe { FillRect(hdc, &rc, bg_brush) };
    unsafe { DeleteObject(bg_brush) };
    unsafe { SetBkMode(hdc, TRANSPARENT) };

    let font_bold = unsafe {
        CreateFontA(
            16, 0, 0, 0, FW_BOLD, 0, 0, 0, 0, 0, 0, 0, 0,
            b"Segoe UI\0".as_ptr(),
        )
    };
    let font_normal = unsafe {
        CreateFontA(
            14, 0, 0, 0, FW_NORMAL, 0, 0, 0, 0, 0, 0, 0, 0,
            b"Segoe UI\0".as_ptr(),
        )
    };

    let margin: i32 = 14;
    let mut y = margin;

    let state = unsafe { &POPUP_STATE };
    let data = match state {
        Some(d) => d,
        None => return,
    };

    // Title
    unsafe { SelectObject(hdc, font_bold) };
    unsafe { SetTextColor(hdc, rgb(61, 214, 140)) };
    text_out(hdc, margin, y, "PHANTOM");
    y += 22;

    // Separator
    draw_sep(hdc, margin, y, rc.right - margin);
    y += 10;

    // Status line
    unsafe { SelectObject(hdc, font_normal) };
    if data.protected {
        unsafe { SetTextColor(hdc, rgb(61, 214, 140)) };
        text_out(hdc, margin, y, "Status: Protected");
    } else {
        unsafe { SetTextColor(hdc, rgb(140, 140, 150)) };
        text_out(hdc, margin, y, "Status: Unprotected");
    }
    y += 20;

    // Profile
    if !data.profile.is_empty() {
        unsafe { SetTextColor(hdc, rgb(200, 204, 212)) };
        let profile_line = format!("Profile: {}", data.profile);
        text_out(hdc, margin, y, &profile_line);
    } else {
        unsafe { SetTextColor(hdc, rgb(100, 104, 114)) };
        text_out(hdc, margin, y, "No active profile");
    }
    y += 22;

    // Separator
    draw_sep(hdc, margin, y, rc.right - margin);
    y += 10;

    // Layer status
    for (label, state) in &data.layer_status {
        let (color, suffix) = match state {
            LayerState::Active => (rgb(61, 214, 140), "ON"),
            LayerState::Inactive => (rgb(140, 140, 150), "OFF"),
            LayerState::Missing => (rgb(100, 104, 114), "--"),
        };
        unsafe { SetTextColor(hdc, rgb(200, 204, 212)) };
        text_out(hdc, margin, y, label);
        unsafe { SetTextColor(hdc, color) };
        text_out(hdc, rc.right - margin - 30, y, suffix);
        y += 20;
    }
    y += 2;

    // Separator
    draw_sep(hdc, margin, y, rc.right - margin);
    y += 10;

    // Uptime
    unsafe { SetTextColor(hdc, rgb(160, 164, 176)) };
    if data.protected && !data.uptime.is_empty() {
        let uptime_line = format!("Protected for {}", data.uptime);
        text_out(hdc, margin, y, &uptime_line);
    } else if !data.protected {
        text_out(hdc, margin, y, "Identity exposed");
    }

    unsafe {
        DeleteObject(font_bold);
        DeleteObject(font_normal);
    }
}

#[cfg(windows)]
fn text_out(hdc: isize, x: i32, y: i32, text: &str) {
    let bytes = text.as_bytes();
    unsafe { TextOutA(hdc, x, y, bytes.as_ptr(), bytes.len() as i32) };
}

#[cfg(windows)]
fn draw_sep(hdc: isize, x1: i32, y: i32, x2: i32) {
    let pen = unsafe { CreatePen(0, 1, rgb(50, 54, 64)) };
    let old = unsafe { SelectObject(hdc, pen) };
    unsafe {
        MoveToEx(hdc, x1, y, std::ptr::null_mut());
        LineTo(hdc, x2, y);
        SelectObject(hdc, old);
        DeleteObject(pen);
    }
}

#[cfg(not(windows))]
#[allow(dead_code)]
pub fn show(_cursor_x: i32, _cursor_y: i32, _data: PopupData) {}

#[cfg(not(windows))]
#[allow(dead_code)]
pub fn hide() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn popup_data_from_status() {
        let status = ServiceStatus {
            protected: true,
            active_profile: Some("test".into()),
            active_layers: vec![1, 2],
            uptime_secs: 3725,
            driver_connected: true,
            firmware_detected: false,
            identifier_count: 20,
        };
        let data = PopupData::from_status(&status);
        assert!(data.protected);
        assert_eq!(data.profile, "test");
        assert_eq!(data.uptime, "1h 2m");
        assert!(matches!(data.layer_status[0].1, LayerState::Missing));
        assert!(matches!(data.layer_status[1].1, LayerState::Active));
        assert!(matches!(data.layer_status[2].1, LayerState::Active));
    }

    #[test]
    fn popup_data_disconnected() {
        let data = PopupData::disconnected();
        assert!(!data.protected);
        assert!(data.profile.is_empty());
        assert!(matches!(data.layer_status[0].1, LayerState::Missing));
    }

    #[test]
    fn uptime_formatting() {
        let make = |secs| {
            let status = ServiceStatus {
                uptime_secs: secs,
                ..ServiceStatus::default()
            };
            PopupData::from_status(&status).uptime
        };
        assert_eq!(make(30), "30s");
        assert_eq!(make(90), "1m 30s");
        assert_eq!(make(7200), "2h 0m");
    }
}
