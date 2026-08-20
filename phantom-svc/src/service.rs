#[cfg(windows)]
use crate::handler::PhantomHandler;
#[cfg(windows)]
use phantom_ipc::server::PhantomServer;
#[cfg(windows)]
use std::sync::atomic::Ordering;
#[cfg(windows)]
use std::sync::Arc;

#[cfg(windows)]
static SHUTDOWN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn run_standalone() {
    println!("  Listening on {}", phantom_ipc::pipe_name());
    println!("  Press Ctrl+C to stop.\n");

    #[cfg(not(windows))]
    {
        println!("  Named pipe server requires Windows.");
        println!("  Standalone mode is available for testing the handler logic.");
        return;
    }

    #[cfg(windows)]
    {
        let shutdown = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let shutdown_clone = shutdown.clone();

        let _ = ctrlc_handler(move || {
            tracing::info!("shutdown signal received");
            println!("\n  Shutting down...");
            shutdown_clone.store(true, Ordering::Relaxed);
        });

        let mut handler = PhantomHandler::new();
        let server = PhantomServer::new();

        tracing::info!(pipe = phantom_ipc::PIPE_NAME, "IPC server starting");
        match server.run(&mut handler, shutdown) {
            Ok(()) => {
                tracing::info!("service stopped cleanly");
                println!("  Service stopped.");
            }
            Err(e) => {
                tracing::error!(error = %e, "service error");
                eprintln!("  Service error: {}", e);
            }
        }
    }
}

pub fn run_as_service() {
    #[cfg(windows)]
    {
        run_windows_service();
    }
    #[cfg(not(windows))]
    {
        eprintln!("Windows service mode is only available on Windows.");
        eprintln!("Use --standalone for development.");
        std::process::exit(1);
    }
}

pub fn install_service() {
    #[cfg(windows)]
    {
        install_windows_service();
    }
    #[cfg(not(windows))]
    {
        eprintln!("Service installation requires Windows.");
        std::process::exit(1);
    }
}

pub fn uninstall_service() {
    #[cfg(windows)]
    {
        uninstall_windows_service();
    }
    #[cfg(not(windows))]
    {
        eprintln!("Service removal requires Windows.");
        std::process::exit(1);
    }
}

// --- Windows Service Control Manager integration ---

#[cfg(windows)]
const SERVICE_NAME: &str = "PhantomService";
#[cfg(windows)]
const SERVICE_DISPLAY_NAME: &str = "Phantom Privacy Service";

#[cfg(windows)]
extern "system" {
    fn StartServiceCtrlDispatcherA(lpServiceStartTable: *const ServiceTableEntry) -> i32;
    fn RegisterServiceCtrlHandlerExA(
        lpServiceName: *const u8,
        lpHandlerProc: extern "system" fn(u32, u32, *mut u8, *mut u8) -> u32,
        lpContext: *mut u8,
    ) -> isize;
    fn SetServiceStatus(hServiceStatus: isize, lpServiceStatus: *const ScmServiceStatus) -> i32;
    fn OpenSCManagerA(
        lpMachineName: *const u8,
        lpDatabaseName: *const u8,
        dwDesiredAccess: u32,
    ) -> isize;
    fn CreateServiceA(
        hSCManager: isize,
        lpServiceName: *const u8,
        lpDisplayName: *const u8,
        dwDesiredAccess: u32,
        dwServiceType: u32,
        dwStartType: u32,
        dwErrorControl: u32,
        lpBinaryPathName: *const u8,
        lpLoadOrderGroup: *const u8,
        lpdwTagId: *mut u32,
        lpDependencies: *const u8,
        lpServiceStartName: *const u8,
        lpPassword: *const u8,
    ) -> isize;
    fn OpenServiceA(hSCManager: isize, lpServiceName: *const u8, dwDesiredAccess: u32) -> isize;
    fn DeleteService(hService: isize) -> i32;
    fn CloseServiceHandle(hSCObject: isize) -> i32;
    fn GetModuleFileNameA(hModule: isize, lpFilename: *mut u8, nSize: u32) -> u32;
}

#[cfg(windows)]
#[repr(C)]
struct ServiceTableEntry {
    name: *const u8,
    proc_fn: Option<extern "system" fn(u32, *mut *mut u8)>,
}

