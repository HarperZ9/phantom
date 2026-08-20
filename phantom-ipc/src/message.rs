use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Request {
    Ping,
    GetStatus,
    Protect {
        profile_name: String,
        layers: Vec<u8>,
    },
    Unprotect,
    ListProfiles,
    GetProfile {
        name: String,
    },
    GenerateProfile {
        name: String,
        seed: String,
    },
    DeleteProfile {
        name: String,
    },
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Response {
    Pong {
        version: u32,
    },
    Status(ServiceStatus),
    Profiles {
        list: Vec<ProfileInfo>,
    },
    Profile {
        data: serde_json::Value,
    },
    Applied {
        layers_applied: Vec<u8>,
        identifiers: usize,
    },
    Reverted {
        #[serde(default)]
        warnings: Vec<String>,
    },
    Generated {
        name: String,
        identifiers: usize,
    },
    Deleted {
        name: String,
    },
    Ok {
        message: String,
    },
    Error {
        code: ErrorCode,
        message: String,
    },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServiceStatus {
    pub protected: bool,
    pub active_profile: Option<String>,
    pub active_layers: Vec<u8>,
    pub uptime_secs: u64,
    pub driver_connected: bool,
    pub firmware_detected: bool,
    #[serde(default)]
    pub identifier_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileInfo {
    pub name: String,
    pub seed: String,
    pub identifier_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ErrorCode {
    ProfileNotFound,
    DriverError,
    PermissionDenied,
    InvalidRequest,
    InternalError,
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorCode::ProfileNotFound => write!(f, "PROFILE_NOT_FOUND"),
            ErrorCode::DriverError => write!(f, "DRIVER_ERROR"),
            ErrorCode::PermissionDenied => write!(f, "PERMISSION_DENIED"),
            ErrorCode::InvalidRequest => write!(f, "INVALID_REQUEST"),
            ErrorCode::InternalError => write!(f, "INTERNAL_ERROR"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_request_variants_roundtrip() {
        let requests = vec![
            Request::Ping,
            Request::GetStatus,
            Request::Protect {
                profile_name: "test".into(),
                layers: vec![0, 1, 2],
            },
            Request::Unprotect,
            Request::ListProfiles,
            Request::GetProfile {
                name: "test".into(),
            },
            Request::GenerateProfile {
                name: "gen".into(),
                seed: "seed".into(),
            },
            Request::DeleteProfile {
                name: "del".into(),
            },
            Request::Shutdown,
        ];

        for req in &requests {
            let json = serde_json::to_string(req).unwrap();
            let decoded: Request = serde_json::from_str(&json).unwrap();
            assert_eq!(
                std::mem::discriminant(req),
                std::mem::discriminant(&decoded),
            );
        }
    }

    #[test]
    fn all_response_variants_roundtrip() {
        let responses: Vec<Response> = vec![
            Response::Pong { version: 1 },
            Response::Status(ServiceStatus {
                protected: true,
                active_profile: Some("p".into()),
                active_layers: vec![1, 2],
                uptime_secs: 60,
                driver_connected: true,
                firmware_detected: false,
                identifier_count: 30,
            }),
            Response::Profiles {
                list: vec![ProfileInfo {
                    name: "p".into(),
                    seed: "s".into(),
                    identifier_count: 30,
                }],
            },
            Response::Profile {
                data: serde_json::json!({"test": true}),
            },
            Response::Applied {
                layers_applied: vec![2],
                identifiers: 6,
            },
            Response::Reverted { warnings: vec![] },
            Response::Generated {
                name: "new".into(),
                identifiers: 32,
            },
            Response::Deleted {
                name: "old".into(),
            },
            Response::Ok {
                message: "done".into(),
            },
            Response::Error {
                code: ErrorCode::ProfileNotFound,
                message: "not found".into(),
            },
        ];

        for resp in &responses {
            let json = serde_json::to_string(resp).unwrap();
            let decoded: Response = serde_json::from_str(&json).unwrap();
            assert_eq!(
                std::mem::discriminant(resp),
                std::mem::discriminant(&decoded),
            );
        }
    }

    #[test]
    fn status_defaults_to_unprotected() {
        let status = ServiceStatus::default();
        assert!(!status.protected);
        assert!(status.active_profile.is_none());
        assert!(status.active_layers.is_empty());
        assert_eq!(status.uptime_secs, 0);
        assert!(!status.driver_connected);
        assert!(!status.firmware_detected);
        assert_eq!(status.identifier_count, 0);
    }

    #[test]
    fn error_code_display() {
        assert_eq!(format!("{}", ErrorCode::ProfileNotFound), "PROFILE_NOT_FOUND");
        assert_eq!(format!("{}", ErrorCode::InternalError), "INTERNAL_ERROR");
    }
}
