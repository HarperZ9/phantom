use std::io::{Error, ErrorKind, Read, Result, Write};

const MAX_MESSAGE_SIZE: u32 = 4 * 1024 * 1024;
const MAC_LEN: usize = 32;

/// Raw framing without a MAC. Retained for parser fuzzing and any
/// bootstrap flows that predate authentication; do NOT use for real
/// client/server traffic — use `send`/`receive`, which sign every
/// frame under the STATE_PURPOSE subkey.
pub fn write_message<W: Write>(writer: &mut W, data: &[u8]) -> Result<()> {
    let len = data.len() as u32;
    if len > MAX_MESSAGE_SIZE {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "message exceeds maximum size",
        ));
    }
    writer.write_all(&len.to_le_bytes())?;
    writer.write_all(data)?;
    writer.flush()
}

pub fn read_message<R: Read>(reader: &mut R) -> Result<Vec<u8>> {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf)?;
    let len = u32::from_le_bytes(len_buf);
    if len > MAX_MESSAGE_SIZE {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("message too large: {} bytes", len),
        ));
    }
    let mut buf = vec![0u8; len as usize];
    reader.read_exact(&mut buf)?;
    Ok(buf)
}

/// Send an authenticated message.
///
/// Wire format: `[u32 LE total_len] [32-byte HMAC-SHA256] [JSON body]`
/// where `total_len = MAC_LEN + body.len()`. The MAC covers the body
/// bytes; a length tamper is caught trivially by the read-exact + MAC
/// combo on the far side.
///
/// Attackers who can talk to the pipe endpoint (SYSTEM on Windows,
/// root on Linux) still cannot forge a payload — the STATE_PURPOSE
/// subkey is derived only inside the Phantom binaries and is never
/// exposed on the wire.
pub fn send<W: Write, T: serde::Serialize>(writer: &mut W, msg: &T) -> Result<()> {
    let json = serde_json::to_vec(msg).map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
    let mac_hex = phantom_license::state_mac_hex(&json);
    let mac_bytes = hex_to_mac_bytes(&mac_hex)
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "state_mac_hex returned non-hex"))?;

    let total_len = (MAC_LEN + json.len()) as u32;
    if total_len > MAX_MESSAGE_SIZE {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "message exceeds maximum size",
        ));
    }
    writer.write_all(&total_len.to_le_bytes())?;
    writer.write_all(&mac_bytes)?;
    writer.write_all(&json)?;
    writer.flush()
}

/// Receive an authenticated message. Fails when the MAC does not
/// verify — the wire is treated as attacker-controlled and any frame
/// without a valid signature is dropped, not deserialized.
pub fn receive<R: Read, T: serde::de::DeserializeOwned>(reader: &mut R) -> Result<T> {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf)?;
    let total_len = u32::from_le_bytes(len_buf) as usize;
    if total_len > MAX_MESSAGE_SIZE as usize {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("message too large: {} bytes", total_len),
        ));
    }
    if total_len < MAC_LEN {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "framed message shorter than MAC length",
        ));
    }

    let mut mac = [0u8; MAC_LEN];
    reader.read_exact(&mut mac)?;
    let payload_len = total_len - MAC_LEN;
    let mut payload = vec![0u8; payload_len];
    reader.read_exact(&mut payload)?;

    let mac_hex: String = mac.iter().map(|b| format!("{:02x}", b)).collect();
    if !phantom_license::verify_state_mac_hex(&payload, &mac_hex) {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "message MAC verification failed",
        ));
    }

    serde_json::from_slice(&payload).map_err(|e| Error::new(ErrorKind::InvalidData, e))
}

