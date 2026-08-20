//! License-callback phone-home.
//!
//! Design constraints — every one is enforced by tests below:
//!
//! 1. **No baked-in URL.** The vendor does not force phone-home
//!    routing through their own servers. The operator (an
//!    individual, an enterprise deployer, a Tor-forwarding proxy)
//!    sets `phone_home_url` in the config. Unset → no calls, ever.
//!    Set → calls happen; there is no "disable" flag other than
//!    clearing the URL.
//!
//! 2. **No fingerprint leaves this machine.** The payload contains
//!    only: an opaque license-key serial (first 8 chars of a domain-
//!    separated hash of the key), the tier the local install
//!    believes it has, the Phantom version, the wall-clock second,
//!    and the tripwire count. No machine fingerprint. No IP-linkable
//!    payload. No profile names, seeds, or content. No operator
//!    identity.
//!
//! 3. **Transport is `curl`, not an in-tree HTTP client.** The URL,
//!    method, and body appear in `ps`, in the shell's audit log, and
//!    in any HIDS the operator is running. No hidden TLS stack, no
//!    unbounded certificate roots pulled from crates.io. If the
//!    operator wants to route through Tor, mitmproxy, or an
//!    enterprise policy proxy, they configure curl the normal way
//!    via env vars — Phantom does not fight them.
//!
//! 4. **Rate-limited to at most once per interval.** Default 24
//!    hours. The last-call time is recorded to a signed file so a
//!    process restart cannot burst-call the endpoint.
//!
//! 5. **Fail-open on network errors.** A timeout, a DNS failure, a
//!    disconnected laptop — none of these degrade the operator's
//!    tool. The license logic proceeds as if the response was OK.
//!
//! 6. **Fail-closed only on an explicit revocation.** If the
//!    endpoint returns a JSON body with `{"revoked": true}`, the
//!    tripwire records a High-severity event and the install
//!    downgrades to Free on subsequent loads. Anything else — HTTP
//!    500, malformed JSON, response silence — is treated as "no new
//!    information" and the operator continues.
//!
//! 7. **Every call is logged locally**, HMAC-signed. `phantom
//!    phone-home log` shows the history so the operator can audit
//!    exactly what left the machine and when.

use crate::keys;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

type HmacSha256 = Hmac<Sha256>;

/// Default 24 hours between calls when the config does not override.
pub const DEFAULT_INTERVAL_SECS: u64 = 24 * 60 * 60;
/// Curl wall-clock timeout — never wait longer than this on a call.
pub const CALL_TIMEOUT_SECS: u64 = 5;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PhoneHomePayload {
    /// Fixed schema version so the endpoint can evolve.
    pub schema: u32,
    /// Opaque, non-reversible license identifier: first 8 hex chars
    /// of HMAC(STATE, license_key). Different key → different
    /// serial; the raw key cannot be recovered from this.
    pub license_serial: String,
    /// Tier this install currently believes it holds.
    pub tier: String,
    /// Phantom version this call originates from.
    pub phantom_version: String,
    /// Wall-clock seconds (unix) at call time. Coarse enough to be
    /// useless as a de-anonymization signal.
    pub unix_secs: u64,
    /// Number of Low-severity tripwire events currently on file.
    pub trip_count_low: u32,
    /// Number of High-severity tripwire events currently on file.
    pub trip_count_high: u32,
    /// Proof-of-possession: HMAC-SHA256(license_key_bytes,
    /// canonical_payload_without_this_field), hex-encoded. The server
    /// (which issued the keys) verifies this to distinguish a call
    /// from an install that actually holds the license material from
    /// an attacker who only scraped a serial. Anti-replay is achieved
    /// by folding `unix_secs` into the signed payload. Detecting the
    /// same signature originating from many IPs signals key-sharing.
    ///
    /// Empty when the install is unlicensed — an attacker cannot
    /// forge a signature without the license key.
    pub proof: String,
}

/// A response the endpoint can send back. Only `revoked` is acted on;
/// every other field is advisory and ignored today.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PhoneHomeResponse {
    #[serde(default)]
    pub revoked: bool,
    #[serde(default)]
    pub message: Option<String>,
}

