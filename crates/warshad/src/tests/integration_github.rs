//! `integrations.test` for GitHub: the one integration whose check leaves the machine.
//!
//! The rule these tests hold the daemon to is D-RN-ONBOARDING-1: one GitHub token, one store,
//! one reader. `integrations.connect github` files the token, `resolve_github_token` finds it,
//! and `integrations.test`, `projects.github_status` and `projects.create_github_repo` all read
//! it through that one function — so "the settings screen says connected" and "the repository
//! was created" can never describe two different credentials.
//!
//! Every test here talks to a loopback mock, never to GitHub: the API base is set on the
//! `DaemonConfig`, which is the same switch an install pointed at GitHub Enterprise uses.

use super::*;
use axum::extract::State as AxumState;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::get;
use axum::{Json, Router};
use std::sync::Mutex;
use warsha_core::IntegrationConfig;
use warsha_providers::{EncryptedFileSecretStore, FallbackSecretStore};

/// The token the mock server is told to expect, and the one the encrypted-file test files.
const MOCK_TOKEN: &str = "ghp_mock_onboarding_token";

/// A mock GitHub `/user` endpoint: one status, one body, and every `Authorization` it saw.
#[derive(Clone)]
struct MockUser {
    status: u16,
    body: serde_json::Value,
    seen: Arc<Mutex<Vec<String>>>,
}

async fn answer_user(
    AxumState(state): AxumState<MockUser>,
    headers: HeaderMap,
) -> (StatusCode, Json<serde_json::Value>) {
    state.seen.lock().expect("the mock lock is held").push(
        headers
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
            .to_string(),
    );
    let status = StatusCode::from_u16(state.status).expect("the mock status is valid");
    (status, Json(state.body))
}

/// Start a mock GitHub on a loopback port: `(base_url, authorization headers seen, server)`.
///
/// The server task is returned so the test can abort it; the loopback address is the only thing
/// that leaves this function, so nothing in the test knows a port number in advance.
async fn mock_github(
    status: u16,
    body: serde_json::Value,
) -> (String, Arc<Mutex<Vec<String>>>, tokio::task::JoinHandle<()>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let app = Router::new()
        .route("/user", get(answer_user))
        .with_state(MockUser {
            status,
            body,
            seen: seen.clone(),
        });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("the mock github binds a loopback port");
    let addr = listener.local_addr().expect("the mock has an address");
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (format!("http://{addr}"), seen, server)
}

/// A daemon over `dir` whose GitHub calls go to `base_url`, plus the owner session.
async fn rpc_over(dir: &Path, base_url: &str) -> (WarshaRpc, RpcExtensions) {
    let (state, _store) = test_state_with(dir, |cfg| {
        cfg.github_api_base = Some(base_url.to_string());
    })
    .await;
    (WarshaRpc::new(state), owner_ext())
}

async fn connect_github(rpc: &WarshaRpc, owner: &RpcExtensions, token: Option<&str>) {
    rpc.connect_integration(
        owner,
        "github".to_string(),
        IntegrationConfig {
            token: token.map(str::to_string),
            ..Default::default()
        },
    )
    .await
    .expect("integrations.connect files the configuration");
}

#[tokio::test]
async fn a_working_token_reports_the_account_it_belongs_to() {
    let _env_lock = crate::test_util::ENV_LOCK.lock().await;
    let temp = crate::test_util::fresh_tempdir();
    let (base_url, seen, server) =
        mock_github(200, serde_json::json!({"login": "octo-mock"})).await;
    let (rpc, owner) = rpc_over(temp.path(), &base_url).await;
    connect_github(&rpc, &owner, Some(MOCK_TOKEN)).await;

    let result = rpc
        .test_integration(&owner, "github".to_string())
        .await
        .expect("integrations.test answers");

    assert_eq!(result.status, "ok");
    assert_eq!(result.account.as_deref(), Some("octo-mock"));
    assert!(
        result.message.contains("octo-mock"),
        "the message names the account: {}",
        result.message
    );
    // The token itself is never in the answer: the settings screen shows this text.
    assert!(!result.message.contains(MOCK_TOKEN));

    {
        let seen = seen.lock().expect("the mock lock is held");
        assert_eq!(seen.len(), 1, "one live call, no retries");
        assert_eq!(seen[0], format!("Bearer {MOCK_TOKEN}"));
    }

    // The same token, through the same reader, is what `projects.github_status` reports.
    let status = rpc
        .project_github_status(&owner)
        .await
        .expect("projects.github_status answers");
    assert!(status.configured);
    assert_eq!(status.source.as_deref(), Some("integrations"));

    server.abort();
}

