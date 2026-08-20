use crate::fingerprint::MachineFingerprint;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use thiserror::Error;

type HmacSha256 = Hmac<Sha256>;

const KEY_VERSION: u8 = 1;
const SIGNING_KEY: &[u8] = b"phantom-license-hmac-v1-key";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LicenseTier {
    Free,
    Pro,
    Enterprise,
}

impl LicenseTier {
    pub fn allows_layer(&self, layer: u8) -> bool {
        match self {
            LicenseTier::Free => layer == 2,
            LicenseTier::Pro | LicenseTier::Enterprise => true,
        }
    }

    pub fn max_profiles(&self) -> usize {
        match self {
            LicenseTier::Free => 2,
            LicenseTier::Pro => 50,
            LicenseTier::Enterprise => usize::MAX,
        }
    }

    pub fn allows_service(&self) -> bool {
        matches!(self, LicenseTier::Pro | LicenseTier::Enterprise)
    }

    fn to_byte(self) -> u8 {
        match self {
            LicenseTier::Free => 0,
            LicenseTier::Pro => 1,
            LicenseTier::Enterprise => 2,
        }
    }

    fn from_byte(b: u8) -> Option<Self> {
        match b {
            0 => Some(LicenseTier::Free),
            1 => Some(LicenseTier::Pro),
            2 => Some(LicenseTier::Enterprise),
            _ => None,
        }
    }
}