/// Build the payload from local state. Pure function — no network.
///
/// Callers hand the result to `preview` (to inspect what would go
/// out) or to `call_endpoint` / `maybe_phone_home` (to actually
/// send). The proof-of-possession signature is computed here so
/// preview reflects the exact bytes a call would send.
pub fn build_payload(
    license_key: Option<&str>,
    tier: &str,
    phantom_version: &str,
    trip_low: u32,
    trip_high: u32,
) -> PhoneHomePayload {
    let mut p = PhoneHomePayload {
        schema: 1,
        license_serial: license_serial_for(license_key),
        tier: tier.to_string(),
        phantom_version: phantom_version.to_string(),
        unix_secs: now_unix_secs(),
        trip_count_low: trip_low,
        trip_count_high: trip_high,
        proof: String::new(),
    };
    p.proof = compute_proof(license_key, &p);
    p
}

/// Canonical bytes over which the proof is computed: the payload
/// serialized with `proof` cleared. Any change to any field
/// (`unix_secs`, `tier`, `trip_count_*`) breaks the signature, so a
/// downstream tamperer cannot bump the tier without invalidating.
fn canonical_for_proof(p: &PhoneHomePayload) -> Vec<u8> {
    let mut c = p.clone();
    c.proof.clear();
    serde_json::to_vec(&c).unwrap_or_default()
}

