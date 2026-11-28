//! Warshad — Daemon binary: local API (JSON-RPC over Unix socket / named pipe + localhost HTTP/WebSocket with token)

use tracing_subscriber as _;

use chrono::Utc;
use jsonrpsee::{
    core::{async_trait, RpcResult},
    types::ErrorObject,
};
use std::collections::{BTreeMap, HashMap};
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};
use ulid::Ulid;
use warsha_core::*;
use warsha_pipeline::mcp_permission::{McpPermissionSource, ToolPermission};
#[allow(unused_imports)] // read only by `mod tests`
use warsha_pipeline::AgentLoop;
use warsha_pipeline::MCP_AGENT_ROLE;
use warsha_pipeline::{
    AgentMode, AgentTurnResult, BackendKit, CommandPolicy, CrashReport, CrashReportStore,
    HandoffBundle, HandoffManager, LivePreview, ManagedProcess, McpConnection, ModeTransition,
    PairingStore, PhonePairing, Preference, PreferenceSignal, PreferenceStore, ProcessStatus,
    ProcessSummary, Routine, RunMode, RunModeConfig, RunWorktree, StoreReview, StoreReviewStore,
    AGENT_ROLE,
};
use warsha_providers::{probe_endpoint, HealthCheck, ProviderSetupWizard};
use warsha_store::{
    ProvisioningRecord, SecretaryConversation, SecretaryTurn, Store, StoredProvider,
};

mod auth;
mod events;
mod http;
mod lock;
mod mcp_approvals;
mod mcp_call_events;
/// The Streamable HTTP MCP client (R-EXT-4): the daemon's implementation of
/// `warsha_pipeline::mcp_http::McpHttpClient`, injected into the manager at startup.
mod mcp_http;
mod providers;
/// The Scout's MCP registry client over `reqwest` (R-EXT-3). Public because the
/// packet that adds a consumer opts the daemon in; no RPC reaches it today.
pub mod registry_http;
pub mod secretary;
pub mod secretary_chat;
mod settings;
mod skills;
pub mod staffing;
mod state;
mod wave2;

pub use crate::mcp_approvals::{McpApproval, McpApprovalAnswer};
pub use crate::mcp_approvals::{PermissionAskAccepted, PermissionDecision};
pub use auth::{
    anonymous_session, owner_session, session_extensions, Actor, ActorRole, DaemonError,
    MethodPolicy, Policy, ProcessPolicy, QuotaConfig, RateGovernor, RateLimitConfig, RpcExtensions,
    Sessions, Tier, DEFAULT_ALLOWED_COMMANDS, METHOD_POLICIES,
};
use auth::{
    bearer_token, retry_after_header, subprotocol_token, RpcAuthLayer, EVENTS_PATH,
    EVENTS_SUBPROTOCOL,
};
use events::events_handler;
pub use events::{
    EventSequencer, RunStreamSync, SequencedEventFrame, StreamSync, EVENT_REPLAY_CAPACITY,
};
#[allow(unused_imports)]
pub(crate) use http::{
    enumerate_interfaces, http_app, origin_is_loopback, pairing_origin, resolve_bind_host,
    resolve_listener_bind, tailscale_bind_ip, InterfaceInfo,
};
pub use http::{REMOTE_ASSETS_DIR_ENV, REMOTE_CSP};
use lock::acquire_lock;
pub use mcp_call_events::StoreMcpCallObserver;
pub use providers::{
    adapter_kind, keychain_credentials, AdapterKind, DeferredProvider, KeychainProviderFactory,
    MockProviderFactory, ProviderFactory, RemovedProvider,
};
pub use settings::{SettingsError, SettingsStore};
pub(crate) use state::paired_device_is_active;
pub use state::{
    scrub_event_payload, windows_firewall_command, AppState, DaemonArgs, DaemonConfig,
    DaemonHandle, DaemonState, McpPermissionRow, McpPermissionsReply, RemoteBind,
};

pub type Result<T> = std::result::Result<T, DaemonError>;

