use axum::{
    body::{Body, Bytes},
    extract::State,
    http::{header, HeaderMap, HeaderValue, Method, Request, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Router,
};
use hmac::{Hmac, Mac};
use http_body_util::BodyExt;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::future::Future;
use std::io::{BufRead, BufReader, Write};
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use thiserror::Error;
use tokio::net::TcpListener;
use tokio::sync::Semaphore;
use tokio::time::timeout;
use tower_http::timeout::TimeoutLayer;
use url::{Host, Url};

type HmacSha256 = Hmac<Sha256>;

pub const POLICY_SCHEMA: u32 = 1;
pub const RECEIPT_SCHEMA: u32 = 1;
pub const CONTENT_ID_PREFIX: &str = "phb1:";
pub const CHAT_COMPLETIONS_PATH: &str = "/v1/chat/completions";
pub const CHAT_COMPLETIONS_OPERATION: &str = "chat.completions.create";
pub const CHAT_COMPLETIONS_METHOD: &str = "POST";
pub const MAX_REQUEST_BYTES_CEILING: u64 = 2 * 1024 * 1024;
pub const MAX_RESPONSE_BYTES_CEILING: u64 = 10 * 1024 * 1024;
pub const MAX_HEADER_BYTES_CEILING: u64 = 64 * 1024;
pub const MAX_TIMEOUT_SECS_CEILING: u64 = 30;

#[derive(Debug, Error)]
pub enum BoundaryError {
    #[error("boundary key must be at least 16 bytes")]
    WeakKey,
    #[error("policy_mac is missing")]
    MissingPolicyMac,
    #[error("policy_mac verification failed")]
    PolicyMacMismatch,
    #[error("invalid policy: {0}")]
    InvalidPolicy(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("http client error: {0}")]
    HttpClient(#[from] reqwest::Error),
    #[error("invalid receipt: {0}")]
    InvalidReceipt(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BoundaryPolicy {
    pub schema: u32,
    pub policy_version: String,
    pub session_id: String,
    pub default_action: BoundaryAction,
    pub remote_fallback: bool,
    pub redirects: RedirectPolicyConfig,
    pub routes: Vec<BoundaryRoute>,
    pub exceptions: Vec<BoundaryException>,
    pub limits: BoundaryLimits,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_profile: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub policy_mac: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BoundaryAction {
    Deny,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RedirectPolicyConfig {
    pub follow: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BoundaryRoute {
    pub id: String,
    pub method: String,
    pub operation: String,
    pub upstream_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BoundaryException {
    pub id: String,
    pub route_id: String,
    pub operation: String,
    pub content_id: String,
    pub expires_unix_secs: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BoundaryLimits {
    pub max_request_bytes: u64,
    pub max_response_bytes: u64,
    pub max_header_bytes: u64,
    pub request_timeout_secs: u64,
    pub max_in_flight_requests: u16,
}

impl Default for BoundaryLimits {
    fn default() -> Self {
        Self {
            max_request_bytes: 128 * 1024,
            max_response_bytes: 1024 * 1024,
            max_header_bytes: 16 * 1024,
            request_timeout_secs: 10,
            max_in_flight_requests: 8,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoundaryRequest<'a> {
    pub session_id: &'a str,
    pub method: &'a str,
    pub operation: &'a str,
    pub gateway_path: &'a str,
    pub content: &'a [u8],
    pub now_unix_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DisclosureDecision {
    pub outcome: DecisionOutcome,
    pub receipt_stage: ReceiptStage,
    pub dispatch_state: DispatchState,
    pub reason_code: String,
    pub policy_version: String,
    pub session_id: String,
    pub operation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destination_origin_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destination_path: Option<String>,
    pub content_id: String,
    pub content_length: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exception_id: Option<String>,
}

impl DisclosureDecision {
    pub fn allowed(&self) -> bool {
        self.outcome == DecisionOutcome::Allowed
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DecisionOutcome {
    Allowed,
    Denied,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptStage {
    PreDispatch,
    PostDispatch,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DispatchState {
    NotStarted,
    StartedDeliveryUnknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReceiptLimits {
    pub controlled_path: String,
    pub not_controlled: Vec<String>,
    pub max_request_bytes: u64,
    pub max_response_bytes: u64,
    pub max_header_bytes: u64,
    pub request_timeout_secs: u64,
    pub max_in_flight_requests: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DisclosureReceipt {
    pub schema: u32,
    pub unix_secs: u64,
    pub outcome: DecisionOutcome,
    pub receipt_stage: ReceiptStage,
    pub dispatch_state: DispatchState,
    pub reason_code: String,
    pub policy_version: String,
    pub session_id: String,
    pub operation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destination_origin_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destination_path: Option<String>,
    pub content_id: String,
    pub content_length: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exception_id: Option<String>,
    pub limits: ReceiptLimits,
    pub mac_hex: String,
}

pub fn content_id(key: &[u8], session_id: &str, content: &[u8]) -> String {
    format!(
        "{}{}",
        CONTENT_ID_PREFIX,
        hmac_hex(
            key,
            b"phantom.boundary.content.v1",
            &[session_id.as_bytes(), content]
        )
    )
}

pub fn seal_policy(policy: BoundaryPolicy, key: &[u8]) -> Result<BoundaryPolicy, BoundaryError> {
    validate_key(key)?;
    validate_policy(&policy)?;
    let mut sealed = policy;
    sealed.policy_mac.clear();
    let bytes = canonical_policy_bytes(&sealed)?;
    sealed.policy_mac = hmac_hex(key, b"phantom.boundary.policy.v1", &[&bytes]);
    Ok(sealed)
}

pub fn load_policy(path: &Path, key: &[u8]) -> Result<BoundaryPolicy, BoundaryError> {
    validate_key(key)?;
    let raw = fs::read_to_string(path)?;
    let policy: BoundaryPolicy = serde_json::from_str(&raw)?;
    verify_policy(&policy, key)?;
    validate_policy(&policy)?;
    Ok(policy)
}

pub fn verify_policy(policy: &BoundaryPolicy, key: &[u8]) -> Result<(), BoundaryError> {
    validate_key(key)?;
    if policy.policy_mac.is_empty() {
        return Err(BoundaryError::MissingPolicyMac);
    }
    let expected = {
        let mut unsigned = policy.clone();
        unsigned.policy_mac.clear();
        let bytes = canonical_policy_bytes(&unsigned)?;
        hmac_hex(key, b"phantom.boundary.policy.v1", &[&bytes])
    };
    if !constant_time_eq(expected.as_bytes(), policy.policy_mac.as_bytes()) {
        return Err(BoundaryError::PolicyMacMismatch);
    }
    Ok(())
}

pub fn write_policy(path: &Path, policy: &BoundaryPolicy) -> Result<(), BoundaryError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(policy)?;
    fs::write(path, json)?;
    Ok(())
}

pub fn decide(
    policy: &BoundaryPolicy,
    key: &[u8],
    request: BoundaryRequest<'_>,
) -> Result<DisclosureDecision, BoundaryError> {
    validate_key(key)?;
    validate_policy(policy)?;
    let content_id = content_id(key, request.session_id, request.content);
    let route = policy.routes.first();
    let mut destination_origin = None;
    let mut destination_path = None;
    if let Some(route) = route {
        let parsed = Url::parse(&route.upstream_url)
            .map_err(|e| BoundaryError::InvalidPolicy(format!("invalid upstream_url: {e}")))?;
        destination_origin = Some(destination_origin_id(key, &parsed));
        destination_path = Some(parsed.path().to_string());
    }

    let base = |outcome: DecisionOutcome, reason_code: &str, exception_id: Option<String>| {
        DisclosureDecision {
            outcome,
            receipt_stage: ReceiptStage::PreDispatch,
            dispatch_state: DispatchState::NotStarted,
            reason_code: reason_code.to_string(),
            policy_version: policy.policy_version.clone(),
            session_id: request.session_id.to_string(),
            operation: request.operation.to_string(),
            route_id: route.map(|r| r.id.clone()),
            destination_origin_id: destination_origin.clone(),
            destination_path: destination_path.clone(),
            content_id: content_id.clone(),
            content_length: request.content.len() as u64,
            exception_id,
        }
    };

    if request.session_id != policy.session_id {
        return Ok(base(DecisionOutcome::Denied, "session_mismatch", None));
    }
    if request.method != CHAT_COMPLETIONS_METHOD || request.gateway_path != CHAT_COMPLETIONS_PATH {
        return Ok(base(DecisionOutcome::Denied, "unsupported_route", None));
    }
    if request.operation != CHAT_COMPLETIONS_OPERATION {
        return Ok(base(DecisionOutcome::Denied, "unsupported_operation", None));
    }
    if request.content.len() as u64 > policy.limits.max_request_bytes {
        return Ok(base(DecisionOutcome::Denied, "request_too_large", None));
    }

    let route = policy
        .routes
        .first()
        .ok_or_else(|| BoundaryError::InvalidPolicy("missing route".to_string()))?;
    let mut saw_expired_match = false;
    for exception in &policy.exceptions {
        if exception.route_id == route.id
            && exception.operation == request.operation
            && exception.content_id == content_id
        {
            if exception.expires_unix_secs < request.now_unix_secs {
                saw_expired_match = true;
                continue;
            }
            return Ok(base(
                DecisionOutcome::Allowed,
                "exception_matched",
                Some(exception.id.clone()),
            ));
        }
    }

    if saw_expired_match {
        Ok(base(DecisionOutcome::Denied, "exception_expired", None))
    } else {
        Ok(base(DecisionOutcome::Denied, "content_not_approved", None))
    }
}

pub fn append_receipt(
    path: &Path,
    policy: &BoundaryPolicy,
    decision: &DisclosureDecision,
    key: &[u8],
) -> Result<(), BoundaryError> {
    validate_key(key)?;
    let mut receipt = DisclosureReceipt {
        schema: RECEIPT_SCHEMA,
        unix_secs: now_unix_secs(),
        outcome: decision.outcome,
        receipt_stage: decision.receipt_stage,
        dispatch_state: decision.dispatch_state,
        reason_code: decision.reason_code.clone(),
        policy_version: decision.policy_version.clone(),
        session_id: decision.session_id.clone(),
        operation: decision.operation.clone(),
        route_id: decision.route_id.clone(),
        destination_origin_id: decision.destination_origin_id.clone(),
        destination_path: decision.destination_path.clone(),
        content_id: decision.content_id.clone(),
        content_length: decision.content_length,
        exception_id: decision.exception_id.clone(),
        limits: ReceiptLimits {
            controlled_path:
                "configured client -> Phantom boundary loopback gateway -> exact loopback upstream"
                    .to_string(),
            not_controlled: vec![
                "unconfigured clients".to_string(),
                "browser or desktop app network paths".to_string(),
                "shell networking outside this gateway".to_string(),
                "provider retention or legal ownership effects".to_string(),
                "chosen upstream side channels, shared mutable state, or tenant isolation defects"
                    .to_string(),
                "same-user host code that can read process environment or local policy files"
                    .to_string(),
                "malicious host processes with local file or process access".to_string(),
            ],
            max_request_bytes: policy.limits.max_request_bytes,
            max_response_bytes: policy.limits.max_response_bytes,
            max_header_bytes: policy.limits.max_header_bytes,
            request_timeout_secs: policy.limits.request_timeout_secs,
            max_in_flight_requests: policy.limits.max_in_flight_requests,
        },
        mac_hex: String::new(),
    };
    receipt.mac_hex = receipt_mac(key, &receipt)?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "{}", serde_json::to_string(&receipt)?)?;
    Ok(())
}

pub fn read_verified_receipts(
    path: &Path,
    key: &[u8],
) -> Result<Vec<DisclosureReceipt>, BoundaryError> {
    validate_key(key)?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let file = fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut verified = Vec::new();
    for (line_index, line) in reader.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let line_number = line_index + 1;
        let receipt: DisclosureReceipt = serde_json::from_str(&line)
            .map_err(|e| BoundaryError::InvalidReceipt(format!("line {line_number}: {e}")))?;
        let expected = receipt_mac(key, &receipt)?;
        if !constant_time_eq(expected.as_bytes(), receipt.mac_hex.as_bytes()) {
            return Err(BoundaryError::InvalidReceipt(format!(
                "line {line_number}: receipt MAC verification failed"
            )));
        }
        verified.push(receipt);
    }
    Ok(verified)
}

#[derive(Clone)]
pub struct GatewayConfig {
    pub policy: BoundaryPolicy,
    pub key: Vec<u8>,
    pub gateway_auth_token: String,
    pub receipt_log: PathBuf,
}

#[derive(Clone)]
struct GatewayState {
    policy: Arc<BoundaryPolicy>,
    key: Arc<Vec<u8>>,
    gateway_auth_token: Arc<String>,
    receipt_log: Arc<PathBuf>,
    client: reqwest::Client,
    in_flight: Arc<Semaphore>,
}

pub async fn serve_gateway<F>(
    listener: TcpListener,
    config: GatewayConfig,
    shutdown: F,
) -> Result<(), BoundaryError>
where
    F: Future<Output = ()> + Send + 'static,
{
    validate_key(&config.key)?;
    validate_policy(&config.policy)?;
    verify_policy(&config.policy, &config.key)?;
    if config.gateway_auth_token.trim().is_empty() {
        return invalid_policy("gateway auth token is required");
    }
    let listen_addr = listener.local_addr()?;
    if !listen_addr.ip().is_loopback() {
        return invalid_policy("listen address must be numeric loopback");
    }

    let timeout = Duration::from_secs(config.policy.limits.request_timeout_secs);
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .referer(false)
        .http1_only()
        .http1_max_headers(64)
        .no_gzip()
        .no_brotli()
        .no_deflate()
        .no_zstd()
        .pool_max_idle_per_host(0)
        .timeout(timeout)
        .read_timeout(timeout)
        .connect_timeout(timeout)
        .build()?;

    let state = GatewayState {
        in_flight: Arc::new(Semaphore::new(
            config.policy.limits.max_in_flight_requests as usize,
        )),
        policy: Arc::new(config.policy),
        key: Arc::new(config.key),
        gateway_auth_token: Arc::new(config.gateway_auth_token),
        receipt_log: Arc::new(config.receipt_log),
        client,
    };

    let handler_timeout = timeout.saturating_add(Duration::from_secs(1));
    let app = Router::new()
        .route(CHAT_COMPLETIONS_PATH, post(handle_chat_completions))
        .fallback(handle_unsupported_route)
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            handler_timeout,
        ))
        .with_state(state);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await?;
    Ok(())
}

async fn handle_chat_completions(
    State(state): State<GatewayState>,
    request: Request<Body>,
) -> Response {
    handle_gateway_request(state, request).await
}

async fn handle_unsupported_route(
    State(state): State<GatewayState>,
    request: Request<Body>,
) -> Response {
    handle_gateway_request(state, request).await
}

async fn handle_gateway_request(state: GatewayState, request: Request<Body>) -> Response {
    let (parts, body_stream) = request.into_parts();
    let method = parts.method;
    let path = parts.uri.path().to_string();
    let headers = parts.headers;
    let empty_body = Bytes::new();
    let permit = match state.in_flight.clone().try_acquire_owned() {
        Ok(permit) => permit,
        Err(_) => {
            return deny_with_receipt(
                &state,
                method.as_str(),
                &path,
                &empty_body,
                "too_many_in_flight_requests",
                StatusCode::SERVICE_UNAVAILABLE,
            );
        }
    };

    if !authorized(&headers, state.gateway_auth_token.as_str()) {
        drop(permit);
        return deny_with_receipt(
            &state,
            method.as_str(),
            &path,
            &empty_body,
            "gateway_auth_required",
            StatusCode::UNAUTHORIZED,
        );
    }

    if header_bytes(&headers) as u64 > state.policy.limits.max_header_bytes {
        drop(permit);
        return deny_with_receipt(
            &state,
            method.as_str(),
            &path,
            &empty_body,
            "headers_too_large",
            StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE,
        );
    }

    if method != Method::POST || path != CHAT_COMPLETIONS_PATH {
        drop(permit);
        return deny_with_receipt(
            &state,
            method.as_str(),
            &path,
            &empty_body,
            "unsupported_route",
            StatusCode::FORBIDDEN,
        );
    }

    let body = match bounded_request_body(body_stream, &state.policy.limits).await {
        Ok(body) => body,
        Err(RequestBodyError::TooLarge) => {
            drop(permit);
            return deny_with_receipt(
                &state,
                method.as_str(),
                &path,
                &empty_body,
                "request_too_large",
                StatusCode::PAYLOAD_TOO_LARGE,
            );
        }
        Err(RequestBodyError::Timeout) => {
            drop(permit);
            return deny_with_receipt(
                &state,
                method.as_str(),
                &path,
                &empty_body,
                "request_body_timeout",
                StatusCode::REQUEST_TIMEOUT,
            );
        }
        Err(RequestBodyError::Read) => {
            drop(permit);
            return deny_with_receipt(
                &state,
                method.as_str(),
                &path,
                &empty_body,
                "request_body_read_failed",
                StatusCode::BAD_REQUEST,
            );
        }
    };

    if asks_for_streaming(&headers, &body) {
        drop(permit);
        return deny_with_receipt(
            &state,
            method.as_str(),
            &path,
            &body,
            "unsupported_streaming",
            StatusCode::NOT_IMPLEMENTED,
        );
    }

    let request = BoundaryRequest {
        session_id: &state.policy.session_id,
        method: method.as_str(),
        operation: CHAT_COMPLETIONS_OPERATION,
        gateway_path: &path,
        content: &body,
        now_unix_secs: now_unix_secs(),
    };
    let decision = match decide(&state.policy, &state.key, request) {
        Ok(decision) => decision,
        Err(_) => {
            drop(permit);
            return deny_with_receipt(
                &state,
                method.as_str(),
                &path,
                &body,
                "policy_error",
                StatusCode::FORBIDDEN,
            );
        }
    };

    if !decision.allowed() {
        if append_decision_receipt(&state, &decision).is_err() {
            drop(permit);
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "receipt_write_failed");
        }
        drop(permit);
        return json_error(StatusCode::FORBIDDEN, &decision.reason_code);
    }

    if append_decision_receipt(&state, &decision).is_err() {
        drop(permit);
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "receipt_write_failed");
    }
    let route = match state.policy.routes.first() {
        Some(route) => route,
        None => {
            drop(permit);
            return deny_with_receipt(
                &state,
                method.as_str(),
                &path,
                &body,
                "policy_error",
                StatusCode::FORBIDDEN,
            );
        }
    };

    let upstream = state
        .client
        .post(route.upstream_url.clone())
        .header(header::CONTENT_TYPE, "application/json")
        .body(body.to_vec())
        .send()
        .await;
    let upstream = match upstream {
        Ok(response) => response,
        Err(_) => {
            drop(permit);
            return post_dispatch_transport_result(
                &state,
                &decision,
                "remote_fallback_denied",
                StatusCode::BAD_GATEWAY,
            );
        }
    };

    if upstream.status().is_redirection() {
        drop(permit);
        return post_dispatch_transport_result(
            &state,
            &decision,
            "redirect_denied",
            StatusCode::BAD_GATEWAY,
        );
    }

    let status = upstream.status();
    let content_type = upstream.headers().get(header::CONTENT_TYPE).cloned();
    let response_body =
        match bounded_upstream_body(upstream, state.policy.limits.max_response_bytes).await {
            Ok(body) => body,
            Err(UpstreamBodyError::TooLarge) => {
                drop(permit);
                return post_dispatch_transport_result(
                    &state,
                    &decision,
                    "upstream_response_too_large",
                    StatusCode::BAD_GATEWAY,
                );
            }
            Err(UpstreamBodyError::Read) => {
                drop(permit);
                return post_dispatch_transport_result(
                    &state,
                    &decision,
                    "upstream_read_failed",
                    StatusCode::BAD_GATEWAY,
                );
            }
        };
    drop(permit);

    let mut response = Response::builder().status(status);
    if let Some(content_type) = content_type {
        response = response.header(header::CONTENT_TYPE, content_type);
    }
    response
        .body(Body::from(response_body))
        .unwrap_or_else(|_| json_error(StatusCode::BAD_GATEWAY, "response_build_failed"))
}

enum RequestBodyError {
    TooLarge,
    Timeout,
    Read,
}

async fn bounded_request_body(
    body: Body,
    limits: &BoundaryLimits,
) -> Result<Bytes, RequestBodyError> {
    let max_bytes = limits.max_request_bytes;
    let timeout_duration = Duration::from_secs(limits.request_timeout_secs);
    match timeout(timeout_duration, read_request_body(body, max_bytes)).await {
        Ok(result) => result,
        Err(_) => Err(RequestBodyError::Timeout),
    }
}

async fn read_request_body(mut body: Body, max_bytes: u64) -> Result<Bytes, RequestBodyError> {
    let mut out = Vec::new();
    while let Some(frame) = body.frame().await {
        let frame = frame.map_err(|_| RequestBodyError::Read)?;
        if let Some(data) = frame.data_ref() {
            let next_len = out.len().saturating_add(data.len());
            if next_len as u64 > max_bytes {
                return Err(RequestBodyError::TooLarge);
            }
            out.extend_from_slice(data);
        }
    }
    Ok(Bytes::from(out))
}

enum UpstreamBodyError {
    TooLarge,
    Read,
}

async fn bounded_upstream_body(
    mut upstream: reqwest::Response,
    max_bytes: u64,
) -> Result<Vec<u8>, UpstreamBodyError> {
    if upstream
        .content_length()
        .is_some_and(|length| length > max_bytes)
    {
        return Err(UpstreamBodyError::TooLarge);
    }
    let mut body = Vec::new();
    while let Some(chunk) = upstream
        .chunk()
        .await
        .map_err(|_| UpstreamBodyError::Read)?
    {
        let next_len = body.len().saturating_add(chunk.len());
        if next_len as u64 > max_bytes {
            return Err(UpstreamBodyError::TooLarge);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn authorized(headers: &HeaderMap, token: &str) -> bool {
    let Some(actual) = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    let expected = format!("Bearer {token}");
    constant_time_eq(expected.as_bytes(), actual.as_bytes())
}

fn header_bytes(headers: &HeaderMap) -> usize {
    headers
        .iter()
        .map(|(name, value)| name.as_str().len() + value.as_bytes().len())
        .sum()
}

fn asks_for_streaming(headers: &HeaderMap, body: &[u8]) -> bool {
    let accept_stream = headers
        .get(header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .split(',')
                .any(|part| part.trim().eq_ignore_ascii_case("text/event-stream"))
        });
    if accept_stream {
        return true;
    }
    serde_json::from_slice::<serde_json::Value>(body)
        .ok()
        .and_then(|value| value.get("stream").and_then(|stream| stream.as_bool()))
        .unwrap_or(false)
}

fn deny_with_receipt(
    state: &GatewayState,
    method: &str,
    path: &str,
    body: &[u8],
    reason_code: &str,
    status: StatusCode,
) -> Response {
    let decision = synthetic_decision(state, method, path, body, reason_code);
    if append_decision_receipt(state, &decision).is_err() {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "receipt_write_failed");
    }
    json_error(status, reason_code)
}

fn post_dispatch_transport_result(
    state: &GatewayState,
    original_decision: &DisclosureDecision,
    reason_code: &str,
    status: StatusCode,
) -> Response {
    let mut decision = original_decision.clone();
    decision.receipt_stage = ReceiptStage::PostDispatch;
    decision.dispatch_state = DispatchState::StartedDeliveryUnknown;
    decision.reason_code = reason_code.to_string();
    if append_decision_receipt(state, &decision).is_err() {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "receipt_write_failed");
    }
    json_error(status, reason_code)
}

fn synthetic_decision(
    state: &GatewayState,
    method: &str,
    path: &str,
    body: &[u8],
    reason_code: &str,
) -> DisclosureDecision {
    let route = state.policy.routes.first();
    let (destination_origin_id, destination_path) = match route {
        Some(route) => Url::parse(&route.upstream_url)
            .ok()
            .map(|url| {
                (
                    Some(destination_origin_id(&state.key, &url)),
                    Some(url.path().to_string()),
                )
            })
            .unwrap_or((None, None)),
        None => (None, None),
    };
    DisclosureDecision {
        outcome: DecisionOutcome::Denied,
        receipt_stage: ReceiptStage::PreDispatch,
        dispatch_state: DispatchState::NotStarted,
        reason_code: reason_code.to_string(),
        policy_version: state.policy.policy_version.clone(),
        session_id: state.policy.session_id.clone(),
        operation: if method == CHAT_COMPLETIONS_METHOD && path == CHAT_COMPLETIONS_PATH {
            CHAT_COMPLETIONS_OPERATION.to_string()
        } else {
            "unsupported".to_string()
        },
        route_id: route.map(|route| route.id.clone()),
        destination_origin_id,
        destination_path,
        content_id: content_id(&state.key, &state.policy.session_id, body),
        content_length: body.len() as u64,
        exception_id: None,
    }
}

fn append_decision_receipt(
    state: &GatewayState,
    decision: &DisclosureDecision,
) -> Result<(), BoundaryError> {
    append_receipt(&state.receipt_log, &state.policy, decision, &state.key)
}

fn json_error(status: StatusCode, code: &str) -> Response {
    let body = serde_json::json!({
        "error": {
            "type": "phantom_boundary",
            "code": code,
            "message": "Phantom boundary rejected the request"
        }
    })
    .to_string();
    (
        status,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        )],
        body,
    )
        .into_response()
}

fn validate_policy(policy: &BoundaryPolicy) -> Result<(), BoundaryError> {
    if policy.schema != POLICY_SCHEMA {
        return invalid_policy(format!("schema must be {POLICY_SCHEMA}"));
    }
    if policy.policy_version.trim().is_empty() {
        return invalid_policy("policy_version is required");
    }
    if policy.session_id.trim().is_empty() {
        return invalid_policy("session_id is required");
    }
    if policy.default_action != BoundaryAction::Deny {
        return invalid_policy("default_action must be deny");
    }
    if policy.remote_fallback {
        return invalid_policy("remote_fallback must be false");
    }
    if policy.redirects.follow {
        return invalid_policy("redirect following is not supported");
    }
    validate_limits(&policy.limits)?;
    if policy.routes.len() != 1 {
        return invalid_policy("first release requires exactly one fixed route");
    }

    let mut route_ids = HashSet::new();
    for route in &policy.routes {
        validate_route(route)?;
        if !route_ids.insert(route.id.as_str()) {
            return invalid_policy("route id must be unique");
        }
    }

    for exception in &policy.exceptions {
        if exception.id.trim().is_empty() {
            return invalid_policy("exception id is required");
        }
        if !route_ids.contains(exception.route_id.as_str()) {
            return invalid_policy("exception route_id does not reference a route");
        }
        if exception.operation != CHAT_COMPLETIONS_OPERATION {
            return invalid_policy("exception operation must match chat.completions.create");
        }
        if !exception.content_id.starts_with(CONTENT_ID_PREFIX)
            || exception.content_id.len() != CONTENT_ID_PREFIX.len() + 64
        {
            return invalid_policy("exception content_id must be a phb1 keyed id");
        }
        if exception.expires_unix_secs == 0 {
            return invalid_policy("exception expiry is required");
        }
    }

    Ok(())
}

fn validate_limits(limits: &BoundaryLimits) -> Result<(), BoundaryError> {
    if limits.max_request_bytes == 0 || limits.max_request_bytes > MAX_REQUEST_BYTES_CEILING {
        return invalid_policy(format!(
            "max_request_bytes must be 1..={MAX_REQUEST_BYTES_CEILING}"
        ));
    }
    if limits.max_response_bytes == 0 || limits.max_response_bytes > MAX_RESPONSE_BYTES_CEILING {
        return invalid_policy(format!(
            "max_response_bytes must be 1..={MAX_RESPONSE_BYTES_CEILING}"
        ));
    }
    if limits.max_header_bytes == 0 || limits.max_header_bytes > MAX_HEADER_BYTES_CEILING {
        return invalid_policy(format!(
            "max_header_bytes must be 1..={MAX_HEADER_BYTES_CEILING}"
        ));
    }
    if limits.request_timeout_secs == 0 || limits.request_timeout_secs > MAX_TIMEOUT_SECS_CEILING {
        return invalid_policy(format!(
            "request_timeout_secs must be 1..={MAX_TIMEOUT_SECS_CEILING}"
        ));
    }
    if limits.max_in_flight_requests == 0 {
        return invalid_policy("max_in_flight_requests must be at least 1");
    }
    Ok(())
}

fn validate_route(route: &BoundaryRoute) -> Result<(), BoundaryError> {
    if route.id.trim().is_empty() {
        return invalid_policy("route id is required");
    }
    if route.method != CHAT_COMPLETIONS_METHOD {
        return invalid_policy("route method must be POST");
    }
    if route.operation != CHAT_COMPLETIONS_OPERATION {
        return invalid_policy("route operation must be chat.completions.create");
    }
    let parsed = Url::parse(&route.upstream_url)
        .map_err(|e| BoundaryError::InvalidPolicy(format!("invalid upstream_url: {e}")))?;
    if parsed.scheme() != "http" {
        return invalid_policy("upstream_url must use http and a numeric loopback host");
    }
    if parsed.username() != "" || parsed.password().is_some() {
        return invalid_policy("upstream_url must not contain userinfo");
    }
    if parsed.fragment().is_some() || parsed.query().is_some() {
        return invalid_policy("upstream_url must not contain query or fragment");
    }
    if parsed.port().is_none() {
        return invalid_policy("upstream_url must include an explicit loopback port");
    }
    if parsed.path() != CHAT_COMPLETIONS_PATH {
        return invalid_policy("upstream_url path must be /v1/chat/completions");
    }
    match parsed.host() {
        Some(Host::Ipv4(addr)) if IpAddr::V4(addr).is_loopback() => Ok(()),
        Some(Host::Ipv6(addr)) if IpAddr::V6(addr).is_loopback() => Ok(()),
        _ => invalid_policy("upstream_url host must be numeric loopback"),
    }
}

fn validate_key(key: &[u8]) -> Result<(), BoundaryError> {
    if key.len() < 16 {
        Err(BoundaryError::WeakKey)
    } else {
        Ok(())
    }
}

fn invalid_policy<T>(message: impl Into<String>) -> Result<T, BoundaryError> {
    Err(BoundaryError::InvalidPolicy(message.into()))
}

fn canonical_policy_bytes(policy: &BoundaryPolicy) -> Result<Vec<u8>, BoundaryError> {
    let mut unsigned = policy.clone();
    unsigned.policy_mac.clear();
    Ok(serde_json::to_vec(&unsigned)?)
}

fn receipt_mac(key: &[u8], receipt: &DisclosureReceipt) -> Result<String, BoundaryError> {
    let mut unsigned = receipt.clone();
    unsigned.mac_hex.clear();
    let bytes = serde_json::to_vec(&unsigned)?;
    Ok(hmac_hex(key, b"phantom.boundary.receipt.v1", &[&bytes]))
}

fn destination_origin_id(key: &[u8], url: &Url) -> String {
    let host = url.host_str().unwrap_or_default();
    let port = url.port().unwrap_or(80);
    let origin = format!("{}://{}:{}", url.scheme(), host, port);
    format!(
        "{}{}",
        CONTENT_ID_PREFIX,
        hmac_hex(
            key,
            b"phantom.boundary.destination-origin.v1",
            &[origin.as_bytes()]
        )
    )
}

fn hmac_hex(key: &[u8], domain: &[u8], parts: &[&[u8]]) -> String {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(domain);
    for part in parts {
        mac.update(&(part.len() as u64).to_le_bytes());
        mac.update(part);
    }
    hex_encode(&mac.finalize().into_bytes())
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn constant_time_eq(expected: &[u8], actual: &[u8]) -> bool {
    let mut diff = expected.len() ^ actual.len();
    for (index, expected_byte) in expected.iter().enumerate() {
        let actual_byte = actual.get(index).copied().unwrap_or(0);
        diff |= (*expected_byte ^ actual_byte) as usize;
    }
    diff == 0
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
