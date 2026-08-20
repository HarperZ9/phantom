//! Machine-readable output payloads for the `--json` flag.
//!
//! Every command that supports JSON emits a single, valid JSON document to
//! stdout. Errors are emitted as JSON as well so scripts can key off `ok=false`
//! without parsing stderr.

use serde::Serialize;

#[derive(Serialize)]
pub struct Envelope<T: Serialize> {
    pub ok: bool,
    pub command: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl<T: Serialize> Envelope<T> {
    pub fn ok(command: &'static str, data: T) -> Self {
        Self {
            ok: true,
            command,
            data: Some(data),
            error: None,
        }
    }

    pub fn error(command: &'static str, err: impl Into<String>) -> Envelope<T> {
        Envelope {
            ok: false,
            command,
            data: None,
            error: Some(err.into()),
        }
    }

    pub fn print(&self) {
        match serde_json::to_string_pretty(self) {
            Ok(s) => println!("{}", s),
            Err(_) => println!("{{\"ok\":false,\"error\":\"serialization failed\"}}"),
        }
    }
}

#[derive(Serialize)]
pub struct StatusPayload {
    pub layers: Vec<LayerStatus>,
    pub data_dir: String,
    pub pipe_name: String,
}

#[derive(Serialize)]
pub struct LayerStatus {
    pub layer: u8,
    pub name: &'static str,
    pub status: String,
}

#[derive(Serialize)]
pub struct LicenseStatusPayload {
    pub tier: String,
    pub licensed: bool,
    pub days_remaining: Option<u32>,
    pub layers_allowed: Vec<u8>,
    pub max_profiles: MaxProfiles,
    pub machine_fingerprint: String,
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum MaxProfiles {
    Limited(u32),
    Unlimited,
}

#[derive(Serialize)]
pub struct ProfileListEntry {
    pub name: String,
    pub seed: String,
    pub identifier_count: usize,
}

#[derive(Serialize)]
pub struct ConfigPayload {
    pub data_dir: String,
    pub pipe_name: String,
    pub log_level: String,
    pub telemetry_enabled: bool,
    pub config_file: String,
    pub config_file_present: bool,
}

#[derive(Serialize)]
pub struct VersionPayload {
    pub name: &'static str,
    pub version: &'static str,
    pub git_commit: &'static str,
    pub target: &'static str,
    pub profile: &'static str,
}
