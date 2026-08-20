//! Investigator-side verifier for Phantom origin marks.
//!
//! # Scope
//!
//! This binary is a **vendor-internal tool**. It carries the same
//! obfuscated master key material as `phantom-cli` — anyone who has
//! the binary can, in principle, extract the master key and forge
//! valid origin marks. Distribution should be limited to the vendor
//! and to specific auditors under NDA. Do NOT ship this binary as
//! part of the public release archive.
//!
//! A follow-up will migrate origin marks from HMAC-SHA256 to
//! Ed25519 signatures; at that point `phantom-verify` can be
//! distributed publicly with only the public key.
//!
//! # What it does
//!
//! Four commands, all operating on evidence produced by an operator
//! (voluntarily) or seized under warrant. **The tool never phones
//! home, never touches network, never opens any file except the ones
//! named on the command line.**
//!
//! - `inspect <profile.json>` — dump the profile's origin_mark
//!   fields in a human-readable form. No crypto, no verdict.
//! - `verify <profile.json>` — verify the origin_mark's HMAC. Prints
//!   `VALID` / `INVALID` / `UNMARKED` / `TAMPERED`. Does NOT
//!   identify the license.
//! - `match <profile.json> --key <license-key>` — given a suspect
//!   profile AND a candidate license key, report whether that key
//!   produced this profile. Returns `MATCH` / `NO_MATCH` /
//!   `INVALID_KEY` / `INVALID_MARK` / `UNMARKED`.
//! - `serial --key <license-key>` — compute the opaque phone-home
//!   serial that a given license would present. Useful for
//!   correlating a call-log entry back to a customer record.

use clap::{Parser, Subcommand};
use phantom_cli::profile::schema::HardwareProfile;
use phantom_license::watermark;
use serde::Serialize;

#[derive(Parser)]
#[command(
    name = "phantom-verify",
    about = "Investigator-side verifier for Phantom origin marks (VENDOR-INTERNAL — see README)",
    version
)]
struct Cli {
    /// Emit machine-readable JSON on stdout instead of the human
    /// summary. Same stable envelope as phantom --json.
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print the origin_mark fields from a profile without any
    /// crypto verdict.
    Inspect {
        /// Path to a profile JSON. Use `-` for stdin.
        path: String,
    },

    /// Verify the origin_mark's HMAC. Reports whether the mark is
    /// authentic and internally consistent with the profile bytes.
    /// Does NOT identify which license produced it.
    Verify {
        /// Path to a profile JSON. Use `-` for stdin.
        path: String,
    },

    /// Report whether a specific license key produced a specific
    /// profile. This is the primary investigation flow.
    Match {
        /// Path to a profile JSON. Use `-` for stdin.
        path: String,
        /// Candidate license key to check against.
        #[arg(long)]
        key: String,
    },

    /// Compute the opaque phone-home serial for a candidate license
    /// key. Correlates against phantom-svc phone-home call logs
    /// which store only the serial, not the raw key.
    Serial {
        /// Candidate license key.
        #[arg(long)]
        key: String,
    },
}

