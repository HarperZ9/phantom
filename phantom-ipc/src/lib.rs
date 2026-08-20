pub mod message;
pub mod protocol;
pub mod transport;
pub mod client;
pub mod server;

pub use message::{Request, Response, ServiceStatus, ProfileInfo, ErrorCode};
pub use client::PhantomClient;
pub use server::{PhantomServer, RequestHandler};

pub const PIPE_NAME: &str = r"\\.\pipe\PhantomService";
pub const PROTOCOL_VERSION: u32 = 1;

pub fn pipe_name() -> String {
    std::env::var("PHANTOM_PIPE_NAME").unwrap_or_else(|_| PIPE_NAME.to_string())
}
