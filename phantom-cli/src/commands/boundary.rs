use crate::cli::BoundaryAction;
use phantom_boundary::{
    content_id, decide, load_policy, seal_policy, serve_gateway, write_policy, BoundaryPolicy,
    BoundaryRequest, DecisionOutcome, GatewayConfig, CHAT_COMPLETIONS_METHOD,
    CHAT_COMPLETIONS_OPERATION, CHAT_COMPLETIONS_PATH,
};
use phantom_cli::json_out::Envelope;
use phantom_cli::profile;
use serde_json::Value;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn run(action: BoundaryAction, json: bool) {
    match action {
        BoundaryAction::ContentId {
            session,
            content,
            key_env,
        } => {
            let key = key_from_env(&key_env, "boundary content-id", json);
            let body = read_content(&content, "boundary content-id", json);
            println!("{}", content_id(&key, &session, &body));
        }
        BoundaryAction::Seal {
            policy,
            out,
            key_env,
        } => {
            let key = key_from_env(&key_env, "boundary seal", json);
            let raw = read_to_string(&policy, "boundary seal", json);
            let unsigned: BoundaryPolicy = serde_json::from_str(&raw).unwrap_or_else(|e| {
                fail("boundary seal", json, format!("invalid policy JSON: {e}"))
            });
            let sealed = seal_policy(unsigned, &key)
                .unwrap_or_else(|e| fail("boundary seal", json, e.to_string()));
            if let Some(out) = out {
                write_policy(&out, &sealed)
                    .unwrap_or_else(|e| fail("boundary seal", json, e.to_string()));
                if json {
                    Envelope::ok(
                        "boundary seal",
                        serde_json::json!({ "policy": out.display().to_string() }),
                    )
                    .print();
                } else {
                    println!("sealed policy written to {}", out.display());
                }
            } else {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&sealed)
                        .unwrap_or_else(|_| "{\"error\":\"serialization failed\"}".to_string())
                );
            }
        }
        BoundaryAction::Check {
            policy,
            session,
            content,
            operation,
            key_env,
        } => {
            let key = key_from_env(&key_env, "boundary check", json);
            let policy = load_policy(&policy, &key)
                .unwrap_or_else(|e| fail("boundary check", json, e.to_string()));
            let body = read_content(&content, "boundary check", json);
            let request = BoundaryRequest {
                session_id: &session,
                method: CHAT_COMPLETIONS_METHOD,
                operation: &operation,
                gateway_path: CHAT_COMPLETIONS_PATH,
                content: &body,
                now_unix_secs: now_unix_secs(),
            };
            let decision = decide(&policy, &key, request)
                .unwrap_or_else(|e| fail("boundary check", json, e.to_string()));
            if json {
                Envelope::ok("boundary check", &decision).print();
            } else {
                println!(
                    "boundary decision: {:?} ({}) content_id={}",
                    decision.outcome, decision.reason_code, decision.content_id
                );
            }
            if decision.outcome == DecisionOutcome::Denied {
                std::process::exit(2);
            }
        }
        BoundaryAction::Serve {
            policy,
            listen,
            receipt_log,
            key_env,
            auth_token_env,
        } => {
            let key = key_from_env(&key_env, "boundary serve", json);
            let auth = std::env::var(&auth_token_env).unwrap_or_else(|_| {
                fail(
                    "boundary serve",
                    json,
                    format!("{auth_token_env} is not set for gateway auth"),
                )
            });
            let policy = load_policy(&policy, &key)
                .unwrap_or_else(|e| fail("boundary serve", json, e.to_string()));
            let receipt_log = receipt_log.unwrap_or_else(default_receipt_log);
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap_or_else(|e| fail("boundary serve", json, format!("runtime error: {e}")));
            runtime.block_on(async move {
                let listener = tokio::net::TcpListener::bind(listen)
                    .await
                    .unwrap_or_else(|e| fail("boundary serve", json, format!("bind failed: {e}")));
                let policy_version = policy.policy_version.clone();
                let receipt_log_display = receipt_log.display().to_string();
                if json {
                    Envelope::ok(
                        "boundary serve",
                        serde_json::json!({
                            "listen": listen.to_string(),
                            "policy_version": policy_version,
                            "receipt_log": receipt_log_display,
                            "path": CHAT_COMPLETIONS_PATH,
                            "operation": CHAT_COMPLETIONS_OPERATION
                        }),
                    )
                    .print();
                } else {
                    println!(
                        "phantom boundary serving {} on http://{}",
                        CHAT_COMPLETIONS_PATH, listen
                    );
                    println!("receipt log: {}", receipt_log.display());
                }
                serve_gateway(
                    listener,
                    GatewayConfig {
                        policy,
                        key,
                        gateway_auth_token: auth,
                        receipt_log,
                    },
                    async {
                        let _ = tokio::signal::ctrl_c().await;
                    },
                )
                .await
                .unwrap_or_else(|e| fail("boundary serve", json, e.to_string()));
            });
        }
    }
}

fn key_from_env(env_name: &str, command: &'static str, json: bool) -> Vec<u8> {
    let key = std::env::var(env_name)
        .unwrap_or_else(|_| fail(command, json, format!("{env_name} is not set")));
    if key.len() < 16 {
        fail(
            command,
            json,
            format!("{env_name} must be at least 16 bytes"),
        );
    }
    key.into_bytes()
}

fn read_content(path: &Path, command: &'static str, json: bool) -> Vec<u8> {
    if path.as_os_str() == "-" {
        let mut body = Vec::new();
        std::io::stdin()
            .read_to_end(&mut body)
            .unwrap_or_else(|e| fail(command, json, format!("stdin read failed: {e}")));
        return body;
    }
    std::fs::read(path).unwrap_or_else(|e| fail(command, json, format!("content read failed: {e}")))
}

fn read_to_string(path: &Path, command: &'static str, json: bool) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| fail(command, json, format!("read failed: {e}")))
}

fn default_receipt_log() -> PathBuf {
    profile::logs_dir().join("boundary-receipts.jsonl")
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn fail(command: &'static str, json: bool, message: impl Into<String>) -> ! {
    let message = message.into();
    if json {
        Envelope::<Value>::error(command, message).print();
    } else {
        eprintln!("boundary error: {message}");
    }
    std::process::exit(1);
}
