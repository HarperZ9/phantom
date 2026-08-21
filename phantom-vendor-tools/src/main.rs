//! Vendor-side operations CLI for Phantom licensing.
//!
//! # Distribution
//!
//! **Vendor-internal only.** Ships against the same master seed as
//! phantom-cli / phantom-verify: whoever holds this binary can sign
//! license keys and needs to be treated with the same care as the
//! master seed itself. Do NOT ship in public release archives.
//!
//! # What it does
//!
//! `phantom-vendor issue`
//!   Sign a new license key for a customer's machine fingerprint.
//!   Called by the licensing team after payment clears. Prints the
//!   key to hand off and the opaque serial to record in D1.
//!
//! `phantom-vendor serial-of --key <license>`
//!   Compute the opaque phone-home serial for a candidate key.
//!   Useful when a phone-home log entry needs to be matched back to
//!   a customer.
//!
//! `phantom-vendor verify-callback <path.json>`
//!   Verify a phone-home payload against a candidate license key.
//!   Reports whether the proof-of-possession is authentic, whether
//!   the payload's tier claim matches the key, and whether the
//!   claimed timestamp is plausible (± 5 min).
//!
//! `phantom-vendor decode --key <license>`
//!   Decode a license key into its constituent parts (tier,
//!   expiration, issued day, machine hash) for support debugging.
//!
//! # Design notes
//!
//! All operations are local. This binary does NOT reach out to the
//! phone-home endpoint or to D1 — those are separate concerns. To
//! actually store or revoke a license, invoke `wrangler d1 execute`
//! against the endpoints project separately.

use clap::{Parser, Subcommand};
use phantom_license::key::{generate_license_key, LicenseTier};
use phantom_license::phone_home::license_serial_for;
use phantom_license::{validate_license_key, MachineFingerprint};

#[derive(Parser)]
#[command(
    name = "phantom-vendor",
    about = "Vendor-side license issuance + phone-home verification (INTERNAL)",
    version
)]
struct Cli {
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Issue a signed license key bound to a customer's machine
    /// fingerprint. This is what the licensing team runs against an
    /// enrollment request from a paying customer.
    Issue {
        /// Tier: free | pro | enterprise.
        #[arg(long)]
        tier: String,
        /// 32-hex machine fingerprint the customer sent via
        /// `phantom license request`.
        #[arg(long)]
        fingerprint: String,
        /// Days from today until the license expires. 0 = perpetual
        /// (recommended only for Enterprise seats with an out-of-
        /// band renewal agreement).
        #[arg(long, default_value = "365")]
        expires_days: u32,
    },

    /// Compute the opaque phone-home serial for a candidate key.
    SerialOf {
        #[arg(long)]
        key: String,
    },

    /// Decode a license key into its fields.
    Decode {
        #[arg(long)]
        key: String,
    },

    /// Verify a phone-home JSON payload against a candidate license
    /// key. Reports whether the proof-of-possession is authentic
    /// and whether the claimed tier / freshness are plausible.
    VerifyCallback {
        /// Path to a JSON file (or - for stdin) containing a
        /// PhoneHomePayload.
        path: String,
        /// The license key the endpoint believes this serial maps
        /// to (looked up in D1 by the caller).
        #[arg(long)]
        key: String,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Command::Issue {
            tier,
            fingerprint,
            expires_days,
        } => cmd_issue(&tier, &fingerprint, expires_days, cli.json),
        Command::SerialOf { key } => cmd_serial_of(&key, cli.json),
        Command::Decode { key } => cmd_decode(&key, cli.json),
        Command::VerifyCallback { path, key } => cmd_verify_callback(&path, &key, cli.json),
    }
}

fn parse_tier(s: &str) -> Result<LicenseTier, String> {
    match s.to_ascii_lowercase().as_str() {
        "free" => Ok(LicenseTier::Free),
        "pro" => Ok(LicenseTier::Pro),
        "enterprise" | "ent" => Ok(LicenseTier::Enterprise),
        other => Err(format!(
            "unknown tier '{}' (want free|pro|enterprise)",
            other
        )),
    }
}

