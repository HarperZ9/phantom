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
    /// URL for license phone-home. Unset → no calls. Set (either by
    /// operator via `config set` or by the acknowledgment flow if a
    /// compile-time default is baked in) → calls happen every
    /// `phone_home_interval_secs`, subject to `phone_home_enabled`.
    #[serde(default)]
    pub phone_home_url: Option<String>,
    /// Master switch for phone-home. Default (`None`) is treated as
    /// enabled AFTER acknowledgment, disabled before. Setting to
    /// `Some(false)` disables regardless of acknowledgment.
    #[serde(default)]
    pub phone_home_enabled: Option<bool>,
    /// Seconds between phone-home calls. Defaults to
    /// `phone_home::DEFAULT_INTERVAL_SECS` (24h) when unset.
    #[serde(default)]
    pub phone_home_interval_secs: Option<u64>,
    /// Unix seconds when the user acknowledged the current privacy
    /// notice. Absent → notice not yet shown. Non-zero AND
    /// `privacy_notice_version_accepted` >= current version → good.
    #[serde(default)]
    pub privacy_notice_acknowledged_at: Option<u64>,
    /// Which version of the privacy notice the user accepted. A
    /// version bump forces re-acknowledgment.
    #[serde(default)]
    pub privacy_notice_version_accepted: Option<u32>,
    /// Unix seconds when the user accepted the current ToU.
    #[serde(default)]
    pub tou_accepted_at: Option<u64>,
    /// Which version of the ToU the user accepted.
    #[serde(default)]
    pub tou_version_accepted: Option<u32>,
    /// HMAC-SHA256 (hex) over the canonical serialization of this
    /// struct with `config_mac` cleared. Prevents field tampering
    /// (e.g. someone changing `data_dir` to point the license loader
    /// at an attacker-controlled path). Legacy files without the
    /// field load once and are re-signed on next save.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub config_mac: String,
}

impl PhantomConfig {
    pub fn empty() -> Self {
        Self {
            data_dir: None,
            pipe_name: None,
            log_level: None,
            license_key: None,
            telemetry_enabled: None,
            phone_home_url: None,
            phone_home_enabled: None,
            phone_home_interval_secs: None,
            privacy_notice_acknowledged_at: None,
            privacy_notice_version_accepted: None,
            tou_accepted_at: None,
            tou_version_accepted: None,
            config_mac: String::new(),
        }
    }

    /// Has the user acknowledged the currently-shipping privacy
    /// notice? A bump to `PRIVACY_NOTICE_VERSION` invalidates the
    /// previous acknowledgment.
    pub fn privacy_notice_current(&self) -> bool {
        self.privacy_notice_version_accepted
            .is_some_and(|v| v >= phantom_license::legal::PRIVACY_NOTICE_VERSION)
    }

    /// Has the user accepted the currently-shipping ToU?
    pub fn tou_current(&self) -> bool {
        self.tou_version_accepted
            .is_some_and(|v| v >= phantom_license::legal::TOU_VERSION)
    }

    /// Should phone-home fire? Requires: URL set (either by user
    /// or by acknowledgment picking up the compile-time default),
    /// notice acknowledged for the current version, and
    /// `phone_home_enabled` not explicitly false.
    pub fn phone_home_active(&self) -> bool {
        self.phone_home_url
            .as_deref()
            .is_some_and(|u| !u.is_empty())
            && self.privacy_notice_current()
            && self.phone_home_enabled.unwrap_or(true)
    }
}

/// Compile-time default phone-home URL, populated from the
/// `PHANTOM_DEFAULT_PHONE_HOME_URL` env var at build time via
/// `build.rs`. Vendor release builds set this env var; dev builds do
/// not, so `option_env!` returns `None` and the tool ships with no
/// baked endpoint. Operator can always override in the config file.
pub fn compiled_default_phone_home_url() -> Option<&'static str> {
    option_env!("PHANTOM_DEFAULT_PHONE_HOME_URL").filter(|s| !s.is_empty())
}

fn canonical_bytes_for_mac(cfg: &PhantomConfig) -> Vec<u8> {
    let mut stripped = cfg.clone();
    stripped.config_mac.clear();
    serde_json::to_vec(&stripped).unwrap_or_default()
}

fn seal(cfg: &PhantomConfig) -> PhantomConfig {
    let mut sealed = cfg.clone();
    sealed.config_mac.clear();
    let bytes = canonical_bytes_for_mac(&sealed);
    sealed.config_mac = phantom_license::state_mac_hex(&bytes);
    sealed
}

/// Verify the MAC on a loaded config. Legacy configs (empty
/// `config_mac`) pass through so upgrading users don't get their
/// managed settings ignored on first load; they'll be re-sealed on
/// the next save.
fn verify(cfg: &PhantomConfig) -> bool {
    if cfg.config_mac.is_empty() {
        return true;
    }
    let bytes = canonical_bytes_for_mac(cfg);
    phantom_license::verify_state_mac_hex(&bytes, &cfg.config_mac)
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
    let Some(cfg) = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str::<PhantomConfig>(&s).ok())
    else {
        return PhantomConfig::empty();
    };
    // MAC check: if a sealed config was tampered with, we cannot trust
    // any of its fields (an attacker could redirect `data_dir` to a
    // writable path they've planted a fake license into). Fall back to
    // defaults + env, which are the only trustworthy inputs left.
    if !verify(&cfg) {
        return PhantomConfig::empty();
    }
    cfg
}

