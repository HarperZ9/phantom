//! Tamper tripwire: an append-only, HMAC-signed record of every
//! integrity event severe enough that a legitimate operator should
//! never trigger it.
//!
//! Design constraints (deliberately narrow — this is anti-tamper, not
//! anti-user):
//!
//! - The tripwire lives at `<data_dir>/.tripwire`. Nothing outside
//!   Phantom's own state directory is touched. Ever.
//! - Every entry carries an HMAC under the STATE_PURPOSE subkey so
//!   an attacker cannot silently trim, edit, or plant events.
//! - **High-severity** events (state MAC failure, honey-key attempts,
//!   confirmed patched-binary integrity checks) cause
//!   `LicenseGuard::load()` to silently return `Free` tier on every
//!   subsequent invocation — a cracked install becomes a functionally
//!   Free install without ever printing "you have been detected". No
//!   error message means no signal for the reverser to grep for.
//! - **Low-severity** events (LD_PRELOAD present, tracer_pid > 0)
//!   are recorded for operator visibility via `phantom tamper-report`
//!   but do not trip the guard on their own — legitimate profilers
//!   trigger these, so acting on them would misfire.
//! - Successful license activation calls `clear()` — the operator
//!   proved they hold a real key, so historical noise is discarded.
//!
//! What this deliberately does NOT do:
//!
//! - Never writes outside `<data_dir>`. No touching the reverser's
//!   home directory, browser data, other applications, system files.
//! - Never spawns code, executes commands, or reaches out to the
//!   network. Phantom is not malware and will not become malware to
//!   punish an attacker.
//! - Never corrupts unrelated files. If a Free-tier bump is the wrong
//!   call for a given user, the worst outcome is that they call
//!   support and re-activate.
//! - Never fabricates evidence against a user. Every logged event has
//!   a reason string an operator can inspect via `tamper-report`.

use crate::keys;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

type HmacSha256 = Hmac<Sha256>;

/// Severity of a tripwire event.
///
/// `High` events cause `LicenseGuard::load()` to silently downgrade
/// the tool to Free tier on every subsequent call — the cracked
/// install becomes a functionally-Free install with no error message.
/// `Low` events are logged only.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Severity {
    Low,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TripwireEvent {
    pub unix_secs: u64,
    pub severity: Severity,
    pub reason: String,
    pub mac_hex: String,
}

fn compute_mac(unix_secs: u64, severity: Severity, reason: &str) -> String {
    let sk = keys::derive_key(keys::STATE_PURPOSE);
    let mut mac = HmacSha256::new_from_slice(&sk).expect("HMAC key length is fixed");
    mac.update(b"phantom.tripwire.v1");
    mac.update(&unix_secs.to_le_bytes());
    mac.update(&[match severity {
        Severity::Low => 0,
        Severity::High => 1,
    }]);
    mac.update(reason.as_bytes());
    let bytes = mac.finalize().into_bytes();
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn verify(e: &TripwireEvent) -> bool {
    let expected = compute_mac(e.unix_secs, e.severity, &e.reason);
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
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn tripwire_path(data_dir: &Path) -> PathBuf {
    data_dir.join(".tripwire")
}

fn load(path: &Path) -> Vec<TripwireEvent> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<Vec<TripwireEvent>>(&s).ok())
        .unwrap_or_default()
        .into_iter()
        .filter(verify)
        .collect()
}

fn save(path: &Path, events: &[TripwireEvent]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(events)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(path, json)
}

/// Record a tripwire event to the log at `data_dir`. Idempotent-ish
/// per (severity, reason) within a short window — repeated triggers
/// (e.g. a debugger attached across multiple runs) do not flood the
/// log; only the first hit of a given reason in the last hour is
/// kept.
pub fn record(data_dir: &Path, severity: Severity, reason: &str) {
    let path = tripwire_path(data_dir);
    let mut events = load(&path);
    let now = now_unix_secs();

    // Dedup: if the same reason fired in the last hour, skip.
    if events.iter().any(|e| {
        e.reason == reason && e.severity == severity && now.saturating_sub(e.unix_secs) < 60 * 60
    }) {
        return;
    }

    events.push(TripwireEvent {
        unix_secs: now,
        severity,
        reason: reason.to_string(),
        mac_hex: compute_mac(now, severity, reason),
    });
    // Bounded log size — keep the most recent 200 entries.
    let excess = events.len().saturating_sub(200);
    events.drain(..excess);
    let _ = save(&path, &events);
}

/// Return all verified tripwire events in the log. Callers use this
/// to render the `tamper-report` command.
pub fn read_events(data_dir: &Path) -> Vec<TripwireEvent> {
    load(&tripwire_path(data_dir))
}

/// True if any High-severity event is present. The guard consults
/// this and silently returns Free tier when it's set.
pub fn is_tripped(data_dir: &Path) -> bool {
    load(&tripwire_path(data_dir))
        .iter()
        .any(|e| e.severity == Severity::High)
}

/// Called on successful license activation to clear the history —
/// the operator proved they hold a real key.
pub fn clear(data_dir: &Path) {
    let _ = std::fs::remove_file(tripwire_path(data_dir));
}

