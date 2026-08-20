//! Compile-time build metadata baked in by `build.rs`.
//!
//! Enterprise support requires the git commit and target triple —
//! without them, "the tool is misbehaving" is unanswerable across
//! rebuilds. All values are `&'static str` so there is zero runtime
//! cost.

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const GIT_COMMIT: &str = env!("PHANTOM_GIT_COMMIT");
pub const BUILD_TARGET: &str = env!("PHANTOM_BUILD_TARGET");
pub const BUILD_PROFILE: &str = env!("PHANTOM_BUILD_PROFILE");
pub const PKG_NAME: &str = env!("CARGO_PKG_NAME");

pub fn full_version_string() -> String {
    format!(
        "{} {} ({}) built for {} in {} mode",
        PKG_NAME, VERSION, GIT_COMMIT, BUILD_TARGET, BUILD_PROFILE
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_string_is_populated() {
        assert!(!VERSION.is_empty());
        assert!(!GIT_COMMIT.is_empty());
        assert!(!BUILD_TARGET.is_empty());
        assert!(!BUILD_PROFILE.is_empty());
    }

    #[test]
    fn full_version_string_contains_all_fields() {
        let s = full_version_string();
        assert!(s.contains(VERSION));
        assert!(s.contains(GIT_COMMIT));
        assert!(s.contains(BUILD_TARGET));
        assert!(s.contains(BUILD_PROFILE));
    }
}
