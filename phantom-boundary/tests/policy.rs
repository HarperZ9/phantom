use phantom_boundary::{
    append_receipt, content_id, decide, load_policy, read_verified_receipts, seal_policy,
    BoundaryAction, BoundaryException, BoundaryLimits, BoundaryPolicy, BoundaryRequest,
    BoundaryRoute, DecisionOutcome, RedirectPolicyConfig,
};
use std::fs;
use std::path::Path;

const KEY: &[u8] = b"operator-owned-boundary-key-32-bytes";
const SESSION_A: &str = "session-a";
const SESSION_B: &str = "session-b";
const OPERATION: &str = "chat.completions.create";
const METHOD: &str = "POST";
const PATH: &str = "/v1/chat/completions";
const UPSTREAM: &str = "http://127.0.0.1:45555/v1/chat/completions";

fn limits() -> BoundaryLimits {
    BoundaryLimits {
        max_request_bytes: 16 * 1024,
        max_response_bytes: 16 * 1024,
        max_header_bytes: 8 * 1024,
        request_timeout_secs: 2,
        max_in_flight_requests: 8,
    }
}

fn unsigned_policy(approved_content_id: &str) -> BoundaryPolicy {
    BoundaryPolicy {
        schema: 1,
        policy_version: "policy-2026-09-09".to_string(),
        session_id: SESSION_A.to_string(),
        default_action: BoundaryAction::Deny,
        remote_fallback: false,
        redirects: RedirectPolicyConfig { follow: false },
        routes: vec![BoundaryRoute {
            id: "local-chat".to_string(),
            method: METHOD.to_string(),
            operation: OPERATION.to_string(),
            upstream_url: UPSTREAM.to_string(),
        }],
        exceptions: vec![BoundaryException {
            id: "operator-approved-synthetic".to_string(),
            route_id: "local-chat".to_string(),
            operation: OPERATION.to_string(),
            content_id: approved_content_id.to_string(),
            expires_unix_secs: 2_000_000_000,
            reason: Some("synthetic fixture approval".to_string()),
        }],
        limits: limits(),
        credential_profile: Some("local-dev-no-provider-key-forwarding".to_string()),
        policy_mac: String::new(),
    }
}

fn write_policy(path: &Path, policy: &BoundaryPolicy) {
    fs::write(path, serde_json::to_string_pretty(policy).unwrap()).unwrap();
}

fn request<'a>(session_id: &'a str, body: &'a [u8]) -> BoundaryRequest<'a> {
    BoundaryRequest {
        session_id,
        method: METHOD,
        operation: OPERATION,
        gateway_path: PATH,
        content: body,
        now_unix_secs: 1_800_000_000,
    }
}

#[test]
fn sealed_policy_loads_and_route_tampering_fails() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("policy.json");
    let approved =
        br#"{"model":"local","messages":[{"role":"user","content":"approved synthetic"}]}"#;
    let sealed = seal_policy(unsigned_policy(&content_id(KEY, SESSION_A, approved)), KEY).unwrap();
    write_policy(&path, &sealed);

    let loaded = load_policy(&path, KEY).unwrap();
    assert_eq!(loaded.policy_version, "policy-2026-09-09");
    assert!(!loaded.policy_mac.is_empty());

    let mut tampered: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    tampered["routes"][0]["upstream_url"] =
        serde_json::json!("http://127.0.0.1:45556/v1/chat/completions");
    fs::write(&path, serde_json::to_string_pretty(&tampered).unwrap()).unwrap();

    let err = load_policy(&path, KEY).unwrap_err().to_string();
    assert!(err.contains("policy_mac"));
}

#[test]
fn content_ids_are_keyed_and_do_not_expose_a_raw_sha256() {
    let body = b"low entropy canary";
    let id_a = content_id(KEY, SESSION_A, body);
    let id_b = content_id(b"different-operator-key-32-bytes", SESSION_A, body);
    let id_session_b = content_id(KEY, SESSION_B, body);
    let raw_sha = {
        use sha2::{Digest, Sha256};
        let digest = Sha256::digest(body);
        digest
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>()
    };

    assert_ne!(id_a, id_b);
    assert_ne!(id_a, id_session_b);
    assert_ne!(id_a, raw_sha);
    assert!(id_a.starts_with("phb1:"));
}