/// The maximum duration to wait for an OS keychain operation before timing out.
///
/// Bounding keychain access at startup, health probes, and provider management
/// prevents wedging the daemon on a locked Linux keyring or missing secret service.
pub const KEYCHAIN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

// --- P-H1 A8: `rpc-core` region (`WarshaRpc` + impl, `rpc_*` error helpers, permission
// store) moved to rpc/mod.rs; kept crate-visible here. ---
mod rpc;
pub use rpc::WarshaRpc;
pub(crate) use rpc::{
    now_epoch_secs, provider_refusal, refuse_a_permission_governed_frame, rpc_error,
    rpc_invalid_params, rpc_policy_refused, DaemonPermissionStore,
};
// --- end P-H1 A8 ---

// ============================================================= /end event stream

// --- P-H1 A4: `state` region (DaemonState + MCP permission DTOs) moved to state.rs; facade above. ---

// --- P-H1 A1: `api` region (WarshaApi trait + DTOs) moved to api.rs; kept crate-visible here. ---
pub mod api;
mod api_estimate;
#[allow(unused_imports)] // read only by `mod tests`
pub(crate) use api::provider_model;
pub(crate) use api::{default_base_url_for_kind, non_empty, provider_endpoint};
pub use api::{
    AgentState, AuthCompleteResponse, AuthStartResponse, AuthStatus, BudgetStatus, ClonedRepo,
    CodingBackendDiagnostics, CodingBackendSummary, CostEstimate, CreatedGithubRepo, GateSummary,
    GateSummaryState, GithubStatus, HealthCapabilities, HealthResponse, PreparedFolder,
    ProviderQuotaStatus, ProviderRecord, QuotaState, RepoStatus, RoleRoutingConfig,
    RoutingTargetConfig, SubscriptionWindow, ToolRulesReply, UsageRow, UsageTimeRow,
    WarshaApiServer, WindowUsage,
};
// --- end P-H1 A1 ---

// --- P-H1 A9: `startup` region (config/env/secret-file helpers, seeded stores, the
// `start_daemon` family, `run_daemon`) moved to startup.rs; kept crate-visible here. ---
mod startup;
pub(crate) use startup::default_backend_kit;
#[allow(unused_imports)]
// `write_private_file` is read by `mod tests`, `apply_env_overrides` by a doc link
pub(crate) use startup::{apply_env_overrides, write_private_file};
pub use startup::{
    load_or_create_config, run_daemon, start_daemon, start_daemon_with_provider_factory,
    start_daemon_with_secrets,
};
// --- end P-H1 A9 ---

// --- P-H1 A7: `runtime` region (agent loop + role routing) moved to runtime.rs; kept crate-visible here. ---
mod runtime;
pub(crate) use runtime::{
    apply_role_routings, build_agent_loop, build_agent_loop_in, load_backend_routings,
    register_configured_providers, routing_config, routing_for, seed_agent_snapshot,
    seed_backend_routings, seed_custom_backends, seed_role_routings, store_agent_snapshot,
    store_backend_routings, store_role_routings, AGENT_METER_KEY, AGENT_WORKSPACE_DIR,
    MOCK_PROVIDER_ID,
};
pub use runtime::{AgentLoopSnapshot, BackendRoutingsConfig};
// --- end P-H1 A7 ---

/// In-memory pending budget store for projects before any run has been created (N2 fix).
static PENDING_PROJECT_BUDGETS: std::sync::LazyLock<std::sync::RwLock<HashMap<ProjectId, u64>>> =
    std::sync::LazyLock::new(|| std::sync::RwLock::new(HashMap::new()));

