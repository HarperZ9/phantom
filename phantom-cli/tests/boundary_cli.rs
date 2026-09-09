use phantom_boundary::{
    content_id, BoundaryAction, BoundaryException, BoundaryLimits, BoundaryPolicy, BoundaryRoute,
    RedirectPolicyConfig,
};
use phantom_cli::config::PhantomConfig;
use std::io::Write;
use std::net::TcpListener;
use std::process::Command;
use std::sync::mpsc;
use std::time::{Duration, Instant};

const KEY: &str = "operator-owned-boundary-key-32-bytes";
const SESSION: &str = "session-a";
const OPERATION: &str = "chat.completions.create";

fn phantom_bin() -> &'static str {
    env!("CARGO_BIN_EXE_phantom-cli")
}

fn write_request(dir: &tempfile::TempDir, name: &str, body: &[u8]) -> std::path::PathBuf {
    let path = dir.path().join(name);
    std::fs::write(&path, body).unwrap();
    path
}

fn unsigned_policy_for(approved_body: &[u8], upstream_url: &str) -> BoundaryPolicy {
    let approved_content_id = content_id(KEY.as_bytes(), SESSION, approved_body);
    BoundaryPolicy {
        schema: 1,
        policy_version: "policy-cli-test".to_string(),
        session_id: SESSION.to_string(),
        default_action: BoundaryAction::Deny,
        remote_fallback: false,
        redirects: RedirectPolicyConfig { follow: false },
        routes: vec![BoundaryRoute {
            id: "local-chat".to_string(),
            method: "POST".to_string(),
            operation: OPERATION.to_string(),
            upstream_url: upstream_url.to_string(),
        }],
        exceptions: vec![BoundaryException {
            id: "approved-cli-body".to_string(),
            route_id: "local-chat".to_string(),
            operation: OPERATION.to_string(),
            content_id: approved_content_id,
            expires_unix_secs: 2_000_000_000,
            reason: Some("synthetic CLI fixture".to_string()),
        }],
        limits: BoundaryLimits {
            max_request_bytes: 16 * 1024,
            max_response_bytes: 16 * 1024,
            max_header_bytes: 8 * 1024,
            request_timeout_secs: 2,
            max_in_flight_requests: 8,
        },
        credential_profile: Some("cli-test-no-upstream-auth".to_string()),
        policy_mac: String::new(),
    }
}

fn start_phone_home_recorder() -> (String, mpsc::Receiver<()>, mpsc::Sender<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let callback_url = format!("http://{}/license/callback", listener.local_addr().unwrap());
    let (hit_tx, hit_rx) = mpsc::channel();
    let (stop_tx, stop_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(12);
        loop {
            if stop_rx.try_recv().is_ok() || Instant::now() >= deadline {
                return;
            }
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let _ = stream.write_all(
                        b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 17\r\nconnection: close\r\n\r\n{\"revoked\":false}",
                    );
                    let _ = hit_tx.send(());
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(_) => return,
            }
        }
    });
    (callback_url, hit_rx, stop_tx)
}

#[test]
fn content_id_command_outputs_keyed_id_without_body_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let body = br#"{"model":"local","messages":[{"role":"user","content":"SYNTHETIC_CANARY"}]}"#;
    let request_path = write_request(&dir, "request.json", body);

    let output = Command::new(phantom_bin())
        .args([
            "boundary",
            "content-id",
            "--session",
            SESSION,
            "--content",
            request_path.to_str().unwrap(),
        ])
        .env("PHANTOM_BOUNDARY_POLICY_KEY", KEY)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.trim().starts_with("phb1:"));
    assert!(!stdout.contains("SYNTHETIC_CANARY"));
}

