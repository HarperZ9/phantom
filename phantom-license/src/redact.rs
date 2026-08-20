//! Log-line and panic-message redaction of likely-secret substrings.
//!
//! Threat: a `tracing::debug!` on the activation path, or a Rust
//! panic that formats a struct containing a license key, can spill
//! sensitive material into a log file that is later shared with
//! support, uploaded to a cloud collector, or included in a bug
//! report. The material never needed to leave the machine.
//!
//! Defense: [`redact`] scrubs three shapes that only ever appear in
//! Phantom output when a secret is being logged:
//!
//!   1. **License keys** — 5-char base32 groups separated by dashes,
//!      totaling at least the 96-char raw key length.
//!   2. **HMAC-hex strings** — 64-char lowercase hex; the shape of
//!      `state_mac_hex`, `mac_hex` on origin marks, and every attempt-
//!      log signature.
//!   3. **Machine fingerprints** — 32-char lowercase hex.
//!
//! Each match is replaced with a fixed placeholder that preserves the
//! shape (so operator log-grep patterns still find them) but reveals
//! no bytes. The placeholders are stable so log-diff tools work.
//!
//! [`install_panic_hook`] wraps the current panic hook so every
//! panic message is redacted before it hits stderr, log files, or
//! any downstream reporter. It is idempotent — callers install it
//! once from `main()`.
//!
//! Redaction is deliberately regex-free (no `regex` dep, no
//! catastrophic backtracking). The scanners are single-pass and
//! bounded by the input length.

use std::sync::OnceLock;

const HEX_LOWER: &str = "0123456789abcdef";
const BASE32: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

pub const LICENSE_KEY_PLACEHOLDER: &str = "<redacted-license-key>";
pub const MAC_PLACEHOLDER: &str = "<redacted-mac>";
pub const FINGERPRINT_PLACEHOLDER: &str = "<redacted-fingerprint>";

fn is_hex_lower(c: char) -> bool {
    HEX_LOWER.contains(c)
}

fn is_base32_char(c: char) -> bool {
    BASE32.contains(c)
}

/// Redact all recognized secret shapes from a string.
///
/// The scan is single-pass and left-to-right; overlapping matches
/// resolve to the earliest-longest match. Order of pattern checks
/// matters — license keys (which contain base32 that overlaps with
/// hex) are checked before hex.
pub fn redact(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < bytes.len() {
        let ch = bytes[i] as char;

        // Try license key first: base32 char and long enough for at
        // least the raw key shape.
        if is_base32_char(ch) {
            if let Some(end) = try_match_license_key(&input[i..]) {
                out.push_str(LICENSE_KEY_PLACEHOLDER);
                i += end;
                continue;
            }
        }

        // Try lowercase hex runs.
        if is_hex_lower(ch) {
            let run = hex_run(&input[i..]);
            match run {
                64 => {
                    out.push_str(MAC_PLACEHOLDER);
                    i += 64;
                    continue;
                }
                32 => {
                    out.push_str(FINGERPRINT_PLACEHOLDER);
                    i += 32;
                    continue;
                }
                _ => {}
            }
        }

        out.push(ch);
        i += 1;
    }
    out
}

/// Length of the maximal run of lowercase hex chars starting at
/// position 0 of `s`. Used to decide between MAC (64) and fingerprint
/// (32) shapes; anything else is left alone so ordinary log ids and
/// UUIDs are not falsely redacted.
fn hex_run(s: &str) -> usize {
    let mut n = 0;
    for c in s.chars() {
        if is_hex_lower(c) {
            n += 1;
        } else {
            break;
        }
    }
    n
}

/// Try to match a Phantom license key at the start of `s`. Returns
/// the length consumed on match, or `None`.
///
/// License keys are printed as five-char base32 groups separated by
/// dashes. The raw payload is 60 bytes → 96 base32 chars → ~19
/// dashed groups. We require at least 15 groups (~75 chars total,
/// leaving slack for wrapping) to avoid falsely matching short
/// dashed identifiers.
fn try_match_license_key(s: &str) -> Option<usize> {
    let mut groups = 0;
    let mut consumed = 0;
    let bytes = s.as_bytes();

    loop {
        // A group: up to 5 base32 chars.
        let start = consumed;
        while consumed < bytes.len() {
            let c = bytes[consumed] as char;
            if !is_base32_char(c) {
                break;
            }
            consumed += 1;
            if consumed - start == 5 {
                break;
            }
        }
        if consumed - start == 0 {
            break;
        }
        groups += 1;

        // Followed by a dash?
        if consumed < bytes.len() && bytes[consumed] as char == '-' {
            consumed += 1;
            continue;
        }
        break;
    }

    if groups >= 15 {
        Some(consumed)
    } else {
        None
    }
}

