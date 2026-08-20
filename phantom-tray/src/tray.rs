#[cfg(windows)]
use phantom_ipc::message::{Request, Response, ServiceStatus};

#[cfg(windows)]
use crate::icons;
#[cfg(windows)]
use crate::popup::{self, PopupData};
#[cfg(windows)]
use crate::toast::{self, ToastIcon};

// --- Win32 constants ---

#[cfg(windows)]
const WM_APP: u32 = 0x8000;
#[cfg(windows)]
const WM_TRAY_CALLBACK: u32 = WM_APP + 1;
#[cfg(windows)]
const WM_COMMAND: u32 = 0x0111;
#[cfg(windows)]
const WM_TIMER: u32 = 0x0113;
#[cfg(windows)]
const WM_DESTROY: u32 = 0x0002;
#[cfg(windows)]
const WM_LBUTTONUP: u32 = 0x0202;
#[cfg(windows)]
const WM_RBUTTONUP: u32 = 0x0205;

#[cfg(windows)]
const NIM_ADD: u32 = 0x00000000;
#[cfg(windows)]
const NIM_MODIFY: u32 = 0x00000001;
#[cfg(windows)]
const NIM_DELETE: u32 = 0x00000002;
#[cfg(windows)]
const NIF_MESSAGE: u32 = 0x00000001;
#[cfg(windows)]
const NIF_ICON: u32 = 0x00000002;
#[cfg(windows)]
const NIF_TIP: u32 = 0x00000004;

#[cfg(windows)]
const MF_STRING: u32 = 0x00000000;
#[cfg(windows)]
const MF_SEPARATOR: u32 = 0x00000800;
#[cfg(windows)]
const MF_GRAYED: u32 = 0x00000001;
#[cfg(windows)]
const MF_POPUP: u32 = 0x00000010;
#[cfg(windows)]
const MF_CHECKED: u32 = 0x00000008;

#[cfg(windows)]
const TPM_BOTTOMALIGN: u32 = 0x0020;
#[cfg(windows)]
const TPM_LEFTALIGN: u32 = 0x0000;

#[cfg(windows)]
const TRAY_ICON_ID: u32 = 1;
#[cfg(windows)]
const TIMER_POLL_ID: usize = 100;
#[cfg(windows)]
const POLL_INTERVAL_MS: u32 = 3000;

#[cfg(windows)]
const CMD_TOGGLE: usize = 1001;
#[cfg(windows)]
const CMD_QUIT: usize = 1002;
#[cfg(windows)]
const CMD_PROFILE_BASE: usize = 2000;

// --- Win32 FFI ---

#[cfg(windows)]
extern "system" {
    fn RegisterClassExA(wc: *const WndClassExA) -> u16;
    fn CreateWindowExA(
        ex_style: u32, class: *const u8, name: *const u8, style: u32,
        x: i32, y: i32, w: i32, h: i32,
        parent: isize, menu: isize, instance: isize, param: *mut u8,
    ) -> isize;
    fn DefWindowProcA(hwnd: isize, msg: u32, wp: usize, lp: isize) -> isize;
    fn DestroyWindow(hwnd: isize) -> i32;
    fn PostQuitMessage(code: i32);
    fn GetMessageA(msg: *mut Msg, hwnd: isize, min: u32, max: u32) -> i32;
    fn TranslateMessage(msg: *const Msg) -> i32;
    fn DispatchMessageA(msg: *const Msg) -> isize;
    fn GetModuleHandleA(name: *const u8) -> isize;
    fn Shell_NotifyIconA(msg: u32, data: *mut NotifyIconDataA) -> i32;
    fn SetTimer(hwnd: isize, id: usize, elapse: u32, func: *const u8) -> usize;
    fn CreatePopupMenu() -> isize;
    fn AppendMenuA(menu: isize, flags: u32, id: usize, text: *const u8) -> i32;
    fn TrackPopupMenu(
        menu: isize, flags: u32, x: i32, y: i32,
        reserved: i32, hwnd: isize, rect: *const u8,
    ) -> i32;
    fn DestroyMenu(menu: isize) -> i32;
    fn SetForegroundWindow(hwnd: isize) -> i32;
    fn PostMessageA(hwnd: isize, msg: u32, wp: usize, lp: isize) -> i32;
    fn GetCursorPos(pt: *mut Point) -> i32;
    fn DestroyIcon(icon: isize) -> i32;
}