/// Best-effort deletion of a run branch using `run.worktree_branch` as a hint
/// when porcelain lookup yields None or worktree metadata was missing.
/// Guarded to only delete branches starting with `warsha/run-` (prefix guard).
async fn delete_branch_hint(
    repo_path: &std::path::Path,
    branch: &str,
    run_id: &warsha_core::RunId,
) {
    if !branch.starts_with("warsha/run-") {
        tracing::debug!(
            branch = %branch,
            "skipping branch deletion hint: does not match warsha/run- prefix"
        );
        return;
    }
    let branch_check = tokio::process::Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(["branch", "--list", branch])
        .output()
        .await;
    if let Ok(out) = branch_check {
        let stdout = String::from_utf8_lossy(&out.stdout);
        if !stdout.trim().is_empty() {
            let del_out = tokio::process::Command::new("git")
                .arg("-C")
                .arg(repo_path)
                .args(["branch", "-D", branch])
                .output()
                .await;
            if let Ok(d_out) = del_out {
                if !d_out.status.success() {
                    warn!(
                        run_id = %run_id.0,
                        branch = %branch,
                        error = %String::from_utf8_lossy(&d_out.stderr).trim(),
                        "failed to delete branch via fallback hint"
                    );
                }
            }
        }
    }
}

/// Record the budget `runs.start` was given.
///
/// An explicit `budget_cents` is the budget the new run stores, including `0`.
/// It wins over a pending `budget.set` and over the previous run's budget.
/// `None` leaves the pipeline's inherited budget and still applies a pending
/// `budget.set`, which is the path callers that do not name a budget already use.
async fn apply_start_budget(
    state: &DaemonState,
    project_id: ProjectId,
    mut run: Run,
    budget_cents: Option<u64>,
) -> RpcResult<Run> {
    let pending_budget = match PENDING_PROJECT_BUDGETS.write() {
        Ok(mut pending) => pending.remove(&project_id),
        Err(poisoned) => poisoned.into_inner().remove(&project_id),
    };
    let cents = match budget_cents {
        Some(explicit) => explicit,
        None => match pending_budget {
            Some(pending) => pending,
            None => return Ok(run),
        },
    };
    match state.pipeline.set_run_budget(run.id, cents).await {
        Ok(updated) => {
            run = updated;
        }
        Err(e) => {
            warn!(
                run_id = %run.id.0,
                project_id = %project_id.0,
                error = %e,
                "failed to apply start budget to newly created run"
            );
            state.agent_sessions.remove(run.id);
            let removed_worktree = state.run_worktrees.write().await.remove(&run.id);
            if let Some(worktree) = removed_worktree {
                // N13-4: Best-effort cleanup of worktree on budget failure.
                // Do not fail the RPC on cleanup error; warn only.
                if let Ok(project) = state.store.get_project(project_id).await {
                    if let Some(repo_path) = project
                        .repo_path
                        .as_deref()
                        .map(str::trim)
                        .filter(|p| !p.is_empty())
                    {
                        if let Err(e) = warsha_pipeline::remove_run_worktree(
                            std::path::Path::new(repo_path),
                            &worktree,
                        )
                        .await
                        {
                            warn!(
                                run_id = %run.id.0,
                                error = %e,
                                "failed to remove git worktree registration on budget failure"
                            );
                        }
                        if let Some(ref branch) = run.worktree_branch {
                            delete_branch_hint(std::path::Path::new(repo_path), branch, &run.id)
                                .await;
                        }
                    }
                }
                if worktree.path.exists() {
                    if let Err(e) = tokio::fs::remove_dir_all(&worktree.path).await {
                        warn!(
                            run_id = %run.id.0,
                            path = %worktree.path.display(),
                            error = %e,
                            "failed to remove worktree directory on budget failure"
                        );
                    }
                }
            } else if let Some(ref path_str) = run.worktree_path {
                let path = std::path::PathBuf::from(path_str);
                // N13-4 / N13-4-3: If git repo exists, also deregister git worktree
                // and delete branch before falling back to directory removal.
                let mut repo_path_buf = None;
                if let Ok(project) = state.store.get_project(project_id).await {
                    if let Some(repo_path) = project
                        .repo_path
                        .as_deref()
                        .map(str::trim)
                        .filter(|p| !p.is_empty())
                    {
                        repo_path_buf = Some(std::path::PathBuf::from(repo_path));
                        let dummy_worktree = warsha_pipeline::RunWorktree {
                            path: path.clone(),
                            base: String::new(),
                        };
                        if let Err(e) = warsha_pipeline::remove_run_worktree(
                            std::path::Path::new(repo_path),
                            &dummy_worktree,
                        )
                        .await
                        {
                            warn!(
                                run_id = %run.id.0,
                                error = %e,
                                "failed to remove git worktree registration on budget failure (fallback arm)"
                            );
                        }
                    }
                }
                if path.exists() {
                    if let Err(e) = tokio::fs::remove_dir_all(&path).await {
                        warn!(
                            run_id = %run.id.0,
                            path = %path.display(),
                            error = %e,
                            "failed to remove worktree directory on budget failure"
                        );
                    }
                }
                // When porcelain lookup in remove_run_worktree yields None (or worktree was already detached/untracked),
                // use run.worktree_branch as a branch-deletion hint (leak otherwise).
                if let (Some(ref branch), Some(ref repo_path)) =
                    (&run.worktree_branch, &repo_path_buf)
                {
                    delete_branch_hint(repo_path, branch, &run.id).await;
                }
            } else if let Some(ref branch) = run.worktree_branch {
                // When worktree_path is absent but worktree_branch was recorded, clean up branch hint.
                if let Ok(project) = state.store.get_project(project_id).await {
                    if let Some(repo_path) = project
                        .repo_path
                        .as_deref()
                        .map(str::trim)
                        .filter(|p| !p.is_empty())
                    {
                        delete_branch_hint(std::path::Path::new(repo_path), branch, &run.id).await;
                    }
                }
            }
            state.mcp_approvals.purge_run(&run.id.0.to_string());
            state.previews.stop_run(&run.id.0.to_string()).await;

            let fail_reason = format!("failed to apply start budget: {e}");
            if let Err(fail_err) = state.pipeline.fail_run(run.id, Some(fail_reason)).await {
                warn!(
                    run_id = %run.id.0,
                    error = %fail_err,
                    "failed to transition run to failed after budget application failure"
                );
            }

            return Err(rpc_error(format!(
                "failed to apply start budget to newly created run: {e}"
            )));
        }
    }
    Ok(run)
}

