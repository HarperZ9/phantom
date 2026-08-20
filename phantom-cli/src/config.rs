//! Layered configuration.
//!
//! Precedence, highest to lowest:
//!   1. Process environment variables (PHANTOM_*)
//!   2. Config file at $PHANTOM_CONFIG or `<data_dir>/config.json`
//!   3. Compiled defaults
//!
//! Enterprise deployments can drop a `config.json` into the data directory
//! for centralized management, and still override any single field per-process
//! via the corresponding env var (SCCM, GPO, Ansible, container overrides).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const DEFAULT_PIPE_NAME: &str = r"\\.\pipe\PhantomService";
pub const DEFAULT_LOG_LEVEL: &str = "info";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PhantomConfig {
    #[serde(default)]
    pub data_dir: Option<String>,
    #[serde(default)]
    pub pipe_name: Option<String>,
    #[serde(default)]
    pub log_level: Option<String>,
    #[serde(default)]
    pub license_key: Option<String>,
    #[serde(default)]
    pub telemetry_enabled: Option<bool>,
}

impl PhantomConfig {
    pub fn empty() -> Self {
        Self {
            data_dir: None,
            pipe_name: None,
            log_level: None,
            license_key: None,
            telemetry_enabled: None,
        }
    }
}

/// Resolve the config file path. `$PHANTOM_CONFIG` wins; otherwise a
/// `config.json` sitting next to the profiles directory is used.
pub fn config_file_path() -> PathBuf {
    if let Ok(p) = std::env::var("PHANTOM_CONFIG") {
        return PathBuf::from(p);
    }
    crate::profile::data_dir().join("config.json")
}

pub fn load_from_file() -> PhantomConfig {
    let path = config_file_path();
    if !path.exists() {
        return PhantomConfig::empty();
    }
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(PhantomConfig::empty)
}

pub fn save_to_file(cfg: &PhantomConfig) -> std::io::Result<PathBuf> {
    let path = config_file_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(cfg)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(&path, json)?;
    Ok(path)
}

/// Resolved runtime config. Env vars override file values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub data_dir: PathBuf,
    pub pipe_name: String,
    pub log_level: String,
    pub telemetry_enabled: bool,
    pub source_file_present: bool,
}

pub fn resolved() -> Resolved {
    let file = load_from_file();
    let source_file_present = config_file_path().exists();

    let data_dir = std::env::var("PHANTOM_DATA_DIR")
        .ok()
        .or_else(|| file.data_dir.clone())
        .map(PathBuf::from)
        .unwrap_or_else(default_data_dir);

    let pipe_name = std::env::var("PHANTOM_PIPE_NAME")
        .ok()
        .or_else(|| file.pipe_name.clone())
        .unwrap_or_else(|| DEFAULT_PIPE_NAME.to_string());

    let log_level = std::env::var("PHANTOM_LOG_LEVEL")
        .ok()
        .or_else(|| std::env::var("RUST_LOG").ok())
        .or_else(|| file.log_level.clone())
        .unwrap_or_else(|| DEFAULT_LOG_LEVEL.to_string());

    let telemetry_enabled = std::env::var("PHANTOM_TELEMETRY")
        .ok()
        .and_then(|v| parse_bool(&v))
        .or(file.telemetry_enabled)
        .unwrap_or(false);

    Resolved {
        data_dir,
        pipe_name,
        log_level,
        telemetry_enabled,
        source_file_present,
    }
}

fn default_data_dir() -> PathBuf {
    if cfg!(windows) {
        std::env::var("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("phantom")
    } else {
        std::env::var("HOME")
            .map(|h| PathBuf::from(h).join(".config"))
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("phantom")
    }
}

fn parse_bool(s: &str) -> Option<bool> {
    match s.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" | "enable" | "enabled" => Some(true),
        "0" | "false" | "no" | "off" | "disable" | "disabled" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static TEST_COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn unique_test_dir() -> PathBuf {
        let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        std::env::temp_dir().join(format!("phantom-cfg-test-{}-{}", std::process::id(), id))
    }

    // All env-var-touching tests are consolidated below into one, and hold
    // a single shared mutex with the profile module tests via
    // `crate::profile::env_test_mutex()`. This prevents races on the
    // process-global env table when tests run in parallel.
    #[test]
    fn env_and_file_resolution_and_roundtrip() {
        let _g = crate::profile::env_test_mutex().lock().unwrap();
        let dir = unique_test_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let cfg_path = dir.join("config.json");

        // Clear everything up front so we get a clean baseline.
        let vars = [
            "PHANTOM_CONFIG",
            "PHANTOM_DATA_DIR",
            "PHANTOM_PIPE_NAME",
            "PHANTOM_LOG_LEVEL",
            "PHANTOM_TELEMETRY",
            "RUST_LOG",
        ];
        for v in vars {
            std::env::remove_var(v);
        }

        // Absent file → empty config.
        std::env::set_var("PHANTOM_CONFIG", cfg_path.to_str().unwrap());
        assert_eq!(load_from_file(), PhantomConfig::empty());

        // Roundtrip a written config.
        let cfg = PhantomConfig {
            data_dir: Some("/opt/phantom".into()),
            pipe_name: Some(r"\\.\pipe\Corp".into()),
            log_level: Some("debug".into()),
            license_key: None,
            telemetry_enabled: Some(false),
        };
        save_to_file(&cfg).unwrap();
        assert_eq!(load_from_file(), cfg);

        // File wins over defaults when env not set.
        save_to_file(&PhantomConfig {
            data_dir: Some("/from/file".into()),
            pipe_name: Some(r"\\.\pipe\FromFile".into()),
            log_level: Some("warn".into()),
            license_key: None,
            telemetry_enabled: Some(true),
        })
        .unwrap();
        let r = resolved();
        assert_eq!(r.data_dir, PathBuf::from("/from/file"));
        assert_eq!(r.pipe_name, r"\\.\pipe\FromFile");
        assert_eq!(r.log_level, "warn");
        assert!(r.telemetry_enabled);
        assert!(r.source_file_present);

        // Env wins over file.
        std::env::set_var("PHANTOM_DATA_DIR", "/from/env");
        std::env::set_var("PHANTOM_PIPE_NAME", r"\\.\pipe\FromEnv");
        std::env::set_var("PHANTOM_LOG_LEVEL", "trace");
        std::env::set_var("PHANTOM_TELEMETRY", "off");
        let r = resolved();
        assert_eq!(r.data_dir, PathBuf::from("/from/env"));
        assert_eq!(r.pipe_name, r"\\.\pipe\FromEnv");
        assert_eq!(r.log_level, "trace");
        assert!(!r.telemetry_enabled);

        for v in vars {
            std::env::remove_var(v);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_bool_variants() {
        assert_eq!(parse_bool("true"), Some(true));
        assert_eq!(parse_bool("YES"), Some(true));
        assert_eq!(parse_bool("1"), Some(true));
        assert_eq!(parse_bool("off"), Some(false));
        assert_eq!(parse_bool("0"), Some(false));
        assert_eq!(parse_bool("maybe"), None);
    }
}