#[test]
fn check_json_reports_allow_and_deny_without_copying_content() {
    let dir = tempfile::tempdir().unwrap();
    let approved =
        br#"{"model":"local","messages":[{"role":"user","content":"APPROVED_SYNTHETIC"}]}"#;
    let denied =
        br#"{"model":"local","messages":[{"role":"user","content":"DENIED_SYNTHETIC_CANARY"}]}"#;
    let request_path = write_request(&dir, "approved.json", approved);
    let denied_path = write_request(&dir, "denied.json", denied);
    let unsigned_policy_path = dir.path().join("policy.unsigned.json");
    let policy_path = dir.path().join("policy.sealed.json");
    let unsigned = unsigned_policy_for(approved, "http://127.0.0.1:45555/v1/chat/completions");
    std::fs::write(
        &unsigned_policy_path,
        serde_json::to_string_pretty(&unsigned).unwrap(),
    )
    .unwrap();

    let sealed = Command::new(phantom_bin())
        .args([
            "boundary",
            "seal",
            "--policy",
            unsigned_policy_path.to_str().unwrap(),
            "--out",
            policy_path.to_str().unwrap(),
        ])
        .env("PHANTOM_BOUNDARY_POLICY_KEY", KEY)
        .output()
        .unwrap();
    assert!(
        sealed.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&sealed.stderr)
    );
    assert!(std::fs::read_to_string(&policy_path)
        .unwrap()
        .contains("policy_mac"));

    let allowed = Command::new(phantom_bin())
        .args([
            "--json",
            "boundary",
            "check",
            "--policy",
            policy_path.to_str().unwrap(),
            "--session",
            SESSION,
            "--content",
            request_path.to_str().unwrap(),
        ])
        .env("PHANTOM_BOUNDARY_POLICY_KEY", KEY)
        .output()
        .unwrap();
    assert!(
        allowed.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&allowed.stderr)
    );
    let allowed_stdout = String::from_utf8(allowed.stdout).unwrap();
    let allowed_json: serde_json::Value = serde_json::from_str(&allowed_stdout).unwrap();
    assert_eq!(allowed_json["data"]["outcome"], "allowed");
    assert_eq!(allowed_json["data"]["reason_code"], "exception_matched");
    assert!(!allowed_stdout.contains("APPROVED_SYNTHETIC"));

    let denied = Command::new(phantom_bin())
        .args([
            "--json",
            "boundary",
            "check",
            "--policy",
            policy_path.to_str().unwrap(),
            "--session",
            SESSION,
            "--content",
            denied_path.to_str().unwrap(),
        ])
        .env("PHANTOM_BOUNDARY_POLICY_KEY", KEY)
        .output()
        .unwrap();
    assert!(!denied.status.success());
    let denied_stdout = String::from_utf8(denied.stdout).unwrap();
    let denied_json: serde_json::Value = serde_json::from_str(&denied_stdout).unwrap();
    assert_eq!(denied_json["data"]["outcome"], "denied");
    assert_eq!(denied_json["data"]["reason_code"], "content_not_approved");
    assert!(!denied_stdout.contains("DENIED_SYNTHETIC_CANARY"));
}

#[test]
fn boundary_command_does_not_start_due_phone_home() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.json");
    let data_dir = dir.path().join("data");
    let (callback_url, rx, stop_recorder) = start_phone_home_recorder();

    let config = PhantomConfig {
        data_dir: Some(data_dir.display().to_string()),
        phone_home_url: Some(callback_url),
        phone_home_enabled: Some(true),
        phone_home_interval_secs: Some(0),
        privacy_notice_acknowledged_at: Some(1_800_000_000),
        privacy_notice_version_accepted: Some(u32::MAX),
        ..PhantomConfig::empty()
    };
    std::fs::write(&config_path, serde_json::to_string_pretty(&config).unwrap()).unwrap();

    let version = Command::new(phantom_bin())
        .arg("version")
        .env("PHANTOM_CONFIG", &config_path)
        .env("PHANTOM_DATA_DIR", &data_dir)
        .output()
        .unwrap();
    assert!(
        version.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&version.stderr)
    );
    rx.recv_timeout(Duration::from_secs(8))
        .expect("version command should trigger the synthetic callback");

    let request_path = write_request(
        &dir,
        "request.json",
        br#"{"model":"local","messages":[{"role":"user","content":"SYNTHETIC"}]}"#,
    );
    let output = Command::new(phantom_bin())
        .args([
            "boundary",
            "content-id",
            "--session",
            SESSION,
            "--content",
            request_path.to_str().unwrap(),
        ])
        .env("PHANTOM_BOUNDARY_POLICY_KEY", KEY)
        .env("PHANTOM_CONFIG", &config_path)
        .env("PHANTOM_DATA_DIR", &data_dir)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        rx.try_recv().is_err(),
        "boundary command unexpectedly triggered the synthetic callback"
    );
    let _ = stop_recorder.send(());
}