mod rpc_impl;

/// The store's answer to "is this run still live", for the preview manager.
///
/// `warsha-pipeline` owns the preview entries and knows nothing about run records,
/// so the daemon hands it this oracle (R-BRIDGE-B5 D3). A run the store still has
/// and that has not finished is live, and its preview is not another run's to take.
/// A run the store no longer has, an id that is not a run id, or a store read that
/// fails is *not* live: a preview entry whose owner cannot be proved alive is
/// swept, which is what keeps a new run on the project from inheriting a server a
/// dead run left behind.
struct StoreRunLiveness {
    store: Arc<Store>,
}

#[async_trait]
impl warsha_pipeline::RunLiveness for StoreRunLiveness {
    async fn is_live(&self, run_id: &str) -> bool {
        let Ok(id) = run_id.parse::<ulid::Ulid>() else {
            return false;
        };
        match self.store.get_run(RunId(id)).await {
            Ok(run) => !matches!(
                run.status,
                RunStatus::Completed | RunStatus::Failed | RunStatus::Cancelled
            ),
            Err(_) => false,
        }
    }
}

/// Check whether `account` holds a non-empty secret in `secrets` (or `fallback`
/// if the primary read fails or times out), bounded by [`KEYCHAIN_TIMEOUT`].
///
/// Returns `Some(true)` if a non-empty secret is found, `Some(false)` if the
/// store answers cleanly that no secret exists, or `None` if a timeout prevents
/// ruling out a stored secret. Never logs any secret value.
async fn check_secret_account(
    secrets: &Arc<dyn warsha_providers::SecretStore>,
    fallback: Option<&Arc<dyn warsha_providers::SecretStore>>,
    account: &str,
) -> Option<bool> {
    match tokio::time::timeout(KEYCHAIN_TIMEOUT, secrets.load(account)).await {
        Ok(Ok(secret)) => Some(!secret.trim().is_empty()),
        Ok(Err(_)) => {
            if let Some(fb) = fallback {
                match tokio::time::timeout(KEYCHAIN_TIMEOUT, fb.load(account)).await {
                    Ok(Ok(secret)) => Some(!secret.trim().is_empty()),
                    Ok(Err(_)) => Some(false),
                    Err(_) => None,
                }
            } else {
                Some(false)
            }
        }
        Err(_) => {
            if let Some(fb) = fallback {
                match tokio::time::timeout(KEYCHAIN_TIMEOUT, fb.load(account)).await {
                    Ok(Ok(secret)) if !secret.trim().is_empty() => Some(true),
                    _ => None,
                }
            } else {
                None
            }
        }
    }
}