// --- Win32 structs ---

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
struct Msg {
    hwnd: isize,
    message: u32,
    w_param: usize,
    l_param: isize,
    time: u32,
    pt_x: i32,
    pt_y: i32,
}

#[cfg(windows)]
#[repr(C)]
struct Point {
    x: i32,
    y: i32,
}

// Exported for toast.rs
#[cfg(windows)]
#[repr(C)]
pub struct NotifyIconDataA {
    pub cb_size: u32,
    pub hwnd: isize,
    pub u_id: u32,
    pub u_flags: u32,
    pub u_callback_message: u32,
    pub h_icon: isize,
    pub sz_tip: [u8; 128],
    pub dw_state: u32,
    pub dw_state_mask: u32,
    pub sz_info: [u8; 256],
    pub u_version: u32,
    pub sz_info_title: [u8; 64],
    pub dw_info_flags: u32,
}

// Non-Windows stub so toast.rs compiles
#[cfg(not(windows))]
#[repr(C)]
pub struct NotifyIconDataA {
    pub cb_size: u32,
    pub hwnd: isize,
    pub u_id: u32,
    pub u_flags: u32,
    pub u_callback_message: u32,
    pub h_icon: isize,
    pub sz_tip: [u8; 128],
    pub dw_state: u32,
    pub dw_state_mask: u32,
    pub sz_info: [u8; 256],
    pub u_version: u32,
    pub sz_info_title: [u8; 64],
    pub dw_info_flags: u32,
}

// --- Tray app state ---

#[cfg(windows)]
struct TrayApp {
    hwnd: isize,
    icon_green: isize,
    icon_grey: isize,
    icon_amber: isize,
    current_icon: TrayIcon,
    status: ServiceStatus,
    connected: bool,
    profile_names: Vec<String>,
    first_run_shown: bool,
}

#[cfg(windows)]
#[derive(PartialEq)]
enum TrayIcon {
    Green,
    Grey,
    Amber,
}

#[cfg(windows)]
static mut APP: *mut TrayApp = std::ptr::null_mut();

#[cfg(windows)]
const TRAY_CLASS: &[u8] = b"PhantomTrayWindow\0";

// --- Entry point ---

#[cfg(windows)]
pub fn run() {
    unsafe {
        let instance = GetModuleHandleA(std::ptr::null());

        let wc = WndClassExA {
            cb_size: std::mem::size_of::<WndClassExA>() as u32,
            style: 0,
            wnd_proc,
            cls_extra: 0,
            wnd_extra: 0,
            instance,
            icon: 0,
            cursor: 0,
            background: 0,
            menu_name: std::ptr::null(),
            class_name: TRAY_CLASS.as_ptr(),
            icon_sm: 0,
        };
        RegisterClassExA(&wc);

        let hwnd = CreateWindowExA(
            0,
            TRAY_CLASS.as_ptr(),
            b"Phantom\0".as_ptr(),
            0,
            0, 0, 0, 0,
            0, 0, instance, std::ptr::null_mut(),
        );
        if hwnd == 0 {
            eprintln!("  Failed to create message window.");
            return;
        }

        let icon_green = icons::create_hicon(&icons::shield_rgba(
            icons::GREEN.0, icons::GREEN.1, icons::GREEN.2,
        ));
        let icon_grey = icons::create_hicon(&icons::shield_rgba(
            icons::GREY.0, icons::GREY.1, icons::GREY.2,
        ));
        let icon_amber = icons::create_hicon(&icons::shield_rgba(
            icons::AMBER.0, icons::AMBER.1, icons::AMBER.2,
        ));

        let app = Box::new(TrayApp {
            hwnd,
            icon_green,
            icon_grey,
            icon_amber,
            current_icon: TrayIcon::Grey,
            status: ServiceStatus::default(),
            connected: false,
            profile_names: Vec::new(),
            first_run_shown: false,
        });
        APP = Box::into_raw(app);

        add_tray_icon(hwnd, icon_grey);
        SetTimer(hwnd, TIMER_POLL_ID, POLL_INTERVAL_MS, std::ptr::null());

        poll_service();

        let mut msg: Msg = std::mem::zeroed();
        while GetMessageA(&mut msg, 0, 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageA(&msg);
        }

        remove_tray_icon(hwnd);

        DestroyIcon(icon_green);
        DestroyIcon(icon_grey);
        DestroyIcon(icon_amber);

        let _ = Box::from_raw(APP);
        APP = std::ptr::null_mut();
    }
}