pub fn save_to_file(cfg: &PhantomConfig) -> std::io::Result<PathBuf> {
    let path = config_file_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Always seal on save. Callers pass a mark-less or pre-sealed
    // struct; `seal` recomputes.
    let sealed = seal(cfg);
    let json = serde_json::to_string_pretty(&sealed)
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
            telemetry_enabled: Some(false),
            ..PhantomConfig::empty()
        };
        save_to_file(&cfg).unwrap();
        // Loaded config carries the freshly-computed MAC; compare
        // fields instead of the whole struct.
        let loaded = load_from_file();
        assert_eq!(loaded.data_dir, cfg.data_dir);
        assert_eq!(loaded.pipe_name, cfg.pipe_name);
        assert_eq!(loaded.log_level, cfg.log_level);
        assert_eq!(loaded.telemetry_enabled, cfg.telemetry_enabled);
        assert!(!loaded.config_mac.is_empty());

        // File wins over defaults when env not set.
        save_to_file(&PhantomConfig {
            data_dir: Some("/from/file".into()),
            pipe_name: Some(r"\\.\pipe\FromFile".into()),
            log_level: Some("warn".into()),
            telemetry_enabled: Some(true),
            ..PhantomConfig::empty()
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

    // Legacy files without a config_mac field must still load; they
    // get re-sealed on the next save. This is the migration path for
    // pre-Sprint-17 installations.
    #[test]
    fn legacy_config_without_mac_is_accepted() {
        let cfg = PhantomConfig {
            data_dir: Some("/opt/legacy".into()),
            log_level: Some("info".into()),
            ..PhantomConfig::empty()
        };
        assert!(verify(&cfg));
    }

    // Tampering with a sealed field must fail the MAC. This is the
    // main protection: an attacker who edits `data_dir` (to redirect
    // license loading) or `license_key` (to plant a foreign key) can
    // no longer make the tool honor the file.
    #[test]
    fn tampered_data_dir_breaks_mac() {
        let mut cfg = seal(&PhantomConfig {
            data_dir: Some("/genuine/path".into()),
            ..PhantomConfig::empty()
        });
        assert!(verify(&cfg));
        cfg.data_dir = Some("/attacker/path".into());
        assert!(!verify(&cfg));
    }

    #[test]
    fn tampered_license_key_breaks_mac() {
        let mut cfg = seal(&PhantomConfig {
            license_key: Some("ORIGINAL-KEY".into()),
            ..PhantomConfig::empty()
        });
        assert!(verify(&cfg));
        cfg.license_key = Some("PLANTED-KEY".into());
        assert!(!verify(&cfg));
    }

    // A garbage MAC of the correct length must be rejected.
    #[test]
    fn forged_mac_rejected() {
        let mut cfg = seal(&PhantomConfig {
            data_dir: Some("/x".into()),
            ..PhantomConfig::empty()
        });
        cfg.config_mac = "00".repeat(32);
        assert!(!verify(&cfg));
    }

    // Acknowledgment gating: the phone-home is inactive until the
    // notice has been accepted for the current version.
    #[test]
    fn phone_home_inactive_without_acknowledgment() {
        let cfg = PhantomConfig {
            phone_home_url: Some("https://example.invalid/cb".into()),
            phone_home_enabled: Some(true),
            ..PhantomConfig::empty()
        };
        assert!(!cfg.phone_home_active());
    }

    #[test]
    fn phone_home_active_after_acknowledgment() {
        let cfg = PhantomConfig {
            phone_home_url: Some("https://example.invalid/cb".into()),
            phone_home_enabled: Some(true),
            privacy_notice_version_accepted: Some(phantom_license::legal::PRIVACY_NOTICE_VERSION),
            privacy_notice_acknowledged_at: Some(1_700_000_000),
            ..PhantomConfig::empty()
        };
        assert!(cfg.phone_home_active());
    }

    // Explicit opt-out overrides an acknowledged state.
    #[test]
    fn explicit_disable_beats_acknowledgment() {
        let cfg = PhantomConfig {
            phone_home_url: Some("https://example.invalid/cb".into()),
            phone_home_enabled: Some(false),
            privacy_notice_version_accepted: Some(phantom_license::legal::PRIVACY_NOTICE_VERSION),
            privacy_notice_acknowledged_at: Some(1_700_000_000),
            ..PhantomConfig::empty()
        };
        assert!(!cfg.phone_home_active());
    }

    // A future version bump invalidates a stale acknowledgment.
    #[test]
    fn stale_notice_version_requires_reacknowledgment() {
        let cfg = PhantomConfig {
            phone_home_url: Some("https://example.invalid/cb".into()),
            phone_home_enabled: Some(true),
            // Pretend the user only ever accepted v0; the shipping
            // version is >= 1.
            privacy_notice_version_accepted: Some(0),
            privacy_notice_acknowledged_at: Some(1_700_000_000),
            ..PhantomConfig::empty()
        };
        assert!(!cfg.privacy_notice_current());
        assert!(!cfg.phone_home_active());
    }
}