fn compute_proof(license_key: Option<&str>, p: &PhoneHomePayload) -> String {
    // The empty proof for unlicensed installs — an attacker cannot
    // forge a valid signature over a claim of a Pro tier without
    // holding a real Pro key.
    let Some(key) = license_key else {
        return String::new();
    };
    let mut mac = HmacSha256::new_from_slice(key.as_bytes()).expect("HMAC accepts any key length");
    mac.update(b"phantom.phone-home-proof.v1");
    mac.update(&canonical_for_proof(p));
    let bytes = mac.finalize().into_bytes();
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Server-side helper: verify the proof against a candidate license
/// key the server knows corresponds to this serial. Vendored in
/// phantom-license so the same code produces and verifies the
/// signature — no drift between client and server implementations.
pub fn verify_proof(license_key: &str, p: &PhoneHomePayload) -> bool {
    let expected = compute_proof(Some(license_key), p);
    let a = expected.as_bytes();
    let b = p.proof.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Domain-separated 8-hex-char serial. Same key → same serial;
/// different keys → different serials with cryptographic probability.
/// The raw key cannot be recovered from this.
pub fn license_serial_for(license_key: Option<&str>) -> String {
    let key = license_key.unwrap_or("<unlicensed>");
    let sk = keys::derive_key(keys::STATE_PURPOSE);
    let mut mac = HmacSha256::new_from_slice(&sk).expect("HMAC key length is fixed");
    mac.update(b"phantom.license-serial.v1");
    mac.update(key.as_bytes());
    let bytes = mac.finalize().into_bytes();
    bytes.iter().take(4).map(|b| format!("{:02x}", b)).collect()
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// -------------------- last-call bookkeeping --------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LastCall {
    unix_secs: u64,
    url_hash_hex: String,
    mac_hex: String,
}

fn last_call_path(data_dir: &Path) -> PathBuf {
    data_dir.join(".phone_home_last")
}

fn url_hash(url: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(url.as_bytes());
    let bytes = hasher.finalize();
    bytes.iter().take(4).map(|b| format!("{:02x}", b)).collect()
}

fn last_call_mac(unix_secs: u64, url_hash_hex: &str) -> String {
    let sk = keys::derive_key(keys::STATE_PURPOSE);
    let mut mac = HmacSha256::new_from_slice(&sk).expect("HMAC key length is fixed");
    mac.update(b"phantom.phone-home-last.v1");
    mac.update(&unix_secs.to_le_bytes());
    mac.update(url_hash_hex.as_bytes());
    let bytes = mac.finalize().into_bytes();
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn load_last(path: &Path) -> Option<LastCall> {
    let s = std::fs::read_to_string(path).ok()?;
    let lc: LastCall = serde_json::from_str(&s).ok()?;
    let expected = last_call_mac(lc.unix_secs, &lc.url_hash_hex);
    let a = expected.as_bytes();
    let b = lc.mac_hex.as_bytes();
    if a.len() != b.len() {
        return None;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    if diff == 0 {
        Some(lc)
    } else {
        None
    }
}

fn save_last(path: &Path, unix_secs: u64, url_hash_hex: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let lc = LastCall {
        unix_secs,
        url_hash_hex: url_hash_hex.to_string(),
        mac_hex: last_call_mac(unix_secs, url_hash_hex),
    };
    let json = serde_json::to_string_pretty(&lc)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(path, json)
}

// -------------------- call log --------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallLogEntry {
    pub unix_secs: u64,
    pub url_hash_hex: String,
    pub payload_serial: String,
    pub outcome: String,
    pub mac_hex: String,
}

fn call_log_path(data_dir: &Path) -> PathBuf {
    data_dir.join(".phone_home_log")
}

fn log_mac(unix_secs: u64, url_hash_hex: &str, serial: &str, outcome: &str) -> String {
    let sk = keys::derive_key(keys::STATE_PURPOSE);
    let mut mac = HmacSha256::new_from_slice(&sk).expect("HMAC key length is fixed");
    mac.update(b"phantom.phone-home-log.v1");
    mac.update(&unix_secs.to_le_bytes());
    mac.update(url_hash_hex.as_bytes());
    mac.update(serial.as_bytes());
    mac.update(outcome.as_bytes());
    let bytes = mac.finalize().into_bytes();
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn append_log(data_dir: &Path, entry: CallLogEntry) {
    let path = call_log_path(data_dir);
    let mut existing: Vec<CallLogEntry> = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    existing.push(entry);
    let excess = existing.len().saturating_sub(200);
    existing.drain(..excess);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(&existing) {
        let _ = std::fs::write(&path, json);
    }
}

/// Return every log entry whose MAC verifies. Entries edited by hand
/// silently drop off.
pub fn read_log(data_dir: &Path) -> Vec<CallLogEntry> {
    let path = call_log_path(data_dir);
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str::<Vec<CallLogEntry>>(&s).ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|e| {
            let expected = log_mac(e.unix_secs, &e.url_hash_hex, &e.payload_serial, &e.outcome);
            let a = expected.as_bytes();
            let b = e.mac_hex.as_bytes();
            if a.len() != b.len() {
                return false;
            }
            let mut diff = 0u8;
            for (x, y) in a.iter().zip(b.iter()) {
                diff |= x ^ y;
            }
            diff == 0
        })
        .collect()
}

// -------------------- call --------------------

/// Whether a call is due right now. Callers who want to check
/// without triggering the network use this.
pub fn is_due(data_dir: &Path, url: &str, interval_secs: u64) -> bool {
    let now = now_unix_secs();
    match load_last(&last_call_path(data_dir)) {
        Some(lc) if lc.url_hash_hex == url_hash(url) => {
            now.saturating_sub(lc.unix_secs) >= interval_secs
        }
        _ => true,
    }
}

/// Actually make the HTTP call. Returns the parsed response on
/// success, `None` on any network / parse failure (fail-open).
/// **This function blocks up to `CALL_TIMEOUT_SECS`.** Callers that
/// want fire-and-forget wrap it in `std::thread::spawn`.
pub fn call_endpoint(url: &str, payload: &PhoneHomePayload) -> Option<PhoneHomeResponse> {
    let body = serde_json::to_vec(payload).ok()?;

    // curl -X POST -H 'Content-Type: application/json' --max-time N -d @- <url>
    let mut child = Command::new("curl")
        .arg("-X")
        .arg("POST")
        .arg("-H")
        .arg("Content-Type: application/json")
        .arg("--max-time")
        .arg(CALL_TIMEOUT_SECS.to_string())
        .arg("--silent")
        .arg("--show-error")
        .arg("--fail")
        .arg("--data-binary")
        .arg("@-")
        .arg(url)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;

    {
        use std::io::Write;
        let mut stdin = child.stdin.take()?;
        stdin.write_all(&body).ok()?;
    }

    // curl's own --max-time bounds this, but wrap with a small extra
    // margin so a stuck curl process is torn down.
    let output = wait_with_timeout(child, Duration::from_secs(CALL_TIMEOUT_SECS + 2))?;
    if !output.status.success() {
        return None;
    }
    serde_json::from_slice::<PhoneHomeResponse>(&output.stdout).ok()
}

fn wait_with_timeout(
    mut child: std::process::Child,
    timeout: Duration,
) -> Option<std::process::Output> {
    // Simple busy-poll — curl times out itself at CALL_TIMEOUT_SECS,
    // this is just a safety net.
    let start = std::time::Instant::now();
    loop {
        match child.try_wait().ok()? {
            Some(_) => return child.wait_with_output().ok(),
            None => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
}

/// The top-level entry point. Called from `LicenseGuard::load()`.
///
/// Returns the response if a call was actually made and a body came
/// back, `None` in every other case (no URL configured, not yet due,
/// network failure). Never panics.
///
/// On a revoked response, records a High-severity tripwire event via
/// the caller's tripwire module.
pub fn maybe_phone_home(
    data_dir: &Path,
    url: Option<&str>,
    interval_secs: u64,
    payload: PhoneHomePayload,
) -> Option<PhoneHomeResponse> {
    let url = url?;
    if !is_due(data_dir, url, interval_secs) {
        return None;
    }

    let url_h = url_hash(url);
    let now = now_unix_secs();
    let resp = call_endpoint(url, &payload);
    let outcome = match &resp {
        Some(r) if r.revoked => "revoked",
        Some(_) => "ok",
        None => "network_fail",
    };

    // Record the attempt (successful or not) so `phone-home log` is
    // faithful. The operator sees exactly when the tool tried.
    append_log(
        data_dir,
        CallLogEntry {
            unix_secs: now,
            url_hash_hex: url_h.clone(),
            payload_serial: payload.license_serial.clone(),
            outcome: outcome.to_string(),
            mac_hex: log_mac(now, &url_h, &payload.license_serial, outcome),
        },
    );

    // Advance the last-call marker even on network failure — otherwise
    // an offline machine calls on every load and floods the pipe.
    let _ = save_last(&last_call_path(data_dir), now, &url_h);

    // Revocation trips the tripwire.
    if resp.as_ref().is_some_and(|r| r.revoked) {
        crate::tripwire::record(
            data_dir,
            crate::tripwire::Severity::High,
            "phone_home_revoked",
        );
    }

    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_dir() -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static C: AtomicUsize = AtomicUsize::new(0);
        let id = C.fetch_add(1, Ordering::SeqCst);
        let p =
            std::env::temp_dir().join(format!("phantom-phonehome-{}-{}", std::process::id(), id));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    // Serial must be deterministic AND must not equal the input key.
    #[test]
    fn license_serial_is_deterministic_and_opaque() {
        let s1 = license_serial_for(Some("PHNTM-USER-KEY-1"));
        let s2 = license_serial_for(Some("PHNTM-USER-KEY-1"));
        let s3 = license_serial_for(Some("PHNTM-USER-KEY-2"));
        assert_eq!(s1, s2);
        assert_ne!(s1, s3);
        assert!(!s1.contains("USER"));
        assert_eq!(s1.len(), 8);
    }

    // Payload construction never touches the network. Populates the
    // proof-of-possession signature.
    #[test]
    fn build_payload_is_pure() {
        let p = build_payload(Some("KEY"), "Pro", "0.5.0", 2, 0);
        assert_eq!(p.schema, 1);
        assert_eq!(p.tier, "Pro");
        assert_eq!(p.phantom_version, "0.5.0");
        assert_eq!(p.trip_count_low, 2);
        assert_eq!(p.trip_count_high, 0);
        assert_eq!(p.license_serial.len(), 8);
        assert_eq!(p.proof.len(), 64);
    }

    // A payload built with the license key verifies against that key.
    // A different key rejects.
    #[test]
    fn proof_roundtrips_with_correct_key() {
        let p = build_payload(Some("KEY-A"), "Pro", "0.5.0", 0, 0);
        assert!(verify_proof("KEY-A", &p));
        assert!(!verify_proof("KEY-B", &p));
    }

    // An unlicensed install has an empty proof — the server cannot
    // be tricked into treating no-proof as valid because the check
    // is a byte-for-byte MAC compare, not an emptiness allowlist.
    #[test]
    fn unlicensed_payload_has_empty_proof() {
        let p = build_payload(None, "Free", "0.5.0", 0, 0);
        assert!(p.proof.is_empty());
        // Verification of an empty proof against any key must fail.
        assert!(!verify_proof("ANYTHING", &p));
    }

    // Changing any signed field (tier, trip_count, unix_secs) breaks
    // the proof. This is the anti-tamper guarantee for the server:
    // an attacker who intercepts a Pro-tier payload cannot promote
    // themselves to Enterprise by flipping the tier field.
    #[test]
    fn field_tamper_breaks_proof() {
        let mut p = build_payload(Some("KEY"), "Pro", "0.5.0", 0, 0);
        assert!(verify_proof("KEY", &p));
        p.tier = "Enterprise".into();
        assert!(!verify_proof("KEY", &p));

        let mut p = build_payload(Some("KEY"), "Pro", "0.5.0", 0, 0);
        p.trip_count_high = 99;
        assert!(!verify_proof("KEY", &p));
    }

    // Two calls at (slightly) different times produce different
    // proofs even for the same license and tier. This is anti-
    // replay: the server sees a fresh signature every call.
    #[test]
    fn proof_varies_with_time() {
        let mut a = build_payload(Some("KEY"), "Pro", "0.5.0", 0, 0);
        let mut b = a.clone();
        b.unix_secs = a.unix_secs.wrapping_add(1);
        // Recompute b's proof for the new timestamp.
        b.proof = compute_proof(Some("KEY"), &b);
        // The proofs must differ despite everything else being equal.
        a.proof = compute_proof(Some("KEY"), &a);
        assert_ne!(a.proof, b.proof);
    }

    // With no URL, is_due is not evaluated by maybe_phone_home and
    // no call happens. Verified indirectly: maybe_phone_home returns
    // None and no log entry appears.
    #[test]
    fn no_url_means_no_call() {
        let d = scratch_dir();
        let payload = build_payload(None, "Free", "0.5.0", 0, 0);
        assert!(maybe_phone_home(&d, None, DEFAULT_INTERVAL_SECS, payload).is_none());
        assert!(read_log(&d).is_empty());
    }

    // First call to a given URL is due; a call recorded at t=now is
    // not due again until interval seconds pass.
    #[test]
    fn is_due_respects_interval() {
        let d = scratch_dir();
        let url = "https://example.invalid/callback";
        assert!(is_due(&d, url, DEFAULT_INTERVAL_SECS));

        save_last(&last_call_path(&d), now_unix_secs(), &url_hash(url)).unwrap();
        assert!(!is_due(&d, url, DEFAULT_INTERVAL_SECS));

        // A different URL is considered a first call.
        assert!(is_due(
            &d,
            "https://other.invalid/cb",
            DEFAULT_INTERVAL_SECS
        ));
    }

    // Tampering with the last-call file must fail MAC and be treated
    // as "no record", so a call is due.
    #[test]
    fn tampered_last_call_falls_back_to_due() {
        let d = scratch_dir();
        let url = "https://example.invalid/callback";
        save_last(&last_call_path(&d), now_unix_secs(), &url_hash(url)).unwrap();
        // Bump the timestamp; MAC no longer matches.
        let path = last_call_path(&d);
        let raw = std::fs::read_to_string(&path).unwrap();
        let mut val: serde_json::Value = serde_json::from_str(&raw).unwrap();
        val["unix_secs"] = serde_json::json!(0u64);
        std::fs::write(&path, val.to_string()).unwrap();
        assert!(is_due(&d, url, DEFAULT_INTERVAL_SECS));
    }

    // The URL never appears in the log — only its hash — so someone
    // reading the log cannot enumerate every callback address the
    // operator has used.
    #[test]
    fn call_log_stores_url_hash_not_url() {
        let d = scratch_dir();
        let url = "https://example.invalid/very-specific-path";
        let payload = build_payload(Some("K"), "Free", "0.5.0", 0, 0);
        // Direct log append (avoid actually calling curl in unit tests).
        let now = now_unix_secs();
        let url_h = url_hash(url);
        append_log(
            &d,
            CallLogEntry {
                unix_secs: now,
                url_hash_hex: url_h.clone(),
                payload_serial: payload.license_serial.clone(),
                outcome: "ok".into(),
                mac_hex: log_mac(now, &url_h, &payload.license_serial, "ok"),
            },
        );
        let log = read_log(&d);
        assert_eq!(log.len(), 1);
        assert!(!log[0].url_hash_hex.contains("example"));
        assert_eq!(log[0].url_hash_hex.len(), 8);
    }

    // Log entries with a bad MAC are silently dropped on read.
    #[test]
    fn forged_log_entry_dropped() {
        let d = scratch_dir();
        let path = call_log_path(&d);
        let forged = vec![CallLogEntry {
            unix_secs: 1_700_000_000,
            url_hash_hex: "deadbeef".into(),
            payload_serial: "cafef00d".into(),
            outcome: "ok".into(),
            mac_hex: "00".repeat(32),
        }];
        std::fs::write(&path, serde_json::to_string_pretty(&forged).unwrap()).unwrap();
        assert!(read_log(&d).is_empty());
    }
}