#[cfg(not(windows))]
pub fn run() {
    eprintln!("  System tray requires Windows.");
    eprintln!("  Use phantom-cli for command-line operation.");
}

// --- Window procedure ---

#[cfg(windows)]
extern "system" fn wnd_proc(hwnd: isize, msg: u32, wp: usize, lp: isize) -> isize {
    match msg {
        WM_TRAY_CALLBACK => {
            let event = (lp & 0xFFFF) as u32;
            match event {
                WM_LBUTTONUP => on_left_click(),
                WM_RBUTTONUP => on_right_click(hwnd),
                _ => {}
            }
            0
        }
        WM_TIMER => {
            if wp == TIMER_POLL_ID {
                poll_service();
            }
            0
        }
        WM_COMMAND => {
            let cmd = wp & 0xFFFF;
            on_menu_command(cmd, hwnd);
            0
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            0
        }
        _ => unsafe { DefWindowProcA(hwnd, msg, wp, lp) },
    }
}

// --- Tray icon management ---

#[cfg(windows)]
fn add_tray_icon(hwnd: isize, icon: isize) {
    let mut nid: NotifyIconDataA = unsafe { std::mem::zeroed() };
    nid.cb_size = std::mem::size_of::<NotifyIconDataA>() as u32;
    nid.hwnd = hwnd;
    nid.u_id = TRAY_ICON_ID;
    nid.u_flags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    nid.u_callback_message = WM_TRAY_CALLBACK;
    nid.h_icon = icon;
    let tip = b"Phantom - Initializing...\0";
    nid.sz_tip[..tip.len()].copy_from_slice(tip);

    unsafe { Shell_NotifyIconA(NIM_ADD, &mut nid) };
}

#[cfg(windows)]
fn remove_tray_icon(hwnd: isize) {
    let mut nid: NotifyIconDataA = unsafe { std::mem::zeroed() };
    nid.cb_size = std::mem::size_of::<NotifyIconDataA>() as u32;
    nid.hwnd = hwnd;
    nid.u_id = TRAY_ICON_ID;
    unsafe { Shell_NotifyIconA(NIM_DELETE, &mut nid) };
}

#[cfg(windows)]
fn update_tray_icon(hwnd: isize, icon: isize, tooltip: &str) {
    let mut nid: NotifyIconDataA = unsafe { std::mem::zeroed() };
    nid.cb_size = std::mem::size_of::<NotifyIconDataA>() as u32;
    nid.hwnd = hwnd;
    nid.u_id = TRAY_ICON_ID;
    nid.u_flags = NIF_ICON | NIF_TIP;
    nid.h_icon = icon;
    let tip_bytes = tooltip.as_bytes();
    let len = tip_bytes.len().min(127);
    nid.sz_tip[..len].copy_from_slice(&tip_bytes[..len]);
    unsafe { Shell_NotifyIconA(NIM_MODIFY, &mut nid) };
}

// --- Service polling ---

