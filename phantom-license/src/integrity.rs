use sha2::{Sha256, Digest};

pub struct IntegrityCheck {
    expected_hash: [u8; 32],
}

impl IntegrityCheck {
    pub fn new(expected: [u8; 32]) -> Self {
        IntegrityCheck { expected_hash: expected }
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

#[cfg(windows)]
pub fn detect_debugger() -> bool {
    extern "system" {
        fn IsDebuggerPresent() -> i32;
    }
    unsafe { IsDebuggerPresent() != 0 }
}

#[cfg(not(windows))]
pub fn detect_debugger() -> bool {
    if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
        for line in status.lines() {
            if line.starts_with("TracerPid:") {
                let pid = line.trim_start_matches("TracerPid:").trim();
                return pid != "0";
            }
        }
    }
    false
}

pub fn self_check() -> bool {
    if detect_debugger() {
        return false;
    }
    true
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

    #[test]
    fn self_check_passes_in_test() {
        assert!(self_check());
    }
}