#[tokio::test]
async fn a_rejected_token_is_a_failure_with_no_account() {
    let _env_lock = crate::test_util::ENV_LOCK.lock().await;
    let temp = crate::test_util::fresh_tempdir();
    let (base_url, seen, server) = mock_github(
        401,
        serde_json::json!({"message": "Bad credentials", "documentation_url": "https://docs.github.com"}),
    )
    .await;
    let (rpc, owner) = rpc_over(temp.path(), &base_url).await;
    connect_github(&rpc, &owner, Some(MOCK_TOKEN)).await;

    let result = rpc
        .test_integration(&owner, "github".to_string())
        .await
        .expect("a rejected token is an answer, not an RPC error");

    assert_eq!(result.status, "failed");
    assert_eq!(result.account, None);
    assert!(
        result.message.contains("rejected"),
        "the message says GitHub refused the token: {}",
        result.message
    );
    assert!(!result.message.contains(MOCK_TOKEN));
    assert_eq!(seen.lock().expect("the mock lock is held").len(), 1);

    server.abort();
}

#[tokio::test]
async fn a_broken_answer_is_a_failure_the_owner_can_read() {
    let _env_lock = crate::test_util::ENV_LOCK.lock().await;
    let temp = crate::test_util::fresh_tempdir();
    // A 500 whose body happens to quote the token: the message must still not carry it.
    let (base_url, _seen, server) = mock_github(
        500,
        serde_json::json!({"message": format!("proxy error for {MOCK_TOKEN}")}),
    )
    .await;
    let (rpc, owner) = rpc_over(temp.path(), &base_url).await;
    connect_github(&rpc, &owner, Some(MOCK_TOKEN)).await;

    let result = rpc
        .test_integration(&owner, "github".to_string())
        .await
        .expect("a broken answer is an answer, not an RPC error");

    assert_eq!(result.status, "failed");
    assert_eq!(result.account, None);
    assert!(
        result.message.contains("[REDACTED]"),
        "the token is redacted out of the reason: {}",
        result.message
    );
    assert!(!result.message.contains(MOCK_TOKEN));

    server.abort();
}

#[tokio::test]
async fn a_network_error_is_a_failure_with_no_account() {
    let _env_lock = crate::test_util::ENV_LOCK.lock().await;
    let temp = crate::test_util::fresh_tempdir();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind an ephemeral port");
    let closed_addr = listener.local_addr().expect("read bound address");
    drop(listener);
    let base_url = format!("http://{closed_addr}");

    let (rpc, owner) = rpc_over(temp.path(), &base_url).await;
    connect_github(&rpc, &owner, Some(MOCK_TOKEN)).await;

    let result = rpc
        .test_integration(&owner, "github".to_string())
        .await
        .expect("a network error is an answer, not an RPC error");

    assert_eq!(result.status, "failed");
    assert_eq!(result.account, None);
    assert!(
        !result.message.is_empty(),
        "the failure includes a readable message"
    );
    assert!(!result.message.contains(MOCK_TOKEN));
}

#[tokio::test]
async fn no_token_is_unconfigured_and_reaches_nothing() {
    let _env_lock = crate::test_util::ENV_LOCK.lock().await;
    let temp = crate::test_util::fresh_tempdir();
    let (base_url, seen, server) =
        mock_github(200, serde_json::json!({"login": "octo-mock"})).await;
    let (rpc, owner) = rpc_over(temp.path(), &base_url).await;
    // A connected row with no credential: what "connect, then paste nothing yet" leaves.
    connect_github(&rpc, &owner, None).await;

    let result = rpc
        .test_integration(&owner, "github".to_string())
        .await
        .expect("integrations.test answers");

    assert_eq!(result.status, "unconfigured");
    assert_eq!(result.account, None);
    assert!(result.message.contains("access token"));
    assert!(
        seen.lock().expect("the mock lock is held").is_empty(),
        "an unconfigured integration opens no socket"
    );

    server.abort();
}

