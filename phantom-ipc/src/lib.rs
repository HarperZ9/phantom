pub mod client;
pub mod message;
pub mod protocol;
pub mod server;
pub mod transport;

pub use client::PhantomClient;
pub use message::{ErrorCode, ProfileInfo, Request, Response, ServiceStatus};
pub use server::{PhantomServer, RequestHandler};

pub const PIPE_NAME: &str = r"\\.\pipe\PhantomService";
/// Protocol version 2: every framed message carries a 32-byte
/// HMAC-SHA256 signature under the STATE_PURPOSE subkey between the
/// length prefix and the JSON payload. A v1 peer talking to a v2
/// peer will see MAC-verification failures and disconnect.
pub const PROTOCOL_VERSION: u32 = 2;

pub fn pipe_name() -> String {
    std::env::var("PHANTOM_PIPE_NAME").unwrap_or_else(|_| PIPE_NAME.to_string())
}
