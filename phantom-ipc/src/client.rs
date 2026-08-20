use crate::message::{Request, Response};
#[cfg(windows)]
use crate::protocol;

#[derive(Debug)]
pub enum ClientError {
    ConnectionFailed(String),
    ProtocolError(String),
    NotSupported,
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientError::ConnectionFailed(e) => write!(f, "connection failed: {}", e),
            ClientError::ProtocolError(e) => write!(f, "protocol error: {}", e),
            ClientError::NotSupported => write!(f, "named pipe IPC requires Windows"),
        }
    }
}

impl std::error::Error for ClientError {}

pub struct PhantomClient {
    #[cfg(windows)]
    stream: crate::transport::PipeStream,
    #[cfg(not(windows))]
    _phantom: std::marker::PhantomData<()>,
}

impl PhantomClient {
    #[cfg(windows)]
    pub fn connect() -> Result<Self, ClientError> {
        Self::connect_to(&crate::pipe_name())
    }

    #[cfg(not(windows))]
    pub fn connect() -> Result<Self, ClientError> {
        Err(ClientError::NotSupported)
    }

    #[cfg(windows)]
    pub fn connect_to(pipe_name: &str) -> Result<Self, ClientError> {
        use std::ffi::CString;

        extern "system" {
            fn CreateFileA(
                lpFileName: *const u8,
                dwDesiredAccess: u32,
                dwShareMode: u32,
                lpSecurityAttributes: *mut u8,
                dwCreationDisposition: u32,
                dwFlagsAndAttributes: u32,
                hTemplateFile: isize,
            ) -> isize;
        }

        const GENERIC_READ: u32 = 0x80000000;
        const GENERIC_WRITE: u32 = 0x40000000;
        const OPEN_EXISTING: u32 = 3;
        const INVALID_HANDLE_VALUE: isize = -1;

        let name = CString::new(pipe_name)
            .map_err(|e| ClientError::ConnectionFailed(format!("invalid pipe name: {}", e)))?;

        let handle = unsafe {
            CreateFileA(
                name.as_ptr() as *const u8,
                GENERIC_READ | GENERIC_WRITE,
                0,
                std::ptr::null_mut(),
                OPEN_EXISTING,
                0,
                0,
            )
        };

        if handle == INVALID_HANDLE_VALUE {
            return Err(ClientError::ConnectionFailed(format!(
                "cannot connect to Phantom service at {}",
                pipe_name,
            )));
        }

        Ok(PhantomClient {
            stream: crate::transport::PipeStream::from_handle(handle, true),
        })
    }

    #[cfg(windows)]
    pub fn request(&mut self, req: &Request) -> Result<Response, ClientError> {
        protocol::send(&mut self.stream, req)
            .map_err(|e| ClientError::ProtocolError(format!("send failed: {}", e)))?;

        protocol::receive(&mut self.stream)
            .map_err(|e| ClientError::ProtocolError(format!("receive failed: {}", e)))
    }

    #[cfg(not(windows))]
    pub fn request(&mut self, _req: &Request) -> Result<Response, ClientError> {
        Err(ClientError::NotSupported)
    }

    pub fn ping(&mut self) -> Result<u32, ClientError> {
        match self.request(&Request::Ping)? {
            Response::Pong { version } => Ok(version),
            Response::Error { message, .. } => {
                Err(ClientError::ProtocolError(format!("ping failed: {}", message)))
            }
            _ => Err(ClientError::ProtocolError("unexpected response".into())),
        }
    }

    pub fn status(&mut self) -> Result<crate::message::ServiceStatus, ClientError> {
        match self.request(&Request::GetStatus)? {
            Response::Status(s) => Ok(s),
            Response::Error { message, .. } => {
                Err(ClientError::ProtocolError(format!("status failed: {}", message)))
            }
            _ => Err(ClientError::ProtocolError("unexpected response".into())),
        }
    }

    pub fn protect(
        &mut self,
        profile_name: &str,
        layers: &[u8],
    ) -> Result<Response, ClientError> {
        self.request(&Request::Protect {
            profile_name: profile_name.into(),
            layers: layers.to_vec(),
        })
    }

    pub fn unprotect(&mut self) -> Result<Response, ClientError> {
        self.request(&Request::Unprotect)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_error_display() {
        let err = ClientError::ConnectionFailed("test".into());
        assert!(format!("{}", err).contains("test"));

        let err = ClientError::NotSupported;
        assert!(format!("{}", err).contains("Windows"));
    }

    #[cfg(not(windows))]
    #[test]
    fn connect_fails_on_non_windows() {
        assert!(matches!(
            PhantomClient::connect(),
            Err(ClientError::NotSupported)
        ));
    }
}