impl std::fmt::Display for LicenseTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LicenseTier::Free => write!(f, "Free"),
            LicenseTier::Pro => write!(f, "Pro"),
            LicenseTier::Enterprise => write!(f, "Enterprise"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct License {
    pub tier: LicenseTier,
    pub expires_epoch_days: u32,
    pub machine_hash: [u8; 16],
    pub issued_epoch_days: u32,
}

impl License {
    pub fn is_expired(&self) -> bool {
        if self.expires_epoch_days == 0 {
            return false;
        }
        let now = current_epoch_days();
        now > self.expires_epoch_days
    }

    pub fn days_remaining(&self) -> Option<u32> {
        if self.expires_epoch_days == 0 {
            return None;
        }
        let now = current_epoch_days();
        if now > self.expires_epoch_days {
            Some(0)
        } else {
            Some(self.expires_epoch_days - now)
        }
    }

    pub fn is_bound_to(&self, fp: &MachineFingerprint) -> bool {
        self.machine_hash == fp.hash
    }
}

#[derive(Debug, Error)]
pub enum LicenseError {
    #[error("invalid license key format")]
    InvalidFormat,
    #[error("license key signature verification failed")]
    InvalidSignature,
    #[error("unsupported license version {0}")]
    UnsupportedVersion(u8),
    #[error("license has expired")]
    Expired,
    #[error("license is bound to a different machine")]
    MachineMismatch,
    #[error("license tier does not permit this operation")]
    InsufficientTier,
    #[error("unknown license tier code {0}")]
    UnknownTier(u8),
}

// Key layout (37 bytes raw, before base32):
//   [0]      version (1)
//   [1]      tier
//   [2..6]   expires_epoch_days (LE u32, 0=perpetual)
//   [6..10]  issued_epoch_days (LE u32)
//   [10..26] machine_hash (16 bytes)
//   [26..28] reserved (zeroed)
//   [28..60] HMAC-SHA256 of bytes [0..28]
const RAW_PAYLOAD_LEN: usize = 28;
const RAW_HMAC_LEN: usize = 32;
const RAW_TOTAL_LEN: usize = RAW_PAYLOAD_LEN + RAW_HMAC_LEN;

pub fn generate_license_key(
    tier: LicenseTier,
    machine: &MachineFingerprint,
    expires_epoch_days: u32,
) -> String {
    let issued = current_epoch_days();
    let mut raw = [0u8; RAW_TOTAL_LEN];

    raw[0] = KEY_VERSION;
    raw[1] = tier.to_byte();
    raw[2..6].copy_from_slice(&expires_epoch_days.to_le_bytes());
    raw[6..10].copy_from_slice(&issued.to_le_bytes());
    raw[10..26].copy_from_slice(&machine.hash);

    let mut mac = HmacSha256::new_from_slice(SIGNING_KEY).unwrap();
    mac.update(&raw[..RAW_PAYLOAD_LEN]);
    let sig = mac.finalize().into_bytes();
    raw[RAW_PAYLOAD_LEN..].copy_from_slice(&sig);

    let encoded = base32_encode(&raw);
    format_key_display(&encoded)
}

pub fn validate_license_key(key_str: &str) -> Result<License, LicenseError> {
    let clean: String = key_str.chars().filter(|c| c.is_alphanumeric()).collect();
    let raw = base32_decode(&clean).ok_or(LicenseError::InvalidFormat)?;

    if raw.len() != RAW_TOTAL_LEN {
        return Err(LicenseError::InvalidFormat);
    }

    let version = raw[0];
    if version != KEY_VERSION {
        return Err(LicenseError::UnsupportedVersion(version));
    }

    let mut mac = HmacSha256::new_from_slice(SIGNING_KEY).unwrap();
    mac.update(&raw[..RAW_PAYLOAD_LEN]);
    mac.verify_slice(&raw[RAW_PAYLOAD_LEN..])
        .map_err(|_| LicenseError::InvalidSignature)?;

    let tier_byte = raw[1];
    let tier = LicenseTier::from_byte(tier_byte).ok_or(LicenseError::UnknownTier(tier_byte))?;

    let expires = u32::from_le_bytes([raw[2], raw[3], raw[4], raw[5]]);
    let issued = u32::from_le_bytes([raw[6], raw[7], raw[8], raw[9]]);

    let mut machine_hash = [0u8; 16];
    machine_hash.copy_from_slice(&raw[10..26]);

    Ok(License {
        tier,
        expires_epoch_days: expires,
        machine_hash,
        issued_epoch_days: issued,
    })
}

fn current_epoch_days() -> u32 {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    (secs / 86400) as u32
}

fn format_key_display(encoded: &str) -> String {
    encoded
        .as_bytes()
        .chunks(5)
        .map(|c| std::str::from_utf8(c).unwrap_or(""))
        .collect::<Vec<_>>()
        .join("-")
}

const BASE32_ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

fn base32_encode(data: &[u8]) -> String {
    let mut result = String::new();
    let mut buffer: u64 = 0;
    let mut bits = 0;

    for &byte in data {
        buffer = (buffer << 8) | byte as u64;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            let idx = ((buffer >> bits) & 0x1F) as usize;
            result.push(BASE32_ALPHABET[idx] as char);
        }
    }

    if bits > 0 {
        let idx = ((buffer << (5 - bits)) & 0x1F) as usize;
        result.push(BASE32_ALPHABET[idx] as char);
    }

    result
}

