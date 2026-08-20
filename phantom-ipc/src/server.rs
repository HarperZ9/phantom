use crate::message::{Request, Response};
#[cfg(windows)]
use crate::protocol;
use std::sync::atomic::AtomicBool;
#[cfg(windows)]
use std::sync::atomic::Ordering;
use std::sync::Arc;

pub trait RequestHandler: Send {
    fn handle(&mut self, request: Request) -> Response;
}

pub struct PhantomServer {
    #[allow(dead_code)]
    pipe_name: String,
}

impl PhantomServer {
    pub fn new() -> Self {
        PhantomServer {
            pipe_name: crate::pipe_name(),
        }
    }

    pub fn with_pipe_name(name: &str) -> Self {
        PhantomServer {
            pipe_name: name.to_string(),
        }
    }

    #[cfg(windows)]
    pub fn run<H: RequestHandler>(
        &self,
        handler: &mut H,
        shutdown: Arc<AtomicBool>,
    ) -> Result<(), ServerError> {
        use std::ffi::CString;

        extern "system" {
            fn CreateNamedPipeA(
                lpName: *const u8,
                dwOpenMode: u32,
                dwPipeMode: u32,
                nMaxInstances: u32,
                nOutBufferSize: u32,
                nInBufferSize: u32,
                nDefaultTimeOut: u32,
                lpSecurityAttributes: *const SecurityAttributes,
            ) -> isize;
            fn ConnectNamedPipe(hNamedPipe: isize, lpOverlapped: *mut u8) -> i32;
            fn DisconnectNamedPipe(hNamedPipe: isize) -> i32;
            fn CloseHandle(hObject: isize) -> i32;
        }

        const PIPE_ACCESS_DUPLEX: u32 = 0x00000003;
        const PIPE_TYPE_BYTE: u32 = 0x00000000;
        const PIPE_READMODE_BYTE: u32 = 0x00000000;
        const PIPE_WAIT: u32 = 0x00000000;
        const INVALID_HANDLE_VALUE: isize = -1;
        const BUFFER_SIZE: u32 = 64 * 1024;
        const PIPE_TIMEOUT_MS: u32 = 30_000;

        let pipe_cstr = CString::new(self.pipe_name.as_str())
            .map_err(|e| ServerError::BindFailed(format!("invalid pipe name: {}", e)))?;

        let sa = create_pipe_security()?;

        while !shutdown.load(Ordering::Relaxed) {
            let pipe_handle = unsafe {
                CreateNamedPipeA(
                    pipe_cstr.as_ptr() as *const u8,
                    PIPE_ACCESS_DUPLEX,
                    PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
                    255,
                    BUFFER_SIZE,
                    BUFFER_SIZE,
                    PIPE_TIMEOUT_MS,
                    &sa,
                )
            };

            if pipe_handle == INVALID_HANDLE_VALUE {
                return Err(ServerError::BindFailed("CreateNamedPipeA failed".into()));
            }

            let connected = unsafe { ConnectNamedPipe(pipe_handle, std::ptr::null_mut()) };

            if shutdown.load(Ordering::Relaxed) {
                unsafe {
                    DisconnectNamedPipe(pipe_handle);
                    CloseHandle(pipe_handle);
                }
                break;
            }

            if connected != 0 || std::io::Error::last_os_error().raw_os_error() == Some(535) {
                let mut stream = crate::transport::PipeStream::from_handle(pipe_handle, false);

                match protocol::receive::<_, Request>(&mut stream) {
                    Ok(request) => {
                        let is_shutdown = matches!(request, Request::Shutdown);
                        let response = handler.handle(request);
                        let _ = protocol::send(&mut stream, &response);

                        if is_shutdown {
                            shutdown.store(true, Ordering::Relaxed);
                        }
                    }
                    Err(e) => {
                        eprintln!("[phantom-svc] IPC deserialization error: {}", e);
                    }
                }

                unsafe {
                    DisconnectNamedPipe(pipe_handle);
                    CloseHandle(pipe_handle);
                }
            } else {
                unsafe {
                    CloseHandle(pipe_handle);
                }
            }
        }

        drop(sa);
        Ok(())
    }

