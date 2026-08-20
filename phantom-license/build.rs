//! Compile-time obfuscation of the master signing key material.
//!
//! Seed sourcing (Sprint 24 — precedence, highest first):
//!
//! 1. `PHANTOM_MASTER_SEED` env var — 64-char hex, exactly 32 bytes
//!    after decode. This is how production and vendor release builds
//!    inject the real key: CI stores it as a repo/org secret and
//!    populates the env var for the build step.
//!
//! 2. `.master_seed` file at the workspace root — same 64-char hex
//!    format. Convenient for local dev when an engineer needs to run
//!    against a real seed (e.g. testing license issuance flow
//!    against a real endpoint). Git-ignored.
//!
//! 3. Compiled-in DEV placeholder — used ONLY for `cargo build`
//!    (debug). `cargo build --release` without a seed above **fails
//!    the build with a clear error**. The placeholder is deliberately
//!    public and worthless; every issued license under it is
//!    forgeable by anyone who reads this file.
//!
//! The plaintext seed is XOR-scrambled into `OBFUSCATED_MASTER` and
//! only the scrambled bytes ship. The seed itself never appears in
//! the compiled binary regardless of source.
//!
//! Bumping the seed invalidates every license issued against the old
//! one. `MASTER_KEY_GEN` (baked in below) is the value operators see
//! via `phantom self-check`; bumping the seed also bumps the
//! generation so it is externally observable.

const DEV_PLACEHOLDER_SEED: [u8; 32] = [
    0x8f, 0x2e, 0xa7, 0x13, 0xb6, 0x5c, 0xd1, 0x9a, 0x74, 0x08, 0x3b, 0xef, 0x51, 0xc4, 0x2d, 0x7f,
    0x96, 0x1a, 0x83, 0x5e, 0xd0, 0x67, 0x2b, 0x4c, 0x91, 0xac, 0x38, 0xe7, 0xf5, 0x0d, 0x62, 0x89,
];

fn derive_xor_byte(i: usize) -> u8 {
    // Deliberately non-obvious mixing function. Must match the runtime
    // reader in `src/keys.rs` exactly, byte-for-byte.
    const A: u32 = 0xA5F3_7B24;
    const B: u32 = 0x9E3E_1C71;
    const C: u32 = 0x4B27_D9A6;

    let i32_ = i as u32;
    let rot = A.rotate_left(i32_ & 31);
    let sum = rot.wrapping_add(B.wrapping_mul(i32_.wrapping_add(17)));
    let mix = sum ^ C.rotate_right((i32_ * 3) & 31);
    ((mix >> ((i32_ & 3) * 8)) & 0xFF) as u8
}

/// Decode 64 hex chars into 32 bytes. Rejects any input of the wrong
/// shape with a build error that names the source.
fn hex64_to_bytes(hex: &str, source_desc: &str) -> [u8; 32] {
    let clean: String = hex
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>();
    if clean.len() != 64 {
        panic!(
            "{} is not 64 hex chars (got {}). Format: openssl rand -hex 32",
            source_desc,
            clean.len()
        );
    }
    let mut out = [0u8; 32];
    for i in 0..32 {
        let byte = u8::from_str_radix(&clean[2 * i..2 * i + 2], 16).unwrap_or_else(|_| {
            panic!(
                "{} contains non-hex character at position {}",
                source_desc,
                2 * i
            )
        });
        out[i] = byte;
    }
    out
}

/// Resolve the seed by precedence. Returns `(seed_bytes, source_label,
/// is_placeholder, master_gen)`.
fn resolve_seed() -> ([u8; 32], &'static str, bool, u8) {
    // 1. Env var.
    if let Ok(hex) = std::env::var("PHANTOM_MASTER_SEED") {
        if !hex.trim().is_empty() {
            let bytes = hex64_to_bytes(hex.trim(), "PHANTOM_MASTER_SEED");
            return (bytes, "env:PHANTOM_MASTER_SEED", false, 2);
        }
    }

    // 2. Workspace-root file.
    // build.rs runs from the crate dir; the workspace root is `..`.
    let file_path = std::path::Path::new("..").join(".master_seed");
    if let Ok(contents) = std::fs::read_to_string(&file_path) {
        if !contents.trim().is_empty() {
            let bytes = hex64_to_bytes(contents.trim(), "workspace-root .master_seed");
            // Instruct cargo to rebuild if the file changes.
            println!("cargo:rerun-if-changed=../.master_seed");
            return (bytes, "file:../.master_seed", false, 2);
        }
    }
    println!("cargo:rerun-if-changed=../.master_seed");

    // 3. Placeholder — only allowed for debug builds.
    let profile = std::env::var("PROFILE").unwrap_or_default();
    if profile == "release" {
        panic!(
            "\n\n\
             ========================================================\n\
             phantom-license: release build refused\n\
             ========================================================\n\
             \n\
             No master seed available.\n\
             \n\
             Set PHANTOM_MASTER_SEED (64 hex chars, `openssl rand -hex 32`)\n\
             for the build step, or place the same content in\n\
             `<workspace-root>/.master_seed`.\n\
             \n\
             The DEV placeholder seed is compiled into debug builds\n\
             only. Every license signed with it is forgeable — do not\n\
             distribute a release binary built with it.\n\
             ========================================================\n\n"
        );
    }
    (DEV_PLACEHOLDER_SEED, "compiled DEV placeholder", true, 1)
}

fn main() {
    let (master_seed, source, is_placeholder, master_gen) = resolve_seed();

    let mut obfuscated = [0u8; 32];
    for (i, b) in master_seed.iter().enumerate() {
        obfuscated[i] = b ^ derive_xor_byte(i);
    }

    let mut src = String::with_capacity(1024);
    src.push_str("// AUTO-GENERATED by build.rs — do not edit.\n");
    src.push_str(&format!("// Seed source: {}\n", source));
    src.push_str("// Byte-for-byte XOR-obfuscation of the master signing key.\n");
    src.push_str("pub(crate) const OBFUSCATED_MASTER: [u8; 32] = [\n    ");
    for (i, b) in obfuscated.iter().enumerate() {
        src.push_str(&format!("0x{:02x}, ", b));
        if (i + 1) % 8 == 0 {
            src.push_str("\n    ");
        }
    }
    src.push_str("\n];\n");
    // Generation byte: bumped from 1 → 2 when a real seed is
    // baked (env or file). Placeholder builds keep generation 1
    // so operators running dev binaries can see they're on the
    // dev seed via `phantom self-check`.
    src.push_str(&format!(
        "pub(crate) const MASTER_KEY_GEN: u8 = {};\n",
        master_gen
    ));

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR must be set by cargo");
    let dest = std::path::Path::new(&out_dir).join("master_key_obf.rs");
    std::fs::write(&dest, src).expect("write master_key_obf.rs");

    // Loud console warning on a debug build using the placeholder
    // so a developer running `cargo run` sees the reminder every
    // time. Uses cargo's warning channel so it surfaces in
    // `cargo build` output (colored yellow).
    if is_placeholder {
        println!(
            "cargo:warning=phantom-license: DEV placeholder seed in use — release builds require PHANTOM_MASTER_SEED"
        );
    }

    println!("cargo:rerun-if-env-changed=PHANTOM_MASTER_SEED");
    println!("cargo:rerun-if-changed=build.rs");
}