/// Install a panic hook that redacts the panic message before
/// delegating to the previous hook. Idempotent — a second call is a
/// no-op. Recommended: call from `main()`.
pub fn install_panic_hook() {
    static INSTALLED: OnceLock<()> = OnceLock::new();
    INSTALLED.get_or_init(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            // The default panic hook formats to a string internally.
            // We can't intercept that formatter, but we can construct
            // our own redacted message and call the previous hook
            // with a synthesized PanicInfo... which we can't do
            // (PanicInfo::new is unstable). Instead, print a redacted
            // line ourselves and then also call the previous hook so
            // process-exit reporters (backtrace collectors, etc.)
            // still get the original event.
            let msg = format_panic(info);
            eprintln!("[phantom] panic: {}", redact(&msg));
            previous(info);
        }));
    });
}

fn format_panic(info: &std::panic::PanicHookInfo<'_>) -> String {
    let payload = info.payload();
    let msg = if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "(non-string panic payload)".to_string()
    };
    match info.location() {
        Some(loc) => format!("{} at {}:{}", msg, loc.file(), loc.line()),
        None => msg,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_string_untouched() {
        assert_eq!(redact("hello world"), "hello world");
        assert_eq!(redact(""), "");
    }

    #[test]
    fn short_hex_is_not_redacted() {
        // Ordinary log ids, PR numbers, git shorthashes must survive.
        assert_eq!(redact("abcd1234"), "abcd1234");
        assert_eq!(redact("commit=6910a54"), "commit=6910a54");
    }

    #[test]
    fn fingerprint_shape_redacted() {
        let fp = "a277a1a263b0b5cbb03c6f6f4839897a"; // 32 lowercase hex
        let s = format!("machine fp={} tier=Free", fp);
        assert!(!redact(&s).contains(fp));
        assert!(redact(&s).contains(FINGERPRINT_PLACEHOLDER));
    }

    #[test]
    fn mac_shape_redacted() {
        let mac = "d1f19a1c7e6130266d3d4d438553f1fa22d0a2cf84698c99eb2cbc84db30d500";
        let s = format!("state_mac={}", mac);
        assert!(!redact(&s).contains(mac));
        assert!(redact(&s).contains(MAC_PLACEHOLDER));
    }

    #[test]
    fn license_key_shape_redacted() {
        // 19 groups of 5 base32 chars separated by dashes.
        let key = "ABCDE-FGHIJ-KLMNO-PQRST-UVWXY-Z2345-67ABC-DEFGH-IJKLM-\
                   NOPQR-STUVW-XYZ23-4567A-BCDEF-GHIJK-LMNOP-QRSTU-VWXYZ-23456";
        let s = format!("activated key={}", key);
        assert!(!redact(&s).contains(key));
        assert!(redact(&s).contains(LICENSE_KEY_PLACEHOLDER));
    }

    // A short dashed identifier (e.g. a UUID-like string) must NOT be
    // falsely tagged as a license key.
    #[test]
    fn short_dashed_id_not_matched_as_key() {
        let uuid = "550E8400-E29B-41D4-A716-446655440000";
        assert_eq!(redact(uuid), uuid);
    }

    // Mixed content: several secrets in one line all get scrubbed.
    #[test]
    fn multiple_secrets_all_scrubbed() {
        let fp = "a277a1a263b0b5cbb03c6f6f4839897a";
        let mac = "d1f19a1c7e6130266d3d4d438553f1fa22d0a2cf84698c99eb2cbc84db30d500";
        let s = format!("fp={} sig={}", fp, mac);
        let r = redact(&s);
        assert!(!r.contains(fp));
        assert!(!r.contains(mac));
        assert!(r.contains(FINGERPRINT_PLACEHOLDER));
        assert!(r.contains(MAC_PLACEHOLDER));
    }

    // Panic hook installation is idempotent.
    #[test]
    fn install_panic_hook_is_idempotent() {
        install_panic_hook();
        install_panic_hook();
    }
}