/// Determine whether a provider has a stored credential under `&name` or
/// `integration:{name}`, bounded by [`KEYCHAIN_TIMEOUT`].
async fn provider_has_stored_key(
    secrets: &Arc<dyn warsha_providers::SecretStore>,
    name: &str,
) -> Option<bool> {
    let fallback = warsha_providers::fallback_store();
    let api_key_status = check_secret_account(secrets, fallback.as_ref(), name).await;
    if api_key_status == Some(true) {
        return Some(true);
    }
    let integration_account = format!("integration:{name}");
    let integration_status =
        check_secret_account(secrets, fallback.as_ref(), &integration_account).await;
    match (api_key_status, integration_status) {
        (_, Some(true)) | (Some(true), _) => Some(true),
        (Some(false), Some(false)) => Some(false),
        _ => None,
    }
}

/// Directory under the daemon's data directory that holds handoff bundles.
const HANDOFF_DIR: &str = "handoff";

/// The selection key a harness carries when no project was named.
///
/// `harnesses_active` is keyed by a text id and every other caller in this module uses
/// a project's UUID; `global` is the one key that cannot collide with a UUID, so the
/// daemon-wide default and a project's override live in the same table without either
/// pretending to be the other. A project id that a caller *does* pass is used verbatim
/// as its key, which is what `harness.select` from a browser and a run's own project
/// both read.
const ACTIVE_HARNESS_GLOBAL_KEY: &str = "global";

/// The local PocketBase address PLAN.md M19 documents for a generated project.
const LOCAL_POCKETBASE_URL: &str = "http://127.0.0.1:8090";

// `SlashCommandRegistry` has no non-test reader now that the skill-registry helpers
// live in `skills` (P-H1 A2); the test modules below still name it via `use super::*`.
#[cfg(test)]
use warsha_pipeline::SlashCommandRegistry;

// The auth layer's own pieces that only the test modules below name (P-H1 A3):
// production code reaches the rest through the crate-root import above.
#[cfg(test)]
use auth::{origin_allowed, RpcAuthService};
#[cfg(test)]
use axum::extract::ConnectInfo;
#[cfg(test)]
use jsonrpsee::core::BoxError;
#[cfg(test)]
use jsonrpsee::server::{HttpRequest, HttpResponse};

// The skill-registry helpers the test modules below name through `use super::*`
// (P-H1 A2/A8): production code reaches the rest through the crate-root imports above.
#[cfg(test)]
use skills::*;
#[cfg(test)]
use warsha_pipeline::SlashCommand;
#[cfg(test)]
use warsha_store::{StoredHarness, StoredPlugin, StoredSkill};

// The imports the moved `startup` region reached through the crate root and the test
// modules below still name via `use super::*` (P-H1 A9).
#[cfg(test)]
use std::path::Path;
#[cfg(test)]
use warsha_backends::BackendFactory;
#[cfg(test)]
use warsha_core::{RoleRouting, RoutingTarget};
#[cfg(test)]
use warsha_pipeline::{
    BackendKind, InMemoryCrashReportStore, InMemoryMcpManager, InMemoryPairingStore,
    InMemoryPreferenceStore, InMemoryProcessManager, InMemoryStoreReviewStore, LivePreviewManager,
    McpApprovals, McpManager, ProcessManager, RoutineRegistry, RoutineRunner, RoutineScheduler,
    RunAgentSessions, StaticPermissions,
};
#[cfg(test)]
use warsha_providers::{ProviderRouter, SecretStore};