#[cfg(windows)]
#[repr(C)]
struct ScmServiceStatus {
    service_type: u32,
    current_state: u32,
    controls_accepted: u32,
    win32_exit_code: u32,
    service_specific_exit_code: u32,
    check_point: u32,
    wait_hint: u32,
}

#[cfg(windows)]
const SERVICE_WIN32_OWN_PROCESS: u32 = 0x00000010;
#[cfg(windows)]
const SERVICE_START_PENDING: u32 = 0x00000002;
#[cfg(windows)]
const SERVICE_RUNNING: u32 = 0x00000004;
#[cfg(windows)]
const SERVICE_STOPPED: u32 = 0x00000001;
#[cfg(windows)]
const SERVICE_ACCEPT_STOP: u32 = 0x00000001;
#[cfg(windows)]
const SERVICE_CONTROL_STOP: u32 = 0x00000001;
#[cfg(windows)]
const SC_MANAGER_ALL_ACCESS: u32 = 0x000F003F;
#[cfg(windows)]
const SERVICE_ALL_ACCESS: u32 = 0x000F01FF;
#[cfg(windows)]
const SERVICE_AUTO_START: u32 = 0x00000002;
#[cfg(windows)]
const SERVICE_ERROR_NORMAL: u32 = 0x00000001;
#[cfg(windows)]
const DELETE: u32 = 0x00010000;

#[cfg(windows)]
static mut SERVICE_STATUS_HANDLE: isize = 0;

#[cfg(windows)]
extern "system" fn service_ctrl_handler(
    control: u32,
    _event_type: u32,
    _event_data: *mut u8,
    _context: *mut u8,
) -> u32 {
    if control == SERVICE_CONTROL_STOP {
        SHUTDOWN.store(true, Ordering::Relaxed);
        report_service_status(SERVICE_STOPPED, 0);
    }
    0
}