fn parse_fingerprint(hex: &str) -> Result<[u8; 16], String> {
    let clean: String = hex.chars().filter(|c| !c.is_whitespace()).collect();
    if clean.len() != 32 {
        return Err(format!(
            "fingerprint must be 32 hex chars, got {}",
            clean.len()
        ));
    }
    let mut out = [0u8; 16];
    for i in 0..16 {
        out[i] = u8::from_str_radix(&clean[2 * i..2 * i + 2], 16)
            .map_err(|_| format!("non-hex character at position {}", 2 * i))?;
    }
    Ok(out)
}

fn today_epoch_days() -> u32 {
    (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
        / 86400) as u32
}

fn cmd_issue(tier_str: &str, fp_hex: &str, expires_days: u32, json: bool) {
    let tier = match parse_tier(tier_str) {
        Ok(t) => t,
        Err(e) => {
            emit_error(json, "issue", &e);
            std::process::exit(1);
        }
    };
    let fp_bytes = match parse_fingerprint(fp_hex) {
        Ok(b) => b,
        Err(e) => {
            emit_error(json, "issue", &e);
            std::process::exit(1);
        }
    };
    let fp = MachineFingerprint::from_bytes(&fp_bytes);
    let expires_epoch = if expires_days == 0 {
        0
    } else {
        today_epoch_days() + expires_days
    };
    let key = generate_license_key(tier, &fp, expires_epoch);
    let serial = license_serial_for(Some(&key));

    if json {
        let payload = serde_json::json!({
            "ok": true,
            "command": "issue",
            "data": {
                "license_key": key,
                "serial": serial,
                "tier": tier.to_string(),
                "fingerprint_hex": fp_hex.to_lowercase(),
                "expires_epoch_days": expires_epoch,
                "expires_days": expires_days,
            }
        });
        println!("{}", serde_json::to_string_pretty(&payload).unwrap());
    } else {
        println!("License issued.");
        println!();
        println!("  key         : {}", key);
        println!("  serial      : {}", serial);
        println!("  tier        : {}", tier);
        println!("  fingerprint : {}", fp_hex.to_lowercase());
        if expires_epoch == 0 {
            println!("  expires     : perpetual");
        } else {
            println!(
                "  expires     : epoch-day {} ({} days from now)",
                expires_epoch, expires_days
            );
        }
        println!();
        println!("Record `serial` in D1 so phone-home lookups succeed.");
        println!("Deliver `key` to the customer via a secure channel.");
    }
}

fn cmd_serial_of(key: &str, json: bool) {
    let serial = license_serial_for(Some(key));
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "ok": true,
                "command": "serial-of",
                "data": {"serial": serial}
            }))
            .unwrap()
        );
    } else {
        println!("{}", serial);
    }
}

fn cmd_decode(key: &str, json: bool) {
    match validate_license_key(key) {
        Ok(license) => {
            let fingerprint_hex: String = license
                .machine_hash
                .iter()
                .map(|b| format!("{:02x}", b))
                .collect();
            let expires = if license.expires_epoch_days == 0 {
                "perpetual".to_string()
            } else {
                format!("epoch-day {}", license.expires_epoch_days)
            };
            let days_remaining = license.days_remaining();

            if json {
                let payload = serde_json::json!({
                    "ok": true,
                    "command": "decode",
                    "data": {
                        "tier": license.tier.to_string(),
                        "expires_epoch_days": license.expires_epoch_days,
                        "issued_epoch_days": license.issued_epoch_days,
                        "machine_fingerprint_hex": fingerprint_hex,
                        "days_remaining": days_remaining,
                        "is_expired": license.is_expired(),
                    }
                });
                println!("{}", serde_json::to_string_pretty(&payload).unwrap());
            } else {
                println!("tier              : {}", license.tier);
                println!("expires           : {}", expires);
                println!("issued (epoch-day): {}", license.issued_epoch_days);
                println!("machine hash      : {}", fingerprint_hex);
                match days_remaining {
                    Some(d) => println!("days remaining    : {}", d),
                    None => println!("days remaining    : perpetual"),
                }
                println!(
                    "status            : {}",
                    if license.is_expired() {
                        "EXPIRED"
                    } else {
                        "valid"
                    }
                );
            }
        }
        Err(e) => {
            emit_error(json, "decode", &format!("cannot decode: {}", e));
            std::process::exit(1);
        }
    }
}

