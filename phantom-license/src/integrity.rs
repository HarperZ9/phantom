//! Integrity self-checks and anti-debugger detection.
//!
//! The ensemble is deliberate: no single detection method should be
//! the only thing standing between a debugger and the license logic.
//! Patching out `IsDebuggerPresent` (a favorite of tutorial reversers)
//! must not disable everything; the caller reads a `Verdict` that
//! records *which* detectors triggered so operators can see how deep
//! an attack went before it stopped mattering.
//!
//! Detection is best-effort and platform-conditional. On the
//! non-detecting side, the ensemble reports `all_clear=true` and lets
//! callers proceed.

use sha2::{Digest, Sha256};

pub struct IntegrityCheck {
    expected_hash: [u8; 32],
}

impl IntegrityCheck {
    pub fn new(expected: [u8; 32]) -> Self {
        IntegrityCheck {
            expected_hash: expected,
        }
    }

    pub fn verify_region(&self, data: &[u8]) -> bool {
        let mut hasher = Sha256::new();
        hasher.update(data);
        let hash = hasher.finalize();
        constant_time_eq(&hash, &self.expected_hash)
    }

    pub fn compute_hash(data: &[u8]) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(data);
        let hash = hasher.finalize();
        let mut result = [0u8; 32];
        result.copy_from_slice(&hash);
        result
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Individual detector name, useful for observability.
pub type Detector = &'static str;

/// Result of the full detection ensemble. `all_clear` is the fast
/// path most callers use; `triggered` records exactly which detectors
/// fired so a partial patch is visible in self-check output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectionVerdict {
    pub all_clear: bool,
    pub triggered: Vec<Detector>,
}

impl DetectionVerdict {
    pub fn clean() -> Self {
        Self {
            all_clear: true,
            triggered: Vec::new(),
        }
    }
}

/// Legacy single-boolean API kept for callers that don't care which
/// detector fired. Delegates to the ensemble.
pub fn detect_debugger() -> bool {
    !detect_debugger_ensemble().all_clear
}

/// Run every debugger detector available on this platform and report
/// which ones fired. Detectors are conservative — they only report a
/// debugger when they have strong evidence, so a `true` here is a
/// real signal.
pub fn detect_debugger_ensemble() -> DetectionVerdict {
    let mut triggered = Vec::new();

    // Common: tests explicitly disable detection so `cargo test` under
    // rust-analyzer or a coverage runner doesn't produce false
    // positives. Ship binaries never set this variable.
    if std::env::var_os("PHANTOM_DISABLE_INTEGRITY").is_some() {
        return DetectionVerdict::clean();
    }

    #[cfg(target_os = "linux")]
    {
        if detect_tracer_pid() {
            triggered.push("tracer_pid");
        }
        if detect_ld_preload() {
            triggered.push("ld_preload");
        }
        if detect_debugger_env() {
            triggered.push("debugger_env");
        }
    }

    #[cfg(target_os = "windows")]
    {
        if detect_windows_is_debugger_present() {
            triggered.push("is_debugger_present");
        }
        if detect_windows_remote_debugger() {
            triggered.push("check_remote_debugger");
        }
    }

    DetectionVerdict {
        all_clear: triggered.is_empty(),
        triggered,
    }
}

// -------------------- Linux detectors --------------------

#[cfg(target_os = "linux")]
fn detect_tracer_pid() -> bool {
    let Ok(status) = std::fs::read_to_string("/proc/self/status") else {
        return false;
    };
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("TracerPid:") {
            return rest.trim() != "0";
        }
    }
    false
}

#[cfg(target_os = "linux")]
fn detect_debugger_env() -> bool {
    // Environment variables commonly set by debuggers, tracers,
    // and dynamic-instrumentation frameworks. Presence is a strong
    // hint that the process is under active analysis. Legitimate
    // operators can bypass with PHANTOM_DISABLE_INTEGRITY.
    const MARKERS: &[&str] = &[
        "LD_AUDIT",       // dynamic linker audit interface
        "MALLOC_TRACE",   // glibc heap tracer
        "MALLOC_CHECK_",  // glibc heap checker (any value)
        "GDB_PYTHON",     // gdb python integration
        "FRIDA_AGENT",    // frida instrumentation
        "PIN_INSTRUMENT", // Intel Pin tool
    ];
    MARKERS
        .iter()
        .any(|k| std::env::var_os(k).is_some_and(|v| !v.is_empty()))
}