fn hex_to_mac_bytes(hex: &str) -> Option<[u8; MAC_LEN]> {
    if hex.len() != 2 * MAC_LEN {
        return None;
    }
    let mut out = [0u8; MAC_LEN];
    for i in 0..MAC_LEN {
        out[i] = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).ok()?;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{Request, Response, ServiceStatus};
    use std::io::Cursor;

    #[test]
    fn raw_roundtrip() {
        let data = b"hello, phantom";
        let mut buf = Vec::new();
        write_message(&mut buf, data).unwrap();

        assert_eq!(buf.len(), 4 + data.len());
        let len = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
        assert_eq!(len, data.len() as u32);

        let mut cursor = Cursor::new(buf);
        let result = read_message(&mut cursor).unwrap();
        assert_eq!(result, data);
    }

    #[test]
    fn rejects_oversized_length() {
        let mut buf = Vec::new();
        let fake_len: u32 = MAX_MESSAGE_SIZE + 1;
        buf.extend_from_slice(&fake_len.to_le_bytes());

        let mut cursor = Cursor::new(buf);
        assert!(read_message(&mut cursor).is_err());
    }

    #[test]
    fn empty_message() {
        let mut buf = Vec::new();
        write_message(&mut buf, b"").unwrap();

        let mut cursor = Cursor::new(buf);
        let result = read_message(&mut cursor).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn request_send_receive() {
        let req = Request::Protect {
            profile_name: "test-profile".into(),
            layers: vec![1, 2],
        };

        let mut buf = Vec::new();
        send(&mut buf, &req).unwrap();

        let mut cursor = Cursor::new(buf);
        let decoded: Request = receive(&mut cursor).unwrap();

        match decoded {
            Request::Protect {
                profile_name,
                layers,
            } => {
                assert_eq!(profile_name, "test-profile");
                assert_eq!(layers, vec![1, 2]);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn response_send_receive() {
        let resp = Response::Status(ServiceStatus {
            protected: true,
            active_profile: Some("my-profile".into()),
            active_layers: vec![1, 2],
            uptime_secs: 3600,
            driver_connected: true,
            firmware_detected: false,
            identifier_count: 30,
        });

        let mut buf = Vec::new();
        send(&mut buf, &resp).unwrap();

        let mut cursor = Cursor::new(buf);
        let decoded: Response = receive(&mut cursor).unwrap();

        match decoded {
            Response::Status(s) => {
                assert!(s.protected);
                assert_eq!(s.active_profile.as_deref(), Some("my-profile"));
                assert_eq!(s.uptime_secs, 3600);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn multiple_messages_on_same_stream() {
        let mut buf = Vec::new();
        send(&mut buf, &Request::Ping).unwrap();
        send(&mut buf, &Request::GetStatus).unwrap();
        send(&mut buf, &Request::Unprotect).unwrap();

        let mut cursor = Cursor::new(buf);
        let r1: Request = receive(&mut cursor).unwrap();
        let r2: Request = receive(&mut cursor).unwrap();
        let r3: Request = receive(&mut cursor).unwrap();

        assert!(matches!(r1, Request::Ping));
        assert!(matches!(r2, Request::GetStatus));
        assert!(matches!(r3, Request::Unprotect));
    }

    #[test]
    fn truncated_stream_errors() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&10u32.to_le_bytes());
        buf.extend_from_slice(b"short");

        let mut cursor = Cursor::new(buf);
        assert!(read_message(&mut cursor).is_err());
    }

    // The signed wire format must be at least 4 bytes for the length
    // plus 32 bytes for the MAC — any smaller frame is malformed. A
    // v1 client sending a bare 4-byte-prefixed frame trips this.
    #[test]
    fn signed_receive_rejects_frame_shorter_than_mac() {
        let mut buf = Vec::new();
        // Claim 10 bytes total but MAC is 32 bytes → structural reject.
        buf.extend_from_slice(&10u32.to_le_bytes());
        buf.extend_from_slice(b"1234567890");
        let mut cursor = Cursor::new(buf);
        let r: Result<Request> = receive(&mut cursor);
        assert!(r.is_err());
    }

    // Tampering with a single byte of the payload must fail the MAC.
    // This is the core protection against an attacker who has taken
    // over the pipe endpoint and is trying to inject a Protect or
    // Unprotect command.
    #[test]
    fn tampered_payload_fails_mac() {
        let req = Request::Protect {
            profile_name: "genuine".into(),
            layers: vec![2],
        };
        let mut buf = Vec::new();
        send(&mut buf, &req).unwrap();

        // Payload starts at offset 4 (len) + 32 (mac) = 36. Flip a
        // byte inside the JSON.
        assert!(buf.len() > 40);
        buf[38] ^= 0x01;

        let mut cursor = Cursor::new(buf);
        let r: Result<Request> = receive(&mut cursor);
        assert!(r.is_err());
    }

    // Tampering with the MAC bytes themselves must also fail. An
    // attacker who knows the payload shape but not the key can't
    // brute-force a MAC.
    #[test]
    fn tampered_mac_fails_verification() {
        let mut buf = Vec::new();
        send(&mut buf, &Request::Ping).unwrap();
        // MAC lives at offset 4 through 4+32.
        buf[4] ^= 0xFF;
        let mut cursor = Cursor::new(buf);
        let r: Result<Request> = receive(&mut cursor);
        assert!(r.is_err());
    }

    // Frames written with the unauthenticated `write_message` cannot
    // masquerade as signed frames — the MAC bytes will be JSON
    // content, which is astronomically unlikely to verify.
    #[test]
    fn unsigned_frame_is_not_accepted_by_signed_receiver() {
        let mut buf = Vec::new();
        write_message(&mut buf, b"{\"type\":\"Ping\"}").unwrap();
        let mut cursor = Cursor::new(buf);
        let r: Result<Request> = receive(&mut cursor);
        assert!(r.is_err());
    }

    // Every frame carries a fresh MAC — even for identical payloads,
    // the wire bytes are (obviously) identical because the MAC is
    // deterministic in the payload. This test pins that behavior so a
    // future change to add a nonce is a deliberate wire-format bump.
    #[test]
    fn signed_frames_are_deterministic_per_payload() {
        let mut a = Vec::new();
        let mut b = Vec::new();
        send(&mut a, &Request::Ping).unwrap();
        send(&mut b, &Request::Ping).unwrap();
        assert_eq!(a, b);
    }
}