// The unit-test modules. They are declared last so the first `#[cfg(test)]` in this
// file still comes after every production line:
// `tests::the_startup_listener_lines_are_written_once` scans `lib.rs` up to its first
// `#[cfg(test)]` as the production part of the crate. P-H1 A10 moved them out of this
// file into `tests/`, one file per module (graph map §6.1); the module tree, the module
// names and the test names are unchanged, and each file holds the moved text verbatim.
// Every module but `tests` itself is declared here rather than inside `tests/mod.rs`,
// so its `use super::*` still means the crate root and its tests keep their names.
/// ENGINE E1 wiring: the language a project is created in reaches the prompt its
/// run's stages render — a project created as `ar` through the daemon RPC speaks
/// Arabic to the provider, instead of the English the renderer used to hardcode.
#[cfg(test)]
#[path = "tests/e1_language_wiring.rs"]
mod e1_language_wiring;
/// ENGINE E2 (OD-E2): the legacy daemon-wide `agent.turn` is retired. It refuses
/// with a migration error naming the per-run method, its policy row still gates the
/// refusal, and `agent.turn_run` still runs a turn end to end.
#[cfg(test)]
#[path = "tests/e2_legacy_retired.rs"]
mod e2_legacy_retired;
/// ENGINE E2 wiring: `agent.turn_run` runs a turn in the run's own loop, created
/// at `runs.start`; an unknown run is refused instead of borrowing the legacy loop.
#[cfg(test)]
#[path = "tests/e2_wiring.rs"]
mod e2_wiring;
/// ENGINE E3 wiring: `runs.start` gives a repository-backed project a per-run
/// worktree, and an unusable repository cancels the run instead of letting it
/// execute in the shared workspace.
#[cfg(test)]
#[path = "tests/e3_wiring.rs"]
mod e3_wiring;
/// ENGINE E5 wiring: `gates.evidence` returns Gate C evidence for a
/// repository-backed run, with the test program judged by the command policy.
#[cfg(test)]
#[path = "tests/e5_wiring.rs"]
mod e5_wiring;
/// ENGINE E8 (pilot): the whole loop once, end to end, on the daemon's own mock
/// provider — attach a repository, start a run, let the run's own agent loop edit
/// its worktree, walk the pipeline to Gate C, take the decision, and collect the
/// evidence a reviewer reads.
///
/// It is `#[ignore]`d because it spawns two child processes (the Gate C test
/// program and the preview server), which a plain unit-test run should not: it runs
/// with `cargo test -p warshad -- --ignored pilot`. No key, no secret and no
/// network to a real model API is involved: every model call is answered by
/// `MockProvider`, and the two child processes are this test binary itself.
#[cfg(test)]
#[path = "tests/e8_wiring.rs"]
mod e8_wiring;
#[cfg(test)]
#[path = "tests/greenfield_autoinit.rs"]
mod greenfield_autoinit;
#[cfg(test)]
#[path = "tests/harness_opencode.rs"]
mod harness_opencode;
#[cfg(test)]
#[path = "tests/health_reports_probed_capabilities.rs"]
mod health_reports_probed_capabilities;
/// The RPC listener refuses a present-but-unlisted Origin — including the WebSocket
/// upgrade a CORS layer cannot gate — while an absent Origin and the app origins
/// still reach the auth check (security audit LOW: no Origin check on the RPC WS).
#[cfg(test)]
#[path = "tests/origin_guard.rs"]
mod origin_guard;
/// PAIR repair gates: Owner-only `pairing.list`, live WebSocket authority and the
/// event stream after revocation (`packets/PAIR/brief.md`).
#[cfg(test)]
#[path = "tests/pair_repair.rs"]
mod pair_repair;
#[cfg(test)]
#[path = "tests/secretary_chat.rs"]
mod secretary_chat_tests;
#[cfg(test)]
pub(crate) mod test_util;
#[cfg(test)]
#[allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]
mod tests;