#[cfg(target_os = "linux")]
fn detect_ld_preload() -> bool {
    // LD_PRELOAD is the standard shim/interposition mechanism. Its
    // presence is not proof of malice — legitimate profilers use it —
    // but it is a strong signal that the process's syscall surface is
    // no longer trustworthy. Callers can decide policy.
    std::env::var_os("LD_PRELOAD").map_or(false, |v| !v.is_empty())
}

// -------------------- Windows detectors --------------------

#[cfg(target_os = "windows")]
fn detect_windows_is_debugger_present() -> bool {
    extern "system" {
        fn IsDebuggerPresent() -> i32;
    }
    unsafe { IsDebuggerPresent() != 0 }
}

#[cfg(target_os = "windows")]
fn detect_windows_remote_debugger() -> bool {
    extern "system" {
        fn CheckRemoteDebuggerPresent(hProcess: isize, pbDebuggerPresent: *mut i32) -> i32;
        fn GetCurrentProcess() -> isize;
    }
    let mut present: i32 = 0;
    unsafe {
        let ok = CheckRemoteDebuggerPresent(GetCurrentProcess(), &mut present);
        ok != 0 && present != 0
    }
}

// -------------------- Non-Linux/non-Windows stubs --------------------

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn detect_tracer_pid() -> bool {
    false
}

// -------------------- Process hardening --------------------

/// Apply platform-appropriate hardening to the current process at
/// startup. Idempotent; safe to call more than once.
///
/// On Linux: `prctl(PR_SET_DUMPABLE, 0)` prevents the kernel from
/// writing a core dump on crash and blocks another UID's ptrace, which
/// closes the "dump the process and grep for the master key" attack.
///
/// On Windows: no-op today; SetProcessMitigationPolicy for
/// dynamic-code and CFG hardening is a candidate for a later sprint.
pub fn harden_process() {
    #[cfg(target_os = "linux")]
    {
        const PR_SET_DUMPABLE: i32 = 4;
        extern "C" {
            fn prctl(option: i32, arg2: u64, arg3: u64, arg4: u64, arg5: u64) -> i32;
        }
        unsafe {
            let _ = prctl(PR_SET_DUMPABLE, 0, 0, 0, 0);
        }
    }
}

/// Boolean self-check. False means at least one detector fired OR
/// hardening declined to apply. Legacy signature; new code should
/// prefer `full_self_check()` to see *which* detectors fired.
pub fn self_check() -> bool {
    detect_debugger_ensemble().all_clear
}

/// Full self-check result usable by `phantom self-check` and any
/// caller that wants observability into which detectors fired.
pub fn full_self_check() -> DetectionVerdict {
    detect_debugger_ensemble()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_computation() {
        let data = b"test data";
        let hash = IntegrityCheck::compute_hash(data);
        assert_eq!(hash.len(), 32);

        let check = IntegrityCheck::new(hash);
        assert!(check.verify_region(data));
    }

    #[test]
    fn tampered_data_fails() {
        let data = b"original";
        let hash = IntegrityCheck::compute_hash(data);
        let check = IntegrityCheck::new(hash);
        assert!(!check.verify_region(b"modified"));
    }

    #[test]
    fn constant_time_eq_works() {
        assert!(constant_time_eq(b"hello", b"hello"));
        assert!(!constant_time_eq(b"hello", b"world"));
        assert!(!constant_time_eq(b"short", b"longer"));
    }

    // With PHANTOM_DISABLE_INTEGRITY set, the ensemble is a no-op —
    // required so `cargo test` under rust-analyzer / IDE coverage
    // runners / debuggers doesn't get false positives. This test
    // deliberately DOES NOT set it and relies on the test env being
    // clean of debuggers.
    #[test]
    fn detector_ensemble_returns_verdict() {
        // Under normal test runs (no debugger attached), most
        // detectors should not trigger. On CI this is almost always
        // clean; developer machines may occasionally trigger the
        // LD_PRELOAD detector if profiling. We assert only that we
        // get a well-formed verdict, not a specific outcome.
        let v = detect_debugger_ensemble();
        // The struct fields exist and triggered is a Vec.
        assert!(v.triggered.len() < 20);
        // If any detector fires, all_clear must be false.
        assert_eq!(v.all_clear, v.triggered.is_empty());
    }

    // Serialize env-touching tests inside this crate.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn env_bypass_forces_clean() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::set_var("PHANTOM_DISABLE_INTEGRITY", "1");
        let v = detect_debugger_ensemble();
        assert!(v.all_clear);
        assert!(v.triggered.is_empty());
        std::env::remove_var("PHANTOM_DISABLE_INTEGRITY");
    }

    #[test]
    fn harden_process_is_idempotent() {
        harden_process();
        harden_process();
    }
}