#[test]
fn policy_decision_allows_only_exact_session_route_operation_and_content() {
    let approved =
        br#"{"model":"local","messages":[{"role":"user","content":"approved synthetic"}]}"#;
    let sealed = seal_policy(unsigned_policy(&content_id(KEY, SESSION_A, approved)), KEY).unwrap();

    let allowed = decide(&sealed, KEY, request(SESSION_A, approved)).unwrap();
    assert_eq!(allowed.outcome, DecisionOutcome::Allowed);
    assert_eq!(allowed.reason_code, "exception_matched");
    assert_eq!(
        allowed.exception_id.as_deref(),
        Some("operator-approved-synthetic")
    );

    let denied_content = decide(&sealed, KEY, request(SESSION_A, b"private canary")).unwrap();
    assert_eq!(denied_content.outcome, DecisionOutcome::Denied);
    assert_eq!(denied_content.reason_code, "content_not_approved");

    let denied_session = decide(&sealed, KEY, request(SESSION_B, approved)).unwrap();
    assert_eq!(denied_session.outcome, DecisionOutcome::Denied);
    assert_eq!(denied_session.reason_code, "session_mismatch");

    let denied_route = decide(
        &sealed,
        KEY,
        BoundaryRequest {
            gateway_path: "/v1/responses",
            ..request(SESSION_A, approved)
        },
    )
    .unwrap();
    assert_eq!(denied_route.outcome, DecisionOutcome::Denied);
    assert_eq!(denied_route.reason_code, "unsupported_route");

    let denied_operation = decide(
        &sealed,
        KEY,
        BoundaryRequest {
            operation: "responses.create",
            ..request(SESSION_A, approved)
        },
    )
    .unwrap();
    assert_eq!(denied_operation.outcome, DecisionOutcome::Denied);
    assert_eq!(denied_operation.reason_code, "unsupported_operation");
}

#[test]
fn expired_exception_denies_instead_of_inheriting_route_approval() {
    let approved = b"approved once";
    let mut policy = unsigned_policy(&content_id(KEY, SESSION_A, approved));
    policy.exceptions[0].expires_unix_secs = 1_700_000_000;
    let sealed = seal_policy(policy, KEY).unwrap();

    let denied = decide(&sealed, KEY, request(SESSION_A, approved)).unwrap();
    assert_eq!(denied.outcome, DecisionOutcome::Denied);
    assert_eq!(denied.reason_code, "exception_expired");
}

#[test]
fn unsupported_policy_shapes_fail_closed_before_serving() {
    let approved = b"approved";

    let mut remote_fallback = unsigned_policy(&content_id(KEY, SESSION_A, approved));
    remote_fallback.remote_fallback = true;
    assert!(seal_policy(remote_fallback, KEY)
        .unwrap_err()
        .to_string()
        .contains("remote_fallback"));

    let mut redirects = unsigned_policy(&content_id(KEY, SESSION_A, approved));
    redirects.redirects.follow = true;
    assert!(seal_policy(redirects, KEY)
        .unwrap_err()
        .to_string()
        .contains("redirect"));

    let mut hostname = unsigned_policy(&content_id(KEY, SESSION_A, approved));
    hostname.routes[0].upstream_url = "http://localhost:45555/v1/chat/completions".to_string();
    assert!(seal_policy(hostname, KEY)
        .unwrap_err()
        .to_string()
        .contains("numeric loopback"));

    let mut remote = unsigned_policy(&content_id(KEY, SESSION_A, approved));
    remote.routes[0].upstream_url = "https://api.openai.com/v1/chat/completions".to_string();
    assert!(seal_policy(remote, KEY)
        .unwrap_err()
        .to_string()
        .contains("loopback"));

    let mut oversized = unsigned_policy(&content_id(KEY, SESSION_A, approved));
    oversized.limits.max_request_bytes = 2 * 1024 * 1024 + 1;
    assert!(seal_policy(oversized, KEY)
        .unwrap_err()
        .to_string()
        .contains("max_request_bytes"));

    let mut no_in_flight = unsigned_policy(&content_id(KEY, SESSION_A, approved));
    no_in_flight.limits.max_in_flight_requests = 0;
    assert!(seal_policy(no_in_flight, KEY)
        .unwrap_err()
        .to_string()
        .contains("max_in_flight_requests"));
}

#[test]
fn receipts_are_content_free_and_tamper_evident() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("receipts.jsonl");
    let approved = b"approved synthetic";
    let sealed = seal_policy(unsigned_policy(&content_id(KEY, SESSION_A, approved)), KEY).unwrap();
    let decision = decide(&sealed, KEY, request(SESSION_A, approved)).unwrap();

    append_receipt(&path, &sealed, &decision, KEY).unwrap();
    let raw = fs::read_to_string(&path).unwrap();
    assert!(!raw.contains("approved synthetic"));
    assert!(raw.contains("exception_matched"));

    let receipts = read_verified_receipts(&path, KEY).unwrap();
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].outcome, DecisionOutcome::Allowed);

    let tampered = raw.replace("allowed", "denied");
    fs::write(&path, tampered).unwrap();
    let error = read_verified_receipts(&path, KEY).unwrap_err().to_string();
    assert!(error.contains("receipt MAC verification failed"));

    fs::write(&path, "{not-json}\n").unwrap();
    let error = read_verified_receipts(&path, KEY).unwrap_err().to_string();
    assert!(error.contains("invalid receipt"));
}