#[tokio::test]
async fn the_api_base_prefers_the_environment_then_the_config_then_github() {
    // The environment variable is what a test or a Playwright mock sets, so it wins; a
    // configured base is a GitHub Enterprise install and must win over api.github.com; with
    // neither, the base is GitHub's own. A blank or slash-only value names nothing.
    assert_eq!(
        crate::rpc::github_api_base_for(Some("http://127.0.0.1:9/"), Some("http://elsewhere")),
        "http://127.0.0.1:9"
    );
    assert_eq!(
        crate::rpc::github_api_base_for(Some("   "), Some("http://elsewhere/")),
        "http://elsewhere"
    );
    assert_eq!(
        crate::rpc::github_api_base_for(None, Some("https://ghe.internal/api/v3")),
        "https://ghe.internal/api/v3"
    );
    assert_eq!(
        crate::rpc::github_api_base_for(None, None),
        "https://api.github.com"
    );

    // And prove the daemon reads `WARSHA_GITHUB_API_BASE` end-to-end when `DaemonConfig`
    // leaves `github_api_base` unset, serialised behind `ENV_LOCK` so no concurrent test
    // sees the override.
    let _env_lock = crate::test_util::ENV_LOCK.lock().await;
    let temp = crate::test_util::fresh_tempdir();
    let (base_url, seen, server) = mock_github(200, serde_json::json!({"login": "octo-env"})).await;
    let _guard = crate::test_util::EnvVarGuard::set(crate::rpc::GITHUB_API_BASE_ENV, &base_url);
    let (state, _store) = test_state(temp.path()).await;
    let rpc = WarshaRpc::new(state);
    let owner = owner_ext();
    connect_github(&rpc, &owner, Some(MOCK_TOKEN)).await;

    let result = rpc
        .test_integration(&owner, "github".to_string())
        .await
        .expect("integrations.test answers");
    assert_eq!(result.status, "ok");
    assert_eq!(result.account.as_deref(), Some("octo-env"));
    assert_eq!(seen.lock().expect("the mock lock is held").len(), 1);

    server.abort();
}

/// A primary store whose every call fails, which is what a locked keychain is to the fallback.
#[derive(Debug)]
struct LockedKeychain;

#[async_trait::async_trait]
impl SecretStore for LockedKeychain {
    async fn store(&self, _account: &str, _key: &str) -> warsha_providers::Result<()> {
        Err(warsha_providers::ProviderError::Auth(
            "the keychain is locked".to_string(),
        ))
    }

    async fn load(&self, _account: &str) -> warsha_providers::Result<String> {
        Err(warsha_providers::ProviderError::Auth(
            "the keychain is locked".to_string(),
        ))
    }

    async fn delete(&self, _account: &str) -> warsha_providers::Result<()> {
        Err(warsha_providers::ProviderError::Auth(
            "the keychain is locked".to_string(),
        ))
    }
}

#[tokio::test]
async fn a_token_survives_a_locked_keychain_and_the_same_reader_finds_it() {
    let _env_lock = crate::test_util::ENV_LOCK.lock().await;
    let temp = crate::test_util::fresh_tempdir();
    let (base_url, seen, server) =
        mock_github(200, serde_json::json!({"login": "octo-fallback"})).await;

    // The production wiring minus a working keychain: keychain primary, encrypted file behind
    // it (crates/warshad/src/startup.rs builds exactly this pair at start).
    let fallback = EncryptedFileSecretStore::new(temp.path()).expect("the encrypted file store");
    let map_file = fallback.file_path().to_path_buf();
    let secrets: Arc<dyn SecretStore> = Arc::new(FallbackSecretStore::new(
        Arc::new(LockedKeychain),
        Arc::new(fallback),
    ));
    let (state, _store) = test_state_with_seams(
        temp.path(),
        |cfg| cfg.github_api_base = Some(base_url),
        None,
        Some(secrets),
        None,
    )
    .await;
    let rpc = WarshaRpc::new(state);
    let owner = owner_ext();

    // integrations.connect writes to the store it was handed; the keychain refuses every call,
    // so the fallback takes it.
    connect_github(&rpc, &owner, Some(MOCK_TOKEN)).await;
    let filed = std::fs::read(&map_file).expect("the encrypted map file exists");
    assert!(
        !String::from_utf8_lossy(&filed).contains(MOCK_TOKEN),
        "the token is not on disk in the clear"
    );

    // The one reader finds it, and the live check uses it.
    let result = rpc
        .test_integration(&owner, "github".to_string())
        .await
        .expect("integrations.test answers");
    assert_eq!(result.status, "ok");
    assert_eq!(result.account.as_deref(), Some("octo-fallback"));
    assert_eq!(
        seen.lock().expect("the mock lock is held").as_slice(),
        [format!("Bearer {MOCK_TOKEN}")]
    );

    // And it is the same token the project RPCs see.
    let status = rpc
        .project_github_status(&owner)
        .await
        .expect("projects.github_status answers");
    assert!(status.configured);
    assert_eq!(status.source.as_deref(), Some("integrations"));

    server.abort();
}