    #[cfg(not(windows))]
    pub fn run<H: RequestHandler>(
        &self,
        _handler: &mut H,
        _shutdown: Arc<AtomicBool>,
    ) -> Result<(), ServerError> {
        Err(ServerError::NotSupported)
    }
}

#[cfg(windows)]
#[repr(C)]
struct SecurityAttributes {
    length: u32,
    security_descriptor: *mut u8,
    inherit_handle: i32,
}

#[cfg(windows)]
unsafe impl Send for SecurityAttributes {}
#[cfg(windows)]
unsafe impl Sync for SecurityAttributes {}

#[cfg(windows)]
impl Drop for SecurityAttributes {
    fn drop(&mut self) {
        if !self.security_descriptor.is_null() {
            extern "system" {
                fn LocalFree(hMem: *mut u8) -> *mut u8;
            }
            unsafe {
                LocalFree(self.security_descriptor);
            }
        }
    }
}

#[cfg(windows)]
fn create_pipe_security() -> Result<SecurityAttributes, ServerError> {
    extern "system" {
        fn ConvertStringSecurityDescriptorToSecurityDescriptorA(
            StringSecurityDescriptor: *const u8,
            StringSDRevision: u32,
            SecurityDescriptor: *mut *mut u8,
            SecurityDescriptorSize: *mut u32,
        ) -> i32;
    }

    // SDDL: SYSTEM=full, Administrators=full, Users=read+write
    let sddl = b"D:(A;;GA;;;SY)(A;;GA;;;BA)(A;;GRGW;;;BU)\0";
    let mut sd: *mut u8 = std::ptr::null_mut();

    let ok = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorA(
            sddl.as_ptr(),
            1, // SDDL_REVISION_1
            &mut sd,
            std::ptr::null_mut(),
        )
    };

    if ok == 0 {
        return Err(ServerError::BindFailed(
            "failed to create pipe security descriptor".into(),
        ));
    }

    Ok(SecurityAttributes {
        length: std::mem::size_of::<SecurityAttributes>() as u32,
        security_descriptor: sd,
        inherit_handle: 0,
    })
}

#[derive(Debug)]
pub enum ServerError {
    BindFailed(String),
    IoError(String),
    NotSupported,
}

impl std::fmt::Display for ServerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServerError::BindFailed(e) => write!(f, "failed to bind pipe: {}", e),
            ServerError::IoError(e) => write!(f, "I/O error: {}", e),
            ServerError::NotSupported => write!(f, "named pipe server requires Windows"),
        }
    }
}

impl std::error::Error for ServerError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::ErrorCode;

    struct EchoHandler;
    impl RequestHandler for EchoHandler {
        fn handle(&mut self, request: Request) -> Response {
            match request {
                Request::Ping => Response::Pong {
                    version: crate::PROTOCOL_VERSION,
                },
                Request::GetStatus => Response::Status(crate::message::ServiceStatus::default()),
                _ => Response::Error {
                    code: ErrorCode::InvalidRequest,
                    message: "not implemented".into(),
                },
            }
        }
    }

    #[test]
    fn echo_handler_ping() {
        let mut handler = EchoHandler;
        let resp = handler.handle(Request::Ping);
        assert!(matches!(resp, Response::Pong { version: 1 }));
    }

    #[test]
    fn echo_handler_status() {
        let mut handler = EchoHandler;
        let resp = handler.handle(Request::GetStatus);
        match resp {
            Response::Status(s) => assert!(!s.protected),
            _ => panic!("expected Status"),
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn server_not_supported_on_non_windows() {
        let server = PhantomServer::new();
        let shutdown = Arc::new(AtomicBool::new(false));
        let mut handler = EchoHandler;
        assert!(matches!(
            server.run(&mut handler, shutdown),
            Err(ServerError::NotSupported)
        ));
    }
}
