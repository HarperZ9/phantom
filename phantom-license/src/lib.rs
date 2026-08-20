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

/// HMAC-SHA256 of `data` under the STATE_PURPOSE subkey, hex-encoded.
///
/// Callers use this to attach a tamper seal to a config or state
/// document they own (typically the JSON serialization of that
/// document with the `mac` field cleared). Verify with
/// [`verify_state_mac_hex`] before trusting the document.
pub fn state_mac_hex(data: &[u8]) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    type HmacSha256 = Hmac<Sha256>;
    let sk = keys::derive_key(keys::STATE_PURPOSE);
    let mut mac = HmacSha256::new_from_slice(&sk).expect("HMAC key length is fixed");
    mac.update(data);
    let bytes = mac.finalize().into_bytes();
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Constant-time verification of a hex-encoded state MAC. Returns
/// `false` on any mismatch, including a length mismatch, without
/// leaking timing about where the difference is.
pub fn verify_state_mac_hex(data: &[u8], expected_hex: &str) -> bool {
    let actual = state_mac_hex(data);
    let a = actual.as_bytes();
    let b = expected_hex.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}
