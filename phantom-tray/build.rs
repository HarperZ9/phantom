//! Link the Win32 libraries phantom-tray's `extern "system"` blocks
//! call into. Without these directives the MSVC linker fails with
//! ~40 LNK2019 "unresolved external symbol" errors (CreateBitmap,
//! BeginPaint, TextOutA, ...). On non-Windows targets this build
//! script is a no-op — cargo just doesn't consume the directives.

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rustc-link-lib=gdi32");
        println!("cargo:rustc-link-lib=user32");
        println!("cargo:rustc-link-lib=shell32");
        println!("cargo:rustc-link-lib=advapi32");
    }
    println!("cargo:rerun-if-changed=build.rs");
}
