pub mod engine;
pub mod schema;
pub mod vendor_db;

use schema::HardwareProfile;
use std::fs;
use std::path::PathBuf;

pub fn data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("PHANTOM_DATA_DIR") {
        return PathBuf::from(dir);
    }
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

pub fn profiles_dir() -> PathBuf {
    data_dir().join("profiles")
}

pub fn logs_dir() -> PathBuf {
    data_dir().join("logs")
}

pub fn ensure_profiles_dir() -> std::io::Result<PathBuf> {
    let dir = profiles_dir();
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn save_profile(profile: &HardwareProfile) -> std::io::Result<PathBuf> {
    let dir = ensure_profiles_dir()?;
    let filename = format!("{}.json", sanitize_filename(&profile.metadata.name));
    let path = dir.join(&filename);

    // Sign the profile before writing. Callers pass a mark-less
    // profile; we strip any existing mark for canonical hashing,
    // compute the mark, then persist the marked form.
    let signed = sign_profile(profile.clone());
    let json = serde_json::to_string_pretty(&signed)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    fs::write(&path, json)?;
    Ok(path)
}

pub fn load_profile(name: &str) -> std::io::Result<HardwareProfile> {
    let dir = profiles_dir();
    let filename = format!("{}.json", sanitize_filename(name));
    let path = dir.join(&filename);
    let json = fs::read_to_string(&path)?;
    serde_json::from_str(&json).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

/// Verdict from checking a profile's origin mark against this machine.
///
/// `Unmarked` is a first-class outcome: pre-Sprint-14 profiles and
/// hand-authored profiles legitimately have no mark. Callers decide
/// what policy to apply per outcome (typically: import from another
/// machine requires Pro or higher).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportVerdict {
    Unmarked,
    Local,
    Foreign { origin_tier: String },
    ContentTampered,
    Invalid,
    Malformed,
}

/// Compute the canonical bytes a mark should cover: the profile
/// serialized with `origin_mark` cleared. Any change to the profile
/// content — even a re-indent — that a caller does before saving must
/// be reflected here so `verify` sees the same hash both sides.
fn canonical_bytes(profile: &HardwareProfile) -> Vec<u8> {
    let mut stripped = profile.clone();
    stripped.metadata.origin_mark = None;
    serde_json::to_vec(&stripped).unwrap_or_default()
}

pub fn sign_profile(mut profile: HardwareProfile) -> HardwareProfile {
    // Strip any prior mark so the hash is stable across re-signings.
    profile.metadata.origin_mark = None;
    let canonical = serde_json::to_vec(&profile).unwrap_or_default();
    let guard = phantom_license::LicenseGuard::load();
    let fp = phantom_license::MachineFingerprint::collect();
    let tier = guard.tier().to_string();
    // Rough issued-days marker; epoch-days precision is deliberate —
    // marks are for provenance, not for expiration.
    let issued_days = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| (d.as_secs() / 86400) as u32)
        .unwrap_or(0);
    let mark = phantom_license::watermark::sign_bytes(&canonical, fp.hash, &tier, issued_days);
    profile.metadata.origin_mark = Some(mark);
    profile
}

/// Verify a profile's origin mark against this machine's fingerprint.
pub fn check_origin(profile: &HardwareProfile) -> ImportVerdict {
    let Some(mark) = &profile.metadata.origin_mark else {
        return ImportVerdict::Unmarked;
    };
    let canonical = canonical_bytes(profile);
    let mut hasher = sha2::Sha256::new();
    use sha2::Digest;
    hasher.update(&canonical);
    let mut profile_hash = [0u8; 32];
    profile_hash.copy_from_slice(&hasher.finalize());

    let fp = phantom_license::MachineFingerprint::collect();
    match phantom_license::watermark::verify(mark, &profile_hash, &fp.hash) {
        phantom_license::watermark::Verdict::Local => ImportVerdict::Local,
        phantom_license::watermark::Verdict::Foreign { origin_tier } => {
            ImportVerdict::Foreign { origin_tier }
        }
        phantom_license::watermark::Verdict::ContentTampered => ImportVerdict::ContentTampered,
        phantom_license::watermark::Verdict::Invalid => ImportVerdict::Invalid,
        phantom_license::watermark::Verdict::Malformed => ImportVerdict::Malformed,
    }
}

pub fn list_profiles() -> std::io::Result<Vec<String>> {
    let dir = profiles_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut names = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                names.push(stem.to_string());
            }
        }
    }
    names.sort();
    Ok(names)
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

// Shared serialization gate for tests that mutate the process-global
// environment table. The config module's tests live in the same test
// binary and need to hold the same lock.
static ENV_TEST_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());
pub fn env_test_mutex() -> &'static std::sync::Mutex<()> {
    &ENV_TEST_MUTEX
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static TEST_COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn unique_test_dir() -> PathBuf {
        let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        std::env::temp_dir().join(format!(
            "phantom-test-profile-{}-{}",
            std::process::id(),
            id
        ))
    }

    #[test]
    fn data_dir_and_subdirs_with_filesystem_ops() {
        let _g = env_test_mutex().lock().unwrap();
        let dir = unique_test_dir();
        std::env::set_var("PHANTOM_DATA_DIR", dir.to_str().unwrap());

        assert_eq!(data_dir(), dir);
        assert_eq!(profiles_dir(), dir.join("profiles"));
        assert_eq!(logs_dir(), dir.join("logs"));

        let prof = engine::generate_profile("test-seed", "alpha");
        save_profile(&prof).expect("save alpha");

        let loaded = load_profile("alpha").expect("load alpha");
        assert_eq!(loaded.metadata.name, "alpha");
        assert_eq!(loaded.metadata.seed, "test-seed");

        let prof2 = engine::generate_profile("seed-2", "beta");
        save_profile(&prof2).expect("save beta");

        let names = list_profiles().expect("list");
        assert_eq!(names, vec!["alpha", "beta"]);

        let _ = std::fs::remove_dir_all(&dir);
        std::env::remove_var("PHANTOM_DATA_DIR");
    }

    #[test]
    fn sanitize_filename_replaces_special_chars() {
        assert_eq!(sanitize_filename("hello world!"), "hello_world_");
        assert_eq!(sanitize_filename("test-profile_1"), "test-profile_1");
        assert_eq!(sanitize_filename("../../etc"), "______etc");
    }
}