// -------------------- Honey license keys --------------------
//
// Well-formed-looking keys that are guaranteed never to be issued.
// An attacker who dumps the binary might find these strings and try
// them against `phantom license activate`, expecting one to work.
// Every attempt records a High-severity tripwire event on this
// machine, silently locking it into Free tier from that point on.
//
// The strings ARE visible to `strings` on the binary — that is the
// entire point. They are bait.

const HONEY_KEYS: &[&str] = &[
    "PHNTM-DEV-INTERNAL-USE-ONLY-DO-NOT-DISTRIBUTE-XXXXXXXXXXXXXXXXXXXX",
    "PHANTOM-MASTER-UNLOCK-ENTERPRISE-TIER-PERPETUAL-XXXXXXXXXXXXXXXXXX",
    "OPS-ADMIN-OVERRIDE-KEY-2026-Q4-ROLLING-XXXXXXXXXXXXXXXXXXXXXXXXXXX",
    "PHNTM-QA-TEST-BYPASS-KEY-FOR-CI-BUILDS-ONLY-XXXXXXXXXXXXXXXXXXXXXX",
    "PHANTOM-EMERGENCY-RECOVERY-DO-NOT-USE-XXXXXXXXXXXXXXXXXXXXXXXXXXXX",
    "PHNTM-BACKDOOR-DEV-2026-INTERNAL-ONLY-XXXXXXXXXXXXXXXXXXXXXXXXXXXX",
    "MASTER-KEY-ADMIN-OVERRIDE-SUPPORT-XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX",
    "PHANTOM-STAFF-UNLIMITED-TIER-KEY-XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX",
];

/// Check whether `attempted_key` matches any honey key. Whitespace-
/// and case-normalized like the real license key path. Comparison is
/// constant-time per candidate so timing does not reveal a partial
/// prefix match.
pub fn is_honey_key(attempted_key: &str) -> bool {
    let clean: String = attempted_key
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-')
        .collect::<String>()
        .to_ascii_uppercase();

    let mut hit = false;
    for k in HONEY_KEYS {
        let hk: String = k
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '-')
            .collect::<String>()
            .to_ascii_uppercase();
        let a = clean.as_bytes();
        let b = hk.as_bytes();
        if a.len() == b.len() {
            let mut diff = 0u8;
            for (x, y) in a.iter().zip(b.iter()) {
                diff |= x ^ y;
            }
            if diff == 0 {
                hit = true;
            }
        }
    }
    hit
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_dir() -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static C: AtomicUsize = AtomicUsize::new(0);
        let id = C.fetch_add(1, Ordering::SeqCst);
        let p =
            std::env::temp_dir().join(format!("phantom-tripwire-{}-{}", std::process::id(), id));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn no_events_means_not_tripped() {
        let d = scratch_dir();
        assert!(!is_tripped(&d));
        assert!(read_events(&d).is_empty());
    }

    #[test]
    fn low_severity_does_not_trip() {
        let d = scratch_dir();
        record(&d, Severity::Low, "ld_preload");
        assert!(!is_tripped(&d));
        assert_eq!(read_events(&d).len(), 1);
    }

    #[test]
    fn high_severity_trips() {
        let d = scratch_dir();
        record(&d, Severity::High, "state_mac_failed");
        assert!(is_tripped(&d));
    }

    #[test]
    fn clear_removes_history() {
        let d = scratch_dir();
        record(&d, Severity::High, "honey_key_attempt");
        assert!(is_tripped(&d));
        clear(&d);
        assert!(!is_tripped(&d));
    }

    // Editing the file to invert severity from Low to High (or vice
    // versa) must fail the MAC and be dropped on load.
    #[test]
    fn forged_severity_flip_rejected() {
        let d = scratch_dir();
        record(&d, Severity::Low, "ld_preload");
        // Rewrite the file with severity flipped to High but the same
        // (now stale) MAC.
        let path = tripwire_path(&d);
        let mut events: Vec<TripwireEvent> =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        events[0].severity = Severity::High;
        std::fs::write(&path, serde_json::to_string_pretty(&events).unwrap()).unwrap();
        assert!(!is_tripped(&d));
    }

    // Dedup: two identical Low events within the hour collapse to one.
    #[test]
    fn duplicate_reason_within_window_is_deduped() {
        let d = scratch_dir();
        record(&d, Severity::Low, "ld_preload");
        record(&d, Severity::Low, "ld_preload");
        record(&d, Severity::Low, "ld_preload");
        assert_eq!(read_events(&d).len(), 1);
    }

    #[test]
    fn honey_key_normalization_matches() {
        // Match works with lower case + trailing whitespace + wrapping.
        assert!(is_honey_key(
            "phntm-dev-internal-use-only-do-not-distribute-xxxxxxxxxxxxxxxxxxxx"
        ));
        assert!(is_honey_key(
            "  PHNTM-DEV-INTERNAL-USE-ONLY-DO-NOT-DISTRIBUTE-XXXXXXXXXXXXXXXXXXXX  \n"
        ));
    }

    #[test]
    fn arbitrary_string_is_not_a_honey_key() {
        assert!(!is_honey_key("hello world"));
        assert!(!is_honey_key("PHNTM-USER-12345-VALID-KEY-XXXX"));
    }
}
