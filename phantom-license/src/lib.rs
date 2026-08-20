pub mod fingerprint;
pub mod guard;
pub mod integrity;
pub mod key;
pub(crate) mod keys;
pub mod rate_limit;
pub mod time_anchor;
pub mod watermark;

pub use fingerprint::MachineFingerprint;
pub use guard::LicenseGuard;
pub use key::{validate_license_key, License, LicenseError, LicenseTier};

/// Version of the compile-time master key material. Exposed for
/// self-check output so operators can confirm which key generation a
/// given binary was built against.
pub fn master_key_generation() -> u8 {
    keys::master_generation()
}
