//! Integration tests for phantom-verify.
//!
//! Each test lays out a scratch data dir, drives phantom-cli to
//! generate a real signed profile, then invokes phantom-verify by
//! shelling out and asserting on the exit code and stdout. This
//! exercises the full binary boundary (arg parsing, exit codes,
//! JSON envelope) rather than the underlying library only.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn unique_scratch() -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let id = N.fetch_add(1, Ordering::SeqCst);
    let p = std::env::temp_dir().join(format!("phantom-verify-it-{}-{}", std::process::id(), id));
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn generate_marked_profile(scratch: &PathBuf, name: &str, seed: &str) -> PathBuf {
    let status = Command::new("cargo")
        .arg("run")
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(workspace_root().join("Cargo.toml"))
        .arg("-p")
        .arg("phantom-cli")
        .arg("--")
        .arg("profile")
        .arg("generate")
        .arg(name)
        .arg("--seed")
        .arg(seed)
        .env("PHANTOM_DATA_DIR", scratch)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("phantom-cli exec");
    assert!(status.success());
    scratch.join("profiles").join(format!("{}.json", name))
}

fn run_verify(args: &[&str]) -> std::process::Output {
    Command::new("cargo")
        .arg("run")
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(workspace_root().join("Cargo.toml"))
        .arg("-p")
        .arg("phantom-verify")
        .arg("--")
        .args(args)
        .output()
        .expect("phantom-verify exec")
}

#[test]
fn inspect_marked_profile_reports_fields() {
    let d = unique_scratch();
    let path = generate_marked_profile(&d, "alpha", "seed-alpha");
    let out = run_verify(&["inspect", path.to_str().unwrap()]);
    assert!(out.status.success());
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("origin_fingerprint"));
    assert!(s.contains("origin_tier"));
    assert!(s.contains("mac_hex"));
}

#[test]
fn verify_marked_profile_prints_valid() {
    let d = unique_scratch();
    let path = generate_marked_profile(&d, "beta", "seed-beta");
    let out = run_verify(&["verify", path.to_str().unwrap()]);
    assert!(out.status.success());
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("VALID"));
}

#[test]
fn verify_content_tampered_profile_reports_tampered() {
    let d = unique_scratch();
    let path = generate_marked_profile(&d, "gamma", "seed-gamma");
    // Flip a byte inside a VALUE (not a key) so the profile still
    // parses as HardwareProfile but its canonical hash changes.
    // We use the seed string we passed in — swapping one char.
    let text = std::fs::read_to_string(&path).unwrap();
    let tampered = text.replacen("seed-gamma", "seed-Gamma", 1);
    assert_ne!(text, tampered, "expected substitution to change the file");
    std::fs::write(&path, tampered).unwrap();
    let out = run_verify(&["verify", path.to_str().unwrap()]);
    assert!(!out.status.success());
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        combined.contains("TAMPERED") || combined.contains("MALFORMED"),
        "expected TAMPERED/MALFORMED verdict, got: {}",
        combined
    );
}

#[test]
fn verify_json_envelope_is_valid_json() {
    let d = unique_scratch();
    let path = generate_marked_profile(&d, "delta", "seed-delta");
    let out = run_verify(&["--json", "verify", path.to_str().unwrap()]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid JSON");
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "verify");
    assert_eq!(v["data"]["verdict"], "VALID");
}

#[test]
fn inspect_reads_from_stdin() {
    let d = unique_scratch();
    let path = generate_marked_profile(&d, "epsilon", "seed-epsilon");
    let bytes = std::fs::read(&path).unwrap();

    let mut child = Command::new("cargo")
        .arg("run")
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(workspace_root().join("Cargo.toml"))
        .arg("-p")
        .arg("phantom-verify")
        .arg("--")
        .arg("--json")
        .arg("inspect")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.as_mut().unwrap().write_all(&bytes).unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid JSON");
    assert_eq!(v["ok"], true);
    assert_eq!(v["data"]["marked"], true);
}

#[test]
fn serial_command_is_deterministic() {
    let a = run_verify(&["--json", "serial", "--key", "PHNTM-USER-KEY-42"]);
    let b = run_verify(&["--json", "serial", "--key", "PHNTM-USER-KEY-42"]);
    assert!(a.status.success() && b.status.success());
    let va: serde_json::Value = serde_json::from_slice(&a.stdout).unwrap();
    let vb: serde_json::Value = serde_json::from_slice(&b.stdout).unwrap();
    assert_eq!(va["data"]["serial"], vb["data"]["serial"]);
    // Serial length is 8 hex chars per phone_home::license_serial_for.
    let s = va["data"]["serial"].as_str().unwrap();
    assert_eq!(s.len(), 8);
    // And a different key produces a different serial.
    let c = run_verify(&["--json", "serial", "--key", "PHNTM-USER-KEY-43"]);
    let vc: serde_json::Value = serde_json::from_slice(&c.stdout).unwrap();
    assert_ne!(va["data"]["serial"], vc["data"]["serial"]);
}