#[cfg(windows)]
fn poll_service() {
    let app = unsafe { &mut *APP };

    match phantom_ipc::client::PhantomClient::connect() {
        Ok(mut client) => {
            let was_connected = app.connected;
            app.connected = true;

            match client.status() {
                Ok(status) => {
                    let was_protected = app.status.protected;
                    app.status = status;

                    let (target, tooltip) = if app.status.protected {
                        let name = app.status.active_profile.as_deref().unwrap_or("unknown");
                        (TrayIcon::Green, format!("Phantom - Protected ({})", name))
                    } else if !app.status.driver_connected && !app.status.firmware_detected {
                        (TrayIcon::Amber, "Phantom - No drivers loaded".into())
                    } else {
                        (TrayIcon::Grey, "Phantom - Unprotected".into())
                    };

                    if target != app.current_icon {
                        let icon_handle = match target {
                            TrayIcon::Green => app.icon_green,
                            TrayIcon::Grey => app.icon_grey,
                            TrayIcon::Amber => app.icon_amber,
                        };
                        update_tray_icon(app.hwnd, icon_handle, &tooltip);
                        app.current_icon = target;
                    }

                    if !was_connected && app.status.protected && !app.first_run_shown {
                        if app.status.active_profile.as_deref() == Some("default") {
                            app.first_run_shown = true;
                            toast::show(
                                app.hwnd,
                                TRAY_ICON_ID,
                                "Welcome to Phantom",
                                "A default identity profile has been generated. Your hardware identity is now protected.",
                                ToastIcon::Info,
                            );
                        } else {
                            toast::show(
                                app.hwnd,
                                TRAY_ICON_ID,
                                "Phantom",
                                "Connected to Phantom service",
                                ToastIcon::Info,
                            );
                        }
                    } else if !was_connected {
                        toast::show(
                            app.hwnd,
                            TRAY_ICON_ID,
                            "Phantom",
                            "Connected to Phantom service",
                            ToastIcon::Info,
                        );
                    }
                    if !was_protected && app.status.protected && was_connected {
                        let msg = format!(
                            "Identity protected with profile '{}'",
                            app.status.active_profile.as_deref().unwrap_or("?")
                        );
                        toast::show(
                            app.hwnd, TRAY_ICON_ID, "Phantom", &msg, ToastIcon::Info,
                        );
                    }
                }
                Err(_) => {
                    set_disconnected(app);
                }
            }
        }
        Err(_) => {
            if app.connected {
                toast::show(
                    app.hwnd,
                    TRAY_ICON_ID,
                    "Phantom",
                    "Lost connection to Phantom service",
                    ToastIcon::Warning,
                );
            }
            set_disconnected(app);
        }
    }
}

#[cfg(windows)]
fn set_disconnected(app: &mut TrayApp) {
    app.connected = false;
    app.status = ServiceStatus::default();
    if app.current_icon != TrayIcon::Grey {
        update_tray_icon(app.hwnd, app.icon_grey, "Phantom - Service not running");
        app.current_icon = TrayIcon::Grey;
    }
}

// --- Event handlers ---

#[cfg(windows)]
fn on_left_click() {
    let app = unsafe { &*APP };
    let mut pt = Point { x: 0, y: 0 };
    unsafe { GetCursorPos(&mut pt) };

    let data = if app.connected {
        PopupData::from_status(&app.status)
    } else {
        PopupData::disconnected()
    };
    popup::show(pt.x, pt.y, data);
}