#[cfg(windows)]
extern "system" fn service_main(_argc: u32, _argv: *mut *mut u8) {
    use std::ffi::CString;

    let name = CString::new(SERVICE_NAME).unwrap();

    unsafe {
        SERVICE_STATUS_HANDLE = RegisterServiceCtrlHandlerExA(
            name.as_ptr() as *const u8,
            service_ctrl_handler,
            std::ptr::null_mut(),
        );
    }

    if unsafe { SERVICE_STATUS_HANDLE } == 0 {
        return;
    }

    report_service_status(SERVICE_START_PENDING, 3000);

    tracing::info!("service starting");
    report_service_status(SERVICE_RUNNING, 0);

    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_ref = &SHUTDOWN;

    let mut handler = PhantomHandler::new();
    let server = PhantomServer::new();

    let shutdown_clone = shutdown.clone();
    std::thread::spawn(move || {
        while !shutdown_ref.load(Ordering::Relaxed) {
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        shutdown_clone.store(true, Ordering::Relaxed);
    });

    tracing::info!(pipe = phantom_ipc::PIPE_NAME, "IPC server listening");
    match server.run(&mut handler, shutdown) {
        Ok(()) => tracing::info!("service stopped cleanly"),
        Err(e) => tracing::error!(error = %e, "service error"),
    }

    report_service_status(SERVICE_STOPPED, 0);
}

#[cfg(windows)]
fn report_service_status(state: u32, wait_hint: u32) {
    let status = ScmServiceStatus {
        service_type: SERVICE_WIN32_OWN_PROCESS,
        current_state: state,
        controls_accepted: if state == SERVICE_RUNNING {
            SERVICE_ACCEPT_STOP
        } else {
            0
        },
        win32_exit_code: 0,
        service_specific_exit_code: 0,
        check_point: 0,
        wait_hint,
    };

    unsafe {
        SetServiceStatus(SERVICE_STATUS_HANDLE, &status);
    }
}

#[cfg(windows)]
fn run_windows_service() {
    use std::ffi::CString;

    let name = CString::new(SERVICE_NAME).unwrap();

    let table = [
        ServiceTableEntry {
            name: name.as_ptr() as *const u8,
            proc_fn: Some(service_main),
        },
        ServiceTableEntry {
            name: std::ptr::null(),
            proc_fn: None,
        },
    ];

    let result = unsafe { StartServiceCtrlDispatcherA(table.as_ptr()) };

    if result == 0 {
        eprintln!("Failed to start service dispatcher.");
        eprintln!("If running from a terminal, use --standalone instead.");
        std::process::exit(1);
    }
}

#[cfg(windows)]
fn install_windows_service() {
    use std::ffi::CString;

    let mut path_buf = vec![0u8; 1024];
    let len = unsafe { GetModuleFileNameA(0, path_buf.as_mut_ptr(), path_buf.len() as u32) };
    if len == 0 {
        eprintln!("Failed to get executable path.");
        std::process::exit(1);
    }
    path_buf.truncate(len as usize);
    let exe_path = String::from_utf8_lossy(&path_buf).to_string();

    let scm = unsafe { OpenSCManagerA(std::ptr::null(), std::ptr::null(), SC_MANAGER_ALL_ACCESS) };
    if scm == 0 {
        eprintln!("Failed to open Service Control Manager. Run as Administrator.");
        std::process::exit(1);
    }

    let svc_name = CString::new(SERVICE_NAME).unwrap();
    let display_name = CString::new(SERVICE_DISPLAY_NAME).unwrap();
    let bin_path = CString::new(exe_path.as_str()).unwrap();

    let svc = unsafe {
        CreateServiceA(
            scm,
            svc_name.as_ptr() as *const u8,
            display_name.as_ptr() as *const u8,
            SERVICE_ALL_ACCESS,
            SERVICE_WIN32_OWN_PROCESS,
            SERVICE_AUTO_START,
            SERVICE_ERROR_NORMAL,
            bin_path.as_ptr() as *const u8,
            std::ptr::null(),
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
        )
    };

    if svc == 0 {
        eprintln!("Failed to create service. It may already exist.");
        unsafe {
            CloseServiceHandle(scm);
        }
        std::process::exit(1);
    }

    println!(
        "  Service '{}' installed successfully.",
        SERVICE_DISPLAY_NAME
    );
    println!("  Start with: sc start {}", SERVICE_NAME);

    unsafe {
        CloseServiceHandle(svc);
        CloseServiceHandle(scm);
    }
}

#[cfg(windows)]
fn uninstall_windows_service() {
    use std::ffi::CString;

    let scm = unsafe { OpenSCManagerA(std::ptr::null(), std::ptr::null(), SC_MANAGER_ALL_ACCESS) };
    if scm == 0 {
        eprintln!("Failed to open Service Control Manager. Run as Administrator.");
        std::process::exit(1);
    }

    let svc_name = CString::new(SERVICE_NAME).unwrap();
    let svc = unsafe { OpenServiceA(scm, svc_name.as_ptr() as *const u8, DELETE) };

    if svc == 0 {
        eprintln!("Service '{}' not found.", SERVICE_NAME);
        unsafe {
            CloseServiceHandle(scm);
        }
        std::process::exit(1);
    }

    let result = unsafe { DeleteService(svc) };
    if result == 0 {
        eprintln!("Failed to delete service.");
    } else {
        println!("  Service '{}' removed.", SERVICE_NAME);
    }

    unsafe {
        CloseServiceHandle(svc);
        CloseServiceHandle(scm);
    }
}

#[cfg(windows)]
fn ctrlc_handler<F: Fn() + Send + 'static>(handler: F) -> Result<(), String> {
    extern "system" {
        fn SetConsoleCtrlHandler(
            HandlerRoutine: Option<extern "system" fn(u32) -> i32>,
            Add: i32,
        ) -> i32;
    }

    use std::sync::Mutex;
    static HANDLER: Mutex<Option<Box<dyn Fn() + Send>>> = Mutex::new(None);

    extern "system" fn ctrl_handler(ctrl_type: u32) -> i32 {
        if ctrl_type == 0 || ctrl_type == 1 {
            if let Ok(guard) = HANDLER.lock() {
                if let Some(ref f) = *guard {
                    f();
                }
            }
            return 1;
        }
        0
    }

    *HANDLER.lock().unwrap() = Some(Box::new(handler));

    let result = unsafe { SetConsoleCtrlHandler(Some(ctrl_handler), 1) };
    if result == 0 {
        Err("Failed to set console ctrl handler".into())
    } else {
        Ok(())
    }
}