#[derive(Serialize)]
struct Envelope<T: Serialize> {
    ok: bool,
    command: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl<T: Serialize> Envelope<T> {
    fn ok(command: &'static str, data: T) -> Self {
        Self {
            ok: true,
            command,
            data: Some(data),
            error: None,
        }
    }
    fn err(command: &'static str, msg: impl Into<String>) -> Envelope<()> {
        Envelope {
            ok: false,
            command,
            data: None,
            error: Some(msg.into()),
        }
    }
    fn print(&self) {
        match serde_json::to_string_pretty(self) {
            Ok(s) => println!("{}", s),
            Err(_) => println!(r#"{{"ok":false,"error":"serialization failed"}}"#),
        }
    }
}

fn main() {
    // Harden the verifier itself the same way phantom-cli hardens:
    // core dumps off, ptrace lockdown, panic redaction. If this tool
    // is ever exfiltrated it should be no more useful to the
    // attacker than phantom-cli.
    phantom_license::redact::install_panic_hook();
    phantom_license::integrity::harden_process();

    let cli = Cli::parse();
    match cli.command {
        Command::Inspect { path } => cmd_inspect(&path, cli.json),
        Command::Verify { path } => cmd_verify(&path, cli.json),
        Command::Match { path, key } => cmd_match(&path, &key, cli.json),
        Command::Serial { key } => cmd_serial(&key, cli.json),
    }
}

fn read_profile(path: &str) -> Result<HardwareProfile, String> {
    let json = if path == "-" {
        use std::io::Read;
        let mut s = String::new();
        std::io::stdin()
            .read_to_string(&mut s)
            .map_err(|e| format!("stdin: {}", e))?;
        s
    } else {
        std::fs::read_to_string(path).map_err(|e| format!("read {}: {}", path, e))?
    };
    serde_json::from_str::<HardwareProfile>(&json).map_err(|e| format!("parse: {}", e))
}

#[derive(Serialize)]
struct InspectPayload {
    marked: bool,
    profile_hash_hex: Option<String>,
    origin_fingerprint_hex: Option<String>,
    origin_tier: Option<String>,
    issued_epoch_days: Option<u32>,
    mac_hex: Option<String>,
}

fn cmd_inspect(path: &str, json: bool) {
    let prof = match read_profile(path) {
        Ok(p) => p,
        Err(e) => {
            report_error(json, "inspect", &e);
            std::process::exit(1);
        }
    };

    let payload = match &prof.metadata.origin_mark {
        Some(m) => InspectPayload {
            marked: true,
            profile_hash_hex: Some(m.profile_hash_hex.clone()),
            origin_fingerprint_hex: Some(m.origin_fingerprint_hex.clone()),
            origin_tier: Some(m.origin_tier.clone()),
            issued_epoch_days: Some(m.issued_epoch_days),
            mac_hex: Some(m.mac_hex.clone()),
        },
        None => InspectPayload {
            marked: false,
            profile_hash_hex: None,
            origin_fingerprint_hex: None,
            origin_tier: None,
            issued_epoch_days: None,
            mac_hex: None,
        },
    };

    if json {
        Envelope::ok("inspect", payload).print();
    } else {
        if !payload.marked {
            println!("Profile has no origin_mark (unmarked / hand-authored / legacy).");
            return;
        }
        println!("origin_mark:");
        println!(
            "  origin_fingerprint : {}",
            payload.origin_fingerprint_hex.unwrap()
        );
        println!("  origin_tier        : {}", payload.origin_tier.unwrap());
        println!(
            "  issued_epoch_days  : {}",
            payload.issued_epoch_days.unwrap()
        );
        println!(
            "  profile_hash       : {}",
            payload.profile_hash_hex.unwrap()
        );
        println!("  mac_hex            : {}", payload.mac_hex.unwrap());
    }
}

#[derive(Serialize)]
struct VerifyPayload {
    verdict: &'static str,
    detail: String,
}

fn cmd_verify(path: &str, json: bool) {
    let prof = match read_profile(path) {
        Ok(p) => p,
        Err(e) => {
            report_error(json, "verify", &e);
            std::process::exit(1);
        }
    };

    let Some(mark) = prof.metadata.origin_mark.clone() else {
        let p = VerifyPayload {
            verdict: "UNMARKED",
            detail: "Profile has no origin_mark. Every profile Phantom generates \
                     since Sprint 14 carries one — an unmarked profile is either \
                     hand-authored, pre-Sprint-14, or has had its mark stripped."
                .into(),
        };
        if json {
            Envelope::ok("verify", p).print();
        } else {
            println!("UNMARKED: {}", p.detail);
        }
        return;
    };

    // Compute canonical bytes: profile with origin_mark cleared.
    let mut stripped = prof.clone();
    stripped.metadata.origin_mark = None;
    let canonical = serde_json::to_vec(&stripped).unwrap_or_default();

    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    hasher.update(&canonical);
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&hasher.finalize());

    // Reconstruct the fingerprint from the mark for the local-vs-
    // foreign discrimination in `watermark::verify` — we set the
    // "this_fingerprint" argument to the mark's own origin so the
    // verdict is Local when authentic (we don't have the
    // investigator's fingerprint here).
    let origin_fp: [u8; 16] = match hex_16(&mark.origin_fingerprint_hex) {
        Some(b) => b,
        None => {
            let p = VerifyPayload {
                verdict: "TAMPERED",
                detail: "origin_fingerprint_hex has an unparseable value; \
                         the mark was edited by hand"
                    .into(),
            };
            if json {
                Envelope::ok("verify", p).print();
            } else {
                println!("TAMPERED: {}", p.detail);
            }
            std::process::exit(2);
        }
    };

    match watermark::verify(&mark, &hash, &origin_fp) {
        watermark::Verdict::Local => {
            let p = VerifyPayload {
                verdict: "VALID",
                detail: format!(
                    "Origin mark's HMAC verifies. Profile was produced at tier {} \
                     on the machine whose fingerprint hash is {}.",
                    mark.origin_tier, mark.origin_fingerprint_hex
                ),
            };
            if json {
                Envelope::ok("verify", p).print();
            } else {
                println!("VALID: {}", p.detail);
            }
        }
        watermark::Verdict::Foreign { .. } => {
            // Shouldn't happen because we passed origin_fp as the
            // caller fingerprint, but treat defensively as VALID with
            // a caveat.
            let p = VerifyPayload {
                verdict: "VALID",
                detail: "HMAC verifies (foreign-fingerprint pathway).".into(),
            };
            if json {
                Envelope::ok("verify", p).print();
            } else {
                println!("VALID: {}", p.detail);
            }
        }
        watermark::Verdict::Invalid => {
            let p = VerifyPayload {
                verdict: "INVALID",
                detail: "HMAC does not verify. The mark was forged, corrupted, or \
                         produced by a different master-key generation."
                    .into(),
            };
            if json {
                Envelope::ok("verify", p).print();
            } else {
                println!("INVALID: {}", p.detail);
            }
            std::process::exit(2);
        }
        watermark::Verdict::ContentTampered => {
            let p = VerifyPayload {
                verdict: "TAMPERED",
                detail: "The profile's canonical hash does not match the hash the \
                         origin_mark was signed against. Someone edited the profile \
                         after it was signed."
                    .into(),
            };
            if json {
                Envelope::ok("verify", p).print();
            } else {
                println!("TAMPERED: {}", p.detail);
            }
            std::process::exit(2);
        }
        watermark::Verdict::Malformed => {
            let p = VerifyPayload {
                verdict: "MALFORMED",
                detail: "The mark has structural corruption (bad hex, wrong lengths).".into(),
            };
            if json {
                Envelope::ok("verify", p).print();
            } else {
                println!("MALFORMED: {}", p.detail);
            }
            std::process::exit(2);
        }
    }
}

#[derive(Serialize)]
struct MatchPayload {
    verdict: &'static str,
    detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    origin_tier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    issued_epoch_days: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    license_tier: Option<String>,
}

fn cmd_match(path: &str, key: &str, json: bool) {
    let prof = match read_profile(path) {
        Ok(p) => p,
        Err(e) => {
            report_error(json, "match", &e);
            std::process::exit(1);
        }
    };
    let Some(mark) = prof.metadata.origin_mark.clone() else {
        let p = MatchPayload {
            verdict: "UNMARKED",
            detail: "Profile has no origin_mark; nothing to match against.".into(),
            origin_tier: None,
            issued_epoch_days: None,
            license_tier: None,
        };
        if json {
            Envelope::ok("match", p).print();
        } else {
            println!("UNMARKED: {}", p.detail);
        }
        return;
    };

    // Decode the candidate license so we can pull its machine hash.
    let license = match phantom_license::validate_license_key(key) {
        Ok(l) => l,
        Err(e) => {
            let p = MatchPayload {
                verdict: "INVALID_KEY",
                detail: format!("Candidate license key rejected: {}", e),
                origin_tier: None,
                issued_epoch_days: None,
                license_tier: None,
            };
            if json {
                Envelope::ok("match", p).print();
            } else {
                println!("INVALID_KEY: {}", p.detail);
            }
            std::process::exit(3);
        }
    };

    // Verify the mark itself first — a forged mark cannot match any
    // real key.
    let mut stripped = prof.clone();
    stripped.metadata.origin_mark = None;
    let canonical = serde_json::to_vec(&stripped).unwrap_or_default();
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    hasher.update(&canonical);
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&hasher.finalize());

