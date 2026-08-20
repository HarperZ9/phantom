pub mod schema;
pub mod vendor_db;
pub mod engine;

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
    let json = serde_json::to_string_pretty(profile)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    fs::write(&path, json)?;
    Ok(path)
}

pub fn load_profile(name: &str) -> std::io::Result<HardwareProfile> {
    let dir = profiles_dir();
    let filename = format!("{}.json", sanitize_filename(name));
    let path = dir.join(&filename);
    let json = fs::read_to_string(&path)?;
    serde_json::from_str(&json)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
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
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static TEST_COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn unique_test_dir() -> PathBuf {
        let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        std::env::temp_dir().join(format!("phantom-test-profile-{}-{}", std::process::id(), id))
    }

    #[test]
    fn data_dir_and_subdirs_with_filesystem_ops() {
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