fn base32_decode(input: &str) -> Option<Vec<u8>> {
    let mut result = Vec::new();
    let mut buffer: u64 = 0;
    let mut bits = 0;

    for ch in input.chars() {
        let val = match ch {
            'A'..='Z' => ch as u8 - b'A',
            'a'..='z' => ch as u8 - b'a',
            '2'..='7' => ch as u8 - b'2' + 26,
            _ => return None,
        };
        buffer = (buffer << 5) | val as u64;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            result.push((buffer >> bits) as u8);
        }
    }

    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_fingerprint() -> MachineFingerprint {
        MachineFingerprint::from_bytes(&[
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E,
            0x0F, 0x10,
        ])
    }

    #[test]
    fn generate_and_validate_roundtrip() {
        let fp = test_fingerprint();
        let key = generate_license_key(LicenseTier::Pro, &fp, 0);
        let license = validate_license_key(&key).unwrap();

        assert_eq!(license.tier, LicenseTier::Pro);
        assert_eq!(license.expires_epoch_days, 0);
        assert!(license.is_bound_to(&fp));
        assert!(!license.is_expired());
    }

    #[test]
    fn perpetual_license_never_expires() {
        let fp = test_fingerprint();
        let key = generate_license_key(LicenseTier::Enterprise, &fp, 0);
        let license = validate_license_key(&key).unwrap();

        assert!(!license.is_expired());
        assert!(license.days_remaining().is_none());
    }

    #[test]
    fn expired_license_detected() {
        let fp = test_fingerprint();
        let key = generate_license_key(LicenseTier::Pro, &fp, 1);
        let license = validate_license_key(&key).unwrap();
        assert!(license.is_expired());
        assert_eq!(license.days_remaining(), Some(0));
    }

    #[test]
    fn tampered_key_rejected() {
        let fp = test_fingerprint();
        let key = generate_license_key(LicenseTier::Pro, &fp, 0);
        let clean: String = key.chars().filter(|c| c.is_alphanumeric()).collect();
        let mut chars: Vec<char> = clean.chars().collect();
        let last = chars.len() - 1;
        chars[last] = if chars[last] == 'A' { 'B' } else { 'A' };
        let tampered: String = chars.into_iter().collect();
        assert!(validate_license_key(&tampered).is_err());
    }

    #[test]
    fn machine_mismatch_detected() {
        let fp1 = test_fingerprint();
        let fp2 = MachineFingerprint::from_bytes(&[0xFF; 16]);
        let key = generate_license_key(LicenseTier::Pro, &fp1, 0);
        let license = validate_license_key(&key).unwrap();
        assert!(!license.is_bound_to(&fp2));
    }

    #[test]
    fn tier_layer_permissions() {
        assert!(!LicenseTier::Free.allows_layer(0));
        assert!(!LicenseTier::Free.allows_layer(1));
        assert!(LicenseTier::Free.allows_layer(2));
        assert!(LicenseTier::Pro.allows_layer(0));
        assert!(LicenseTier::Pro.allows_layer(1));
        assert!(LicenseTier::Pro.allows_layer(2));
        assert!(LicenseTier::Enterprise.allows_layer(0));
    }

    #[test]
    fn tier_profile_limits() {
        assert_eq!(LicenseTier::Free.max_profiles(), 2);
        assert_eq!(LicenseTier::Pro.max_profiles(), 50);
        assert_eq!(LicenseTier::Enterprise.max_profiles(), usize::MAX);
    }

    #[test]
    fn tier_service_access() {
        assert!(!LicenseTier::Free.allows_service());
        assert!(LicenseTier::Pro.allows_service());
        assert!(LicenseTier::Enterprise.allows_service());
    }

    #[test]
    fn base32_roundtrip() {
        let data = b"Hello, World!";
        let encoded = base32_encode(data);
        let decoded = base32_decode(&encoded).unwrap();
        assert_eq!(&decoded[..data.len()], &data[..]);
    }

    #[test]
    fn key_format_has_dashes() {
        let fp = test_fingerprint();
        let key = generate_license_key(LicenseTier::Pro, &fp, 0);
        assert!(key.contains('-'));
        for segment in key.split('-') {
            assert!(segment.len() <= 5);
        }
    }

    #[test]
    fn invalid_key_format() {
        assert!(matches!(
            validate_license_key("not-a-valid-key"),
            Err(LicenseError::InvalidFormat) | Err(LicenseError::InvalidSignature)
        ));
    }

    #[test]
    fn all_tiers_roundtrip() {
        let fp = test_fingerprint();
        for tier in [LicenseTier::Free, LicenseTier::Pro, LicenseTier::Enterprise] {
            let key = generate_license_key(tier, &fp, 0);
            let license = validate_license_key(&key).unwrap();
            assert_eq!(license.tier, tier);
        }
    }

    #[test]
    fn tier_display() {
        assert_eq!(format!("{}", LicenseTier::Free), "Free");
        assert_eq!(format!("{}", LicenseTier::Pro), "Pro");
        assert_eq!(format!("{}", LicenseTier::Enterprise), "Enterprise");
    }
}