fn cmd_verify_callback(path: &str, key: &str, json: bool) {
    let bytes = if path == "-" {
        use std::io::Read;
        let mut s = String::new();
        if let Err(e) = std::io::stdin().read_to_string(&mut s) {
            emit_error(json, "verify-callback", &format!("stdin: {}", e));
            std::process::exit(1);
        }
        s.into_bytes()
    } else {
        match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => {
                emit_error(json, "verify-callback", &format!("read {}: {}", path, e));
                std::process::exit(1);
            }
        }
    };

    let payload: phantom_license::phone_home::PhoneHomePayload =
        match serde_json::from_slice(&bytes) {
            Ok(p) => p,
            Err(e) => {
                emit_error(json, "verify-callback", &format!("parse: {}", e));
                std::process::exit(1);
            }
        };

    let proof_ok = phantom_license::phone_home::verify_proof(key, &payload);

    // Freshness check: the payload's unix_secs should be within a
    // few minutes of now. Stale timestamps flag replay attempts.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let age = now as i64 - payload.unix_secs as i64;
    let fresh = age.abs() < 60 * 15; // 15 minutes

    // Tier-vs-key consistency: does the payload's claimed tier
    // match what the key actually encodes?
    let tier_claim_matches = match validate_license_key(key) {
        Ok(license) => license.tier.to_string() == payload.tier,
        Err(_) => false,
    };

    let expected_serial = license_serial_for(Some(key));
    let serial_matches = expected_serial == payload.license_serial;

    let verdict = if !serial_matches {
        "SERIAL_MISMATCH"
    } else if !proof_ok {
        "PROOF_INVALID"
    } else if !tier_claim_matches {
        "TIER_MISMATCH"
    } else if !fresh {
        "STALE"
    } else {
        "OK"
    };

    if json {
        let payload_json = serde_json::json!({
            "ok": verdict == "OK",
            "command": "verify-callback",
            "data": {
                "verdict": verdict,
                "proof_ok": proof_ok,
                "serial_matches": serial_matches,
                "tier_claim_matches": tier_claim_matches,
                "fresh": fresh,
                "payload_age_seconds": age,
                "payload_tier": payload.tier,
                "payload_serial": payload.license_serial,
                "payload_trip_count_low": payload.trip_count_low,
                "payload_trip_count_high": payload.trip_count_high,
            }
        });
        println!("{}", serde_json::to_string_pretty(&payload_json).unwrap());
    } else {
        println!("verdict          : {}", verdict);
        println!("proof valid      : {}", proof_ok);
        println!("serial matches   : {}", serial_matches);
        println!("tier claim ok    : {}", tier_claim_matches);
        println!("fresh (±15 min)  : {}", fresh);
        println!("payload age (s)  : {}", age);
        println!("payload tier     : {}", payload.tier);
        println!(
            "trip counts      : low={} high={}",
            payload.trip_count_low, payload.trip_count_high
        );
    }
    if verdict != "OK" {
        std::process::exit(2);
    }
}

fn emit_error(json: bool, cmd: &'static str, msg: &str) {
    if json {
        let payload = serde_json::json!({
            "ok": false,
            "command": cmd,
            "error": msg,
        });
        println!("{}", serde_json::to_string_pretty(&payload).unwrap());
    } else {
        eprintln!("error: {}", msg);
    }
}
