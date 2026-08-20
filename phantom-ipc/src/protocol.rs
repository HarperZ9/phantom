use std::io::{Error, ErrorKind, Read, Result, Write};

const MAX_MESSAGE_SIZE: u32 = 4 * 1024 * 1024;

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

pub fn send<W: Write, T: serde::Serialize>(writer: &mut W, msg: &T) -> Result<()> {
    let json = serde_json::to_vec(msg).map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
    write_message(writer, &json)
}

pub fn receive<R: Read, T: serde::de::DeserializeOwned>(reader: &mut R) -> Result<T> {
    let data = read_message(reader)?;
    serde_json::from_slice(&data).map_err(|e| Error::new(ErrorKind::InvalidData, e))
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
}