    let origin_fp = hex_16(&mark.origin_fingerprint_hex);
    let mark_ok = origin_fp.is_some_and(|fp| {
        matches!(
            watermark::verify(&mark, &hash, &fp),
            watermark::Verdict::Local | watermark::Verdict::Foreign { .. }
        )
    });
    if !mark_ok {
        let p = MatchPayload {
            verdict: "INVALID_MARK",
            detail: "The profile's origin_mark does not verify (forged or tampered).".into(),
            origin_tier: Some(mark.origin_tier),
            issued_epoch_days: Some(mark.issued_epoch_days),
            license_tier: Some(license.tier.to_string()),
        };
        if json {
            Envelope::ok("match", p).print();
        } else {
            println!("INVALID_MARK: {}", p.detail);
        }
        std::process::exit(3);
    }

    // The definitive comparison: does the profile's origin fingerprint
    // equal the machine hash embedded in the candidate license key?
    // The license key ITSELF is signed by the master and cannot be
    // forged, so this pairing is the tie-back.
    let origin_fp = origin_fp.unwrap();
    if license.machine_hash == origin_fp {
        let p = MatchPayload {
            verdict: "MATCH",
            detail: format!(
                "This profile was generated on the machine bound to this license. \
                 License tier: {}. Profile-time tier: {}. Profile issued day: {}.",
                license.tier, mark.origin_tier, mark.issued_epoch_days
            ),
            origin_tier: Some(mark.origin_tier),
            issued_epoch_days: Some(mark.issued_epoch_days),
            license_tier: Some(license.tier.to_string()),
        };
        if json {
            Envelope::ok("match", p).print();
        } else {
            println!("MATCH: {}", p.detail);
        }
    } else {
        let p = MatchPayload {
            verdict: "NO_MATCH",
            detail: "The profile's origin_mark verifies as authentic but its \
                     origin_fingerprint does not match this license's bound machine. \
                     The profile was produced on a different machine."
                .into(),
            origin_tier: Some(mark.origin_tier),
            issued_epoch_days: Some(mark.issued_epoch_days),
            license_tier: Some(license.tier.to_string()),
        };
        if json {
            Envelope::ok("match", p).print();
        } else {
            println!("NO_MATCH: {}", p.detail);
        }
        std::process::exit(3);
    }
}

#[derive(Serialize)]
struct SerialPayload {
    serial: String,
}

fn cmd_serial(key: &str, json: bool) {
    let serial = phantom_license::phone_home::license_serial_for(Some(key));
    if json {
        Envelope::ok("serial", SerialPayload { serial }).print();
    } else {
        println!("{}", serial);
    }
}

fn report_error(json: bool, cmd: &'static str, msg: &str) {
    if json {
        let e: Envelope<()> = Envelope::<()>::err(cmd, msg);
        e.print();
    } else {
        eprintln!("error: {}", msg);
    }
}

fn hex_16(s: &str) -> Option<[u8; 16]> {
    if s.len() != 32 {
        return None;
    }
    let mut out = [0u8; 16];
    for i in 0..16 {
        out[i] = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).ok()?;
    }
    Some(out)
}
