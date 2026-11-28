//! Daemon helper items split out of `lib.rs` (ENGINE H1 step 2); moved verbatim.

use super::*;

/// In-memory pending budget store for projects before any run has been created (N2 fix).
pub(crate) static PENDING_PROJECT_BUDGETS: std::sync::LazyLock<
    std::sync::RwLock<HashMap<ProjectId, u64>>,
> = std::sync::LazyLock::new(|| std::sync::RwLock::new(HashMap::new()));

/// Best-effort deletion of a run branch using `run.worktree_branch` as a hint
/// when porcelain lookup yields None or worktree metadata was missing.
/// Guarded to only delete branches starting with `warsha/run-` (prefix guard).
pub(crate) async fn delete_branch_hint(
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
pub(crate) async fn apply_start_budget(
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

/// The store's answer to "is this run still live", for the preview manager.
///
/// `warsha-pipeline` owns the preview entries and knows nothing about run records,
/// so the daemon hands it this oracle (R-BRIDGE-B5 D3). A run the store still has
/// and that has not finished is live, and its preview is not another run's to take.
/// A run the store no longer has, an id that is not a run id, or a store read that
/// fails is *not* live: a preview entry whose owner cannot be proved alive is
/// swept, which is what keeps a new run on the project from inheriting a server a
/// dead run left behind.
pub(crate) struct StoreRunLiveness {
    pub(crate) store: Arc<Store>,
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
pub(crate) async fn check_secret_account(
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
pub(crate) async fn provider_has_stored_key(
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
