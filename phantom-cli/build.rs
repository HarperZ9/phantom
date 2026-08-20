//! Emit build metadata as compile-time environment variables so the
//! CLI can report them in `phantom --version` and JSON payloads.
//!
//! Everything is best-effort — an unknown git state produces "unknown"
//! rather than failing the build.

use std::process::Command;

fn main() {
    let commit = Command::new("git")
        .args(["rev-parse", "--short=12", "HEAD"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout).ok()
            } else {
                None
            }
        })
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let dirty = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout).ok()
            } else {
                None
            }
        })
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);

    let commit_str = if dirty {
        format!("{}-dirty", commit)
    } else {
        commit
    };

    println!("cargo:rustc-env=PHANTOM_GIT_COMMIT={}", commit_str);
    println!(
        "cargo:rustc-env=PHANTOM_BUILD_TARGET={}",
        std::env::var("TARGET").unwrap_or_else(|_| "unknown".into())
    );
    println!(
        "cargo:rustc-env=PHANTOM_BUILD_PROFILE={}",
        std::env::var("PROFILE").unwrap_or_else(|_| "unknown".into())
    );

    // Rerun if the git HEAD moves or the index changes.
    println!("cargo:rerun-if-changed=../.git/HEAD");
    println!("cargo:rerun-if-changed=../.git/index");
    // And re-run on every rebuild — cheap.
    println!("cargo:rerun-if-changed=build.rs");
}