#[cfg(windows)]
fn on_right_click(hwnd: isize) {
    let app = unsafe { &mut *APP };

    if app.connected {
        if let Ok(mut client) = phantom_ipc::client::PhantomClient::connect() {
            if let Ok(Response::Profiles { list }) =
                client.request(&Request::ListProfiles)
            {
                app.profile_names = list.iter().map(|p| p.name.clone()).collect();
            }
        }
    }

    unsafe {
        let menu = CreatePopupMenu();

        // Header
        let header = if app.connected {
            if app.status.protected {
                format!(
                    "Phantom -- Protected ({})\0",
                    app.status.active_profile.as_deref().unwrap_or("?")
                )
            } else {
                "Phantom -- Unprotected\0".into()
            }
        } else {
            "Phantom -- Service not running\0".into()
        };
        AppendMenuA(menu, MF_STRING | MF_GRAYED, 0, header.as_ptr());
        AppendMenuA(menu, MF_SEPARATOR, 0, std::ptr::null());

        // Profiles submenu
        if app.connected && !app.profile_names.is_empty() {
            let submenu = CreatePopupMenu();
            for (i, name) in app.profile_names.iter().enumerate() {
                let active = app
                    .status
                    .active_profile
                    .as_deref()
                    == Some(name.as_str());
                let flags = MF_STRING | if active { MF_CHECKED } else { 0 };
                let label = format!("{}\0", name);
                AppendMenuA(submenu, flags, CMD_PROFILE_BASE + i, label.as_ptr());
            }
            AppendMenuA(
                menu,
                MF_STRING | MF_POPUP,
                submenu as usize,
                b"Switch Profile\0".as_ptr(),
            );
            AppendMenuA(menu, MF_SEPARATOR, 0, std::ptr::null());
        }

        // Connect/Disconnect
        if app.connected {
            let label = if app.status.protected {
                b"Disconnect\0".as_ptr()
            } else {
                b"Connect\0".as_ptr()
            };
            AppendMenuA(menu, MF_STRING, CMD_TOGGLE, label);
        }
        AppendMenuA(menu, MF_SEPARATOR, 0, std::ptr::null());

        // Quit
        AppendMenuA(menu, MF_STRING, CMD_QUIT, b"Quit\0".as_ptr());

        let mut pt = Point { x: 0, y: 0 };
        GetCursorPos(&mut pt);
        SetForegroundWindow(hwnd);
        TrackPopupMenu(
            menu,
            TPM_BOTTOMALIGN | TPM_LEFTALIGN,
            pt.x,
            pt.y,
            0,
            hwnd,
            std::ptr::null(),
        );
        DestroyMenu(menu);
        PostMessageA(hwnd, 0, 0, 0);
    }
}

#[cfg(windows)]
fn on_menu_command(cmd: usize, hwnd: isize) {
    let app = unsafe { &mut *APP };

    match cmd {
        CMD_QUIT => unsafe {
            DestroyWindow(hwnd);
        },
        CMD_TOGGLE => {
            if !app.connected {
                return;
            }
            if app.status.protected {
                if let Ok(mut client) = phantom_ipc::client::PhantomClient::connect() {
                    match client.request(&Request::Unprotect) {
                        Ok(Response::Reverted { .. }) => {
                            toast::show(
                                app.hwnd, TRAY_ICON_ID,
                                "Phantom",
                                "Identity protection disabled. Hardware identity exposed.",
                                ToastIcon::Warning,
                            );
                            poll_service();
                        }
                        Ok(Response::Error { message, .. }) => {
                            toast::show(
                                app.hwnd, TRAY_ICON_ID,
                                "Phantom", &message, ToastIcon::Error,
                            );
                        }
                        _ => {}
                    }
                }
            } else {
                let profile = app
                    .profile_names
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "default".into());
                do_protect(&profile);
            }
        }
        _ => {
            if cmd >= CMD_PROFILE_BASE {
                let idx = cmd - CMD_PROFILE_BASE;
                if let Some(name) = app.profile_names.get(idx).cloned() {
                    do_protect(&name);
                }
            }
        }
    }
}

#[cfg(windows)]
fn do_protect(profile_name: &str) {
    let app = unsafe { &*APP };
    if let Ok(mut client) = phantom_ipc::client::PhantomClient::connect() {
        match client.protect(profile_name, &[1, 2]) {
            Ok(Response::Applied {
                layers_applied,
                identifiers,
            }) => {
                let msg = format!(
                    "Protected with '{}' ({} layers, {} identifiers)",
                    profile_name,
                    layers_applied.len(),
                    identifiers
                );
                toast::show(
                    app.hwnd, TRAY_ICON_ID, "Phantom", &msg, ToastIcon::Info,
                );
                poll_service();
            }
            Ok(Response::Error { message, .. }) => {
                toast::show(
                    app.hwnd, TRAY_ICON_ID, "Phantom", &message, ToastIcon::Error,
                );
            }
            _ => {}
        }
    }
}
