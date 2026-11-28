//! The `WarshaApiServer` implementation for [`WarshaRpc`], split out of `lib.rs` (ENGINE H1).
//! Moved verbatim; the crate root items it uses arrive through `use super::*`.

use super::*;

#[async_trait]
impl WarshaApiServer for WarshaRpc {
    async fn health(&self, ext: &RpcExtensions) -> RpcResult<HealthResponse> {
        self.health_impl(ext).await
    }

    async fn list_projects(&self, ext: &RpcExtensions) -> RpcResult<Vec<Project>> {
        self.list_projects_impl(ext).await
    }

    #[allow(clippy::too_many_arguments)]
    async fn create_project(
        &self,
        ext: &RpcExtensions,
        name: String,
        description: String,
        language: String,
        confinement: Option<Confinement>,
        approval_mode: Option<ApprovalMode>,
        repo_path: Option<String>,
        repo_url: Option<String>,
    ) -> RpcResult<Project> {
        self.create_project_impl(
            ext,
            name,
            description,
            language,
            confinement,
            approval_mode,
            repo_path,
            repo_url,
        )
        .await
    }

    async fn project_github_status(&self, ext: &RpcExtensions) -> RpcResult<GithubStatus> {
        self.project_github_status_impl(ext).await
    }

    async fn prepare_project_folder(
        &self,
        ext: &RpcExtensions,
        parent_dir: String,
        name: String,
    ) -> RpcResult<PreparedFolder> {
        self.prepare_project_folder_impl(ext, parent_dir, name)
            .await
    }

    async fn prepare_existing_folder(
        &self,
        ext: &RpcExtensions,
        path: String,
    ) -> RpcResult<PreparedFolder> {
        self.prepare_existing_folder_impl(ext, path).await
    }

    async fn project_repo_status(
        &self,
        ext: &RpcExtensions,
        path: Option<String>,
        parent_dir: Option<String>,
        name: Option<String>,
    ) -> RpcResult<RepoStatus> {
        self.project_repo_status_impl(ext, path, parent_dir, name)
            .await
    }

    async fn clone_project_repo(
        &self,
        ext: &RpcExtensions,
        repo_url: String,
        parent_dir: Option<String>,
        folder_name: Option<String>,
    ) -> RpcResult<ClonedRepo> {
        self.clone_project_repo_impl(ext, repo_url, parent_dir, folder_name)
            .await
    }

    async fn create_github_repo(
        &self,
        ext: &RpcExtensions,
        name: String,
        description: Option<String>,
        parent_dir: Option<String>,
        private_repo: Option<bool>,
    ) -> RpcResult<CreatedGithubRepo> {
        self.create_github_repo_impl(ext, name, description, parent_dir, private_repo)
            .await
    }

    async fn get_default_permissions(&self, ext: &RpcExtensions) -> RpcResult<ProjectPermissions> {
        self.get_default_permissions_impl(ext).await
    }

    async fn set_default_permissions(
        &self,
        ext: &RpcExtensions,
        confinement: Confinement,
        approval_mode: ApprovalMode,
    ) -> RpcResult<ProjectPermissions> {
        self.set_default_permissions_impl(ext, confinement, approval_mode)
            .await
    }

    async fn get_project_permissions(
        &self,
        ext: &RpcExtensions,
        project_id: ProjectId,
    ) -> RpcResult<ProjectPermissions> {
        self.get_project_permissions_impl(ext, project_id).await
    }

    async fn set_project_permissions(
        &self,
        ext: &RpcExtensions,
        project_id: ProjectId,
        confinement: Option<Confinement>,
        approval_mode: Option<ApprovalMode>,
    ) -> RpcResult<ProjectPermissions> {
        self.set_project_permissions_impl(ext, project_id, confinement, approval_mode)
            .await
    }

    async fn get_swarm_settings(
        &self,
        ext: &RpcExtensions,
        project_id: ProjectId,
    ) -> RpcResult<warsha_store::StoredProjectSwarmSettings> {
        self.get_swarm_settings_impl(ext, project_id).await
    }

    async fn set_swarm_settings(
        &self,
        ext: &RpcExtensions,
        project_id: ProjectId,
        enable_swarm: bool,
        max_parallel_agents: Option<u32>,
    ) -> RpcResult<warsha_store::StoredProjectSwarmSettings> {
        self.set_swarm_settings_impl(ext, project_id, enable_swarm, max_parallel_agents)
            .await
    }

    async fn get_project(&self, ext: &RpcExtensions, id: ProjectId) -> RpcResult<Project> {
        self.get_project_impl(ext, id).await
    }

    /// Attach an existing repository to a project (ENGINE E3).
    ///
    /// The path is validated with the same probe `create_run_worktree` runs before it
    /// asks git for a checkout, so `projects.attach` cannot record a path that
    /// `runs.start` would then fail (and cancel a run) on.
    ///
    /// A project that already names a repository is refused rather than overwritten:
    /// re-attaching is a decision the owner makes on purpose, and a silent replacement
    /// would move every future run of the project onto a different checkout while the
    /// runs already started keep their old worktrees — the half-attach the E3 packet
    /// forbids. Detaching first is the explicit route to a different repository.
    async fn attach_project(
        &self,
        ext: &RpcExtensions,
        project_id: ProjectId,
        repo_path: String,
    ) -> RpcResult<Project> {
        self.attach_project_impl(ext, project_id, repo_path).await
    }

    async fn estimate_run(
        &self,
        ext: &RpcExtensions,
        project_id: ProjectId,
        idea: String,
    ) -> RpcResult<CostEstimate> {
        self.estimate_run_impl(ext, project_id, idea).await
    }

    async fn interview_start(
        &self,
        ext: &RpcExtensions,
        project_id: ProjectId,
        seed_idea: Option<String>,
    ) -> RpcResult<crate::secretary::TurnOutcome> {
        self.interview_start_impl(ext, project_id, seed_idea).await
    }

    async fn interview_turn(
        &self,
        ext: &RpcExtensions,
        project_id: ProjectId,
        reply: String,
    ) -> RpcResult<crate::secretary::TurnOutcome> {
        self.interview_turn_impl(ext, project_id, reply).await
    }

    async fn interview_status(
        &self,
        ext: &RpcExtensions,
        project_id: ProjectId,
    ) -> RpcResult<Option<crate::secretary::InterviewState>> {
        self.interview_status_impl(ext, project_id).await
    }

    async fn secretary_thread_list(
        &self,
        ext: &RpcExtensions,
        project_id: Option<ProjectId>,
    ) -> RpcResult<Vec<SecretaryConversation>> {
        self.secretary_thread_list_impl(ext, project_id).await
    }

    async fn secretary_thread_turns(
        &self,
        ext: &RpcExtensions,
        conversation_id: Ulid,
        project_id: Option<ProjectId>,
    ) -> RpcResult<Vec<SecretaryTurn>> {
        self.secretary_thread_turns_impl(ext, conversation_id, project_id)
            .await
    }

    async fn secretary_chat(
        &self,
        ext: &RpcExtensions,
        project_id: Option<ProjectId>,
        conversation_id: Option<Ulid>,
        message: String,
        language: Option<String>,
    ) -> RpcResult<crate::secretary_chat::ChatTurnReply> {
        self.secretary_chat_impl(ext, project_id, conversation_id, message, language)
            .await
    }

    async fn secretary_provisioning_list(
        &self,
        ext: &RpcExtensions,
        project_id: ProjectId,
    ) -> RpcResult<Vec<ProvisioningRecord>> {
        self.secretary_provisioning_list_impl(ext, project_id).await
    }

    async fn start_run(
        &self,
        ext: &RpcExtensions,
        project_id: ProjectId,
        idea: String,
        run_mode: Option<String>,
        spec_file_path: Option<String>,
        budget_cents: Option<u64>,
    ) -> RpcResult<Run> {
        self.start_run_impl(
            ext,
            project_id,
            idea,
            run_mode,
            spec_file_path,
            budget_cents,
        )
        .await
    }

    async fn run_worktree(
        &self,
        ext: &RpcExtensions,
        run_id: RunId,
    ) -> RpcResult<Option<RunWorktree>> {
        self.run_worktree_impl(ext, run_id).await
    }

    /// Collect the diff, changed paths and one measured test run for Gate C.
    async fn gate_evidence(
        &self,
        ext: &RpcExtensions,
        run_id: RunId,
        program: String,
        args: Vec<String>,
    ) -> RpcResult<warsha_pipeline::GateEvidence> {
        self.gate_evidence_impl(ext, run_id, program, args).await
    }

    async fn get_run(&self, ext: &RpcExtensions, run_id: RunId) -> RpcResult<Run> {
        self.get_run_impl(ext, run_id).await
    }

    async fn list_runs(&self, ext: &RpcExtensions, project_id: ProjectId) -> RpcResult<Vec<Run>> {
        self.list_runs_impl(ext, project_id).await
    }

    async fn get_events(&self, ext: &RpcExtensions, run_id: RunId) -> RpcResult<Vec<Event>> {
        self.get_events_impl(ext, run_id).await
    }

    async fn list_sessions(&self, ext: &RpcExtensions, run_id: RunId) -> RpcResult<Vec<Session>> {
        self.list_sessions_impl(ext, run_id).await
    }

    async fn list_run_subtasks(
        &self,
        ext: &RpcExtensions,
        run_id: RunId,
    ) -> RpcResult<Vec<warsha_core::RunSubtask>> {
        self.list_run_subtasks_impl(ext, run_id).await
    }

    async fn ask_start(
        &self,
        ext: &RpcExtensions,
        project_id: ProjectId,
        _idea: String,
    ) -> RpcResult<warsha_core::SessionId> {
        self.ask_start_impl(ext, project_id, _idea).await
    }

    async fn ask_turn(
        &self,
        ext: &RpcExtensions,
        project_id: ProjectId,
        session_id: String,
        reply: String,
    ) -> RpcResult<crate::secretary::TurnOutcome> {
        self.ask_turn_impl(ext, project_id, session_id, reply).await
    }

    async fn ask_status(
        &self,
        ext: &RpcExtensions,
        _project_id: ProjectId,
        session_id: String,
    ) -> RpcResult<crate::secretary::TurnOutcome> {
        self.ask_status_impl(ext, _project_id, session_id).await
    }

    async fn session_tail(
        &self,
        ext: &RpcExtensions,
        session_id: SessionId,
    ) -> RpcResult<Vec<String>> {
        self.session_tail_impl(ext, session_id).await
    }

    async fn session_stop(&self, ext: &RpcExtensions, session_id: SessionId) -> RpcResult<()> {
        self.session_stop_impl(ext, session_id).await
    }

    async fn approve_gate(
        &self,
        ext: &RpcExtensions,
        run_id: RunId,
        stage: Stage,
        device_id: Option<DeviceId>,
    ) -> RpcResult<Run> {
        self.approve_gate_impl(ext, run_id, stage, device_id).await
    }

    async fn reject_gate(
        &self,
        ext: &RpcExtensions,
        run_id: RunId,
        stage: Stage,
        reason: String,
        device_id: Option<DeviceId>,
    ) -> RpcResult<Run> {
        self.reject_gate_impl(ext, run_id, stage, reason, device_id)
            .await
    }

    async fn request_changes(
        &self,
        ext: &RpcExtensions,
        run_id: RunId,
        stage: Stage,
        notes: String,
        device_id: Option<DeviceId>,
    ) -> RpcResult<Run> {
        self.request_changes_impl(ext, run_id, stage, notes, device_id)
            .await
    }

    async fn list_gates(
        &self,
        ext: &RpcExtensions,
        run_id: RunId,
        device_id: Option<DeviceId>,
    ) -> RpcResult<Vec<GateSummary>> {
        self.list_gates_impl(ext, run_id, device_id).await
    }

    async fn list_providers(&self, ext: &RpcExtensions) -> RpcResult<Vec<ProviderRecord>> {
        self.list_providers_impl(ext).await
    }

    async fn provider_auth_status(
        &self,
        ext: &RpcExtensions,
        name: String,
    ) -> RpcResult<AuthStatus> {
        self.provider_auth_status_impl(ext, name).await
    }

    async fn provider_auth_start(
        &self,
        ext: &RpcExtensions,
        name: String,
    ) -> RpcResult<AuthStartResponse> {
        self.provider_auth_start_impl(ext, name).await
    }

    async fn provider_auth_complete(
        &self,
        ext: &RpcExtensions,
        name: String,
        code: String,
    ) -> RpcResult<AuthCompleteResponse> {
        self.provider_auth_complete_impl(ext, name, code).await
    }

    async fn provider_disconnect(&self, ext: &RpcExtensions, name: String) -> RpcResult<()> {
        self.provider_disconnect_impl(ext, name).await
    }

    /// Add a provider: keychain first, then the settings row.
    ///
    /// The order is the point. A keychain that refuses (a locked keyring, a
    /// platform with no secret service) must leave no row behind, because a
    /// configured provider whose key is missing is a provider every
    /// `providers.test` would refuse — recorded as if the owner had finished
    /// setup. Nothing here logs the key, echoes it in an error, or writes it
    /// anywhere but the store it was handed.
    async fn add_provider(
        &self,
        ext: &RpcExtensions,
        name: String,
        kind: String,
        key: String,
        api_base: Option<String>,
    ) -> RpcResult<ProviderRecord> {
        self.add_provider_impl(ext, name, kind, key, api_base).await
    }

    /// Test a stored provider for real: is the input well formed, then does the
    /// endpoint answer and accept the key.
    ///
    /// A refusal is an error rather than `{healthy: false}`, because the desktop
    /// client's `providers.test` treats a resolved call as a passing test — the
    /// wizard could not tell an unhealthy result from a healthy one. The reason
    /// therefore travels in the error's message (what the wizard shows) and in its
    /// `data` as the [`HealthCheck`] the RPC declares.
    async fn test_provider(
        &self,
        ext: &RpcExtensions,
        name: String,
        model: Option<String>,
    ) -> RpcResult<HealthCheck> {
        self.test_provider_impl(ext, name, model).await
    }

    async fn check_provider_models(
        &self,
        ext: &RpcExtensions,
        name: String,
        api_base: Option<String>,
        key: Option<String>,
        kind: Option<String>,
    ) -> RpcResult<warsha_providers::ModelCatalogDiff> {
        self.check_provider_models_impl(ext, name, api_base, key, kind)
            .await
    }

    async fn provider_quota(
        &self,
        ext: &RpcExtensions,
        name: String,
    ) -> RpcResult<ProviderQuotaStatus> {
        self.provider_quota_impl(ext, name).await
    }

    async fn usage_by_dimension(
        &self,
        ext: &RpcExtensions,
        dimension: String,
        project_id: Option<ProjectId>,
    ) -> RpcResult<Vec<UsageRow>> {
        self.usage_by_dimension_impl(ext, dimension, project_id)
            .await
    }

    async fn usage_by_time(
        &self,
        ext: &RpcExtensions,
        granularity: String,
        project_id: Option<ProjectId>,
    ) -> RpcResult<Vec<UsageTimeRow>> {
        self.usage_by_time_impl(ext, granularity, project_id).await
    }

    /// A CLI-managed provider's rolling subscription windows (A3): both are
    /// estimated from the daemon's own cost records — the provider exposes no
    /// limit, so none is invented here.
    async fn usage_subscription_window(
        &self,
        ext: &RpcExtensions,
    ) -> RpcResult<SubscriptionWindow> {
        self.usage_subscription_window_impl(ext).await
    }

    async fn list_devices(
        &self,
        ext: &RpcExtensions,
    ) -> RpcResult<Vec<warsha_remote::DeviceIdentity>> {
        self.list_devices_impl(ext).await
    }

    async fn pair_device(
        &self,
        ext: &RpcExtensions,
        pairing_data: warsha_remote::PairingData,
    ) -> RpcResult<()> {
        self.pair_device_impl(ext, pairing_data).await
    }

    async fn start_pairing(&self, ext: &RpcExtensions) -> RpcResult<warsha_remote::PairingOffer> {
        self.start_pairing_impl(ext).await
    }

    async fn revoke_device(&self, ext: &RpcExtensions, device_id: DeviceId) -> RpcResult<()> {
        self.revoke_device_impl(ext, device_id).await
    }

    // Providers
    async fn remove_provider(
        &self,
        ext: &RpcExtensions,
        name: String,
    ) -> RpcResult<ProviderRecord> {
        self.remove_provider_impl(ext, name).await
    }

    // Roles.
    //
    // The routing an owner sets is the routing a turn follows: `roles.set` hands it to
    // the router *and* to the daemon's settings, so the model the owner chose answers
    // the next call and every call after a restart. Both halves matter — a routing kept
    // only in memory was C2's second gap, and one kept only on disk is a preference
    // nobody honours.
    async fn list_roles(&self, ext: &RpcExtensions) -> RpcResult<Vec<RoleRoutingConfig>> {
        self.list_roles_impl(ext).await
    }

    /// Route one role, or refuse the request and change nothing.
    ///
    /// Three things have to hold before the router is touched (audit H-01, H-07):
    ///
    /// - the role is one the pipeline knows, and every target names both a provider and
    ///   a model — an empty target resolves to nothing at the call, which is a slower
    ///   way of saying no;
    /// - every named provider is one the owner configured (or the daemon's own mock).
    ///   A routing to a name with no row behind it would be a routing that silently
    ///   never spends, and an owner who reads a `200` here is entitled to believe the
    ///   next call reaches that provider;
    /// - the settings write succeeds before the in-memory table is replaced, so a
    ///   refused write leaves the daemon routing exactly what it was routing.
    async fn set_role(&self, ext: &RpcExtensions, config: RoleRoutingConfig) -> RpcResult<()> {
        self.set_role_impl(ext, config).await
    }

    // Budget
    async fn show_budget(
        &self,
        ext: &RpcExtensions,
        project_id: ProjectId,
    ) -> RpcResult<BudgetStatus> {
        self.show_budget_impl(ext, project_id).await
    }

    async fn set_budget(
        &self,
        ext: &RpcExtensions,
        project_id: ProjectId,
        cents: u64,
    ) -> RpcResult<()> {
        self.set_budget_impl(ext, project_id, cents).await
    }

    // Backend kit (PLAN.md M19)
    async fn export_backend(
        &self,
        ext: &RpcExtensions,
        run_id: String,
        output_dir: String,
    ) -> RpcResult<String> {
        self.export_backend_impl(ext, run_id, output_dir).await
    }

    // Continue on server (PLAN.md M6a)
    async fn create_handoff(
        &self,
        ext: &RpcExtensions,
        run_id: String,
        files: Vec<(String, String)>,
    ) -> RpcResult<String> {
        self.create_handoff_impl(ext, run_id, files).await
    }

    async fn apply_handoff(
        &self,
        ext: &RpcExtensions,
        path: String,
        owner_wins: bool,
    ) -> RpcResult<String> {
        self.apply_handoff_impl(ext, path, owner_wins).await
    }

    // Post-launch care (PLAN.md M17)
    async fn list_crash_reports(
        &self,
        ext: &RpcExtensions,
        run_id: String,
    ) -> RpcResult<Vec<CrashReport>> {
        self.list_crash_reports_impl(ext, run_id).await
    }

    async fn list_store_reviews(
        &self,
        ext: &RpcExtensions,
        store: String,
    ) -> RpcResult<Vec<StoreReview>> {
        self.list_store_reviews_impl(ext, store).await
    }

    // Agent workflow (PLAN.md M18, C2: the mode an owner approves is the mode that
    // spends, and both are the conversation the next daemon reads back).
    //
    // The three methods here share one rule from audit H-03: *who approved* is never a
    // value in the params. It is the device the transport authenticated, and a session
    // whose pairing names no device is refused instead of being recorded under a name
    // the daemon invented.
    /// The retired daemon-wide turn: refuse, and name the method that replaced it.
    ///
    /// The per-run loop is the one loop story (ENGINE E2, OD-E2), so this method no
    /// longer runs a turn. It stays on the wire because callers still name it —
    /// `packages/client`'s `agentTurn`, the desktop bench action and the e2e harness
    /// stub all do — and the smallest change the evidence supports is a refusal they
    /// can act on instead of a `-32601` they would read as a wiring bug. `authorize`
    /// runs first and its policy row stays, so the caller who may ask receives the
    /// migration message rather than a policy error.
    async fn agent_turn(&self, ext: &RpcExtensions, _input: String) -> RpcResult<AgentTurnResult> {
        self.agent_turn_impl(ext, _input).await
    }

    /// Run one turn in one run's own loop and workspace.
    ///
    /// This is the one turn a daemon runs (ENGINE E2), so two runs cannot share
    /// context, todos, asks or mode state. The session is created at `runs.start`;
    /// a run that predates it has no session and is refused. `agent.turn`, the one
    /// conversation per daemon, is retired by OD-E2 and refuses.
    async fn agent_turn_run(
        &self,
        ext: &RpcExtensions,
        run_id: RunId,
        input: String,
    ) -> RpcResult<AgentTurnResult> {
        self.agent_turn_run_impl(ext, run_id, input).await
    }

    /// Approve Agent mode for the session that asks.
    ///
    /// Plan mode is what a turn starts in, and the M18 rule is absolute: nothing
    /// edits a file or runs a tool until the owner approves. Until now the only caller
    /// that could approve was in-process, which meant Agent mode — the mode this daemon
    /// routes to a configured provider — was unreachable from the desktop. The approver
    /// recorded is the authenticated device, and a blank one is refused by the mode
    /// manager itself.
    ///
    /// `run_id` is OD-E2b: present, the approval is that run's own loop — the one
    /// `agent.turn_run` executes on, so the plan → approve → execute flow reaches Agent
    /// mode over the wire; absent, this is the daemon-wide record it always was.
    async fn approve_agent_mode(
        &self,
        ext: &RpcExtensions,
        run_id: Option<RunId>,
    ) -> RpcResult<ModeTransition> {
        self.approve_agent_mode_impl(ext, run_id).await
    }

    /// Return the conversation to Plan mode.
    ///
    /// The read-only direction is always allowed, which is why an owner can end Agent
    /// mode without another approval — and why the daemon can be put back to a state
    /// that cannot edit or spend by any session that can reach it. A `run_id` returns
    /// that run's own loop instead, by the same rule.
    async fn return_agent_to_plan(
        &self,
        ext: &RpcExtensions,
        run_id: Option<RunId>,
    ) -> RpcResult<ModeTransition> {
        self.return_agent_to_plan_impl(ext, run_id).await
    }

    /// Report which mode the loop is in, and how it got there.
    ///
    /// A `run_id` answers for that run's own loop — the mode its next `agent.turn_run`
    /// runs in, its approvals and its todos — so the read agrees with a run-scoped
    /// `agent.approve_mode`; without one this is the daemon-wide record, unchanged.
    async fn agent_state(
        &self,
        ext: &RpcExtensions,
        run_id: Option<RunId>,
    ) -> RpcResult<AgentState> {
        self.agent_state_impl(ext, run_id).await
    }

    // Process spawning and MCP transports (PLAN.md M20)
    async fn spawn_process(
        &self,
        ext: &RpcExtensions,
        command: String,
        args: Vec<String>,
        env: HashMap<String, String>,
    ) -> RpcResult<String> {
        self.spawn_process_impl(ext, command, args, env).await
    }

    async fn stop_process(&self, ext: &RpcExtensions, id: String) -> RpcResult<()> {
        self.stop_process_impl(ext, id).await
    }

    async fn process_status(&self, ext: &RpcExtensions, id: String) -> RpcResult<ProcessStatus> {
        self.process_status_impl(ext, id).await
    }

    async fn list_processes(&self, ext: &RpcExtensions) -> RpcResult<Vec<ProcessSummary>> {
        self.list_processes_impl(ext).await
    }

    async fn connect_mcp(
        &self,
        ext: &RpcExtensions,
        config: warsha_pipeline::mcp_transport::McpServerConfig,
    ) -> RpcResult<()> {
        self.connect_mcp_impl(ext, config).await
    }

    async fn disconnect_mcp(&self, ext: &RpcExtensions, name: String) -> RpcResult<()> {
        self.disconnect_mcp_impl(ext, name).await
    }

    async fn list_mcp(&self, ext: &RpcExtensions) -> RpcResult<Vec<McpConnection>> {
        self.list_mcp_impl(ext).await
    }

    async fn send_mcp(
        &self,
        ext: &RpcExtensions,
        name: String,
        payload: String,
    ) -> RpcResult<String> {
        self.send_mcp_impl(ext, name, payload).await
    }

    /// Call one MCP tool on the owner's behalf (`mcp.call`).
    ///
    /// The table is consulted here exactly as the agent loop consults it (KEEL K6,
    /// R-KEEL row 6); without this, `mcp.call` would be the one path that reaches the
    /// transport with no `(server, tool, role)` check at all. The role is
    /// [`MCP_AGENT_ROLE`] and not a distinct `rpc` role on purpose: `agent` is the only
    /// role the daemon checks MCP calls under, so a second string would match none of
    /// the owner's existing rows and put every `mcp.call` at the default `Ask` — one
    /// table split into two rather than a boundary. There is no interactive approval on
    /// this path (the agent loop's ask queue belongs to the agent's turn), so `Ask` is
    /// refused with the grant the owner has to make rather than queued.
    async fn call_mcp(
        &self,
        ext: &RpcExtensions,
        name: String,
        tool: String,
        arguments: serde_json::Value,
    ) -> RpcResult<warsha_pipeline::mcp::McpToolResult> {
        self.call_mcp_impl(ext, name, tool, arguments).await
    }

    async fn list_mcp_permissions(&self, ext: &RpcExtensions) -> RpcResult<McpPermissionsReply> {
        self.list_mcp_permissions_impl(ext).await
    }

    async fn set_mcp_permission(
        &self,
        ext: &RpcExtensions,
        server: String,
        tool: String,
        role: String,
        permission: String,
    ) -> RpcResult<McpPermissionRow> {
        self.set_mcp_permission_impl(ext, server, tool, role, permission)
            .await
    }

    async fn list_mcp_approvals(&self, ext: &RpcExtensions) -> RpcResult<Vec<McpApproval>> {
        self.list_mcp_approvals_impl(ext).await
    }

    async fn allow_once_mcp_approval(
        &self,
        ext: &RpcExtensions,
        id: String,
    ) -> RpcResult<McpApprovalAnswer> {
        self.allow_once_mcp_approval_impl(ext, id).await
    }

    // B2: the approval bridge's two calls. The run-scoped token that carries
    // them authorises nothing else (§3.2: env-only, memory-only, expires with
    // the run), and the asks land in the same queue every other ask uses.
    async fn submit_permission_ask(
        &self,
        ext: &RpcExtensions,
        run_id: String,
        server: String,
        tool: String,
        arguments: serde_json::Value,
        summary: String,
    ) -> RpcResult<PermissionAskAccepted> {
        self.submit_permission_ask_impl(ext, run_id, server, tool, arguments, summary)
            .await
    }
    async fn poll_permission_decision(
        &self,
        ext: &RpcExtensions,
        id: String,
    ) -> RpcResult<PermissionDecision> {
        self.poll_permission_decision_impl(ext, id).await
    }

    // Live preview dev server (PLAN.md M5b + M20)
    //
    // A preview belongs to a project on the wire and to the run that started it in
    // the manager (R-BRIDGE-B5 D1). The wire is frozen (OD-B2), so the run context
    // a call arrives in is read here, from the store: a project's newest unfinished
    // run is the run a server started now belongs to, and the run whose server a
    // status call reports. With no live run the call is the project's, exactly as
    // before the run link existed.
    async fn start_preview(
        &self,
        ext: &RpcExtensions,
        project_id: String,
        command: String,
    ) -> RpcResult<LivePreview> {
        self.start_preview_impl(ext, project_id, command).await
    }

    async fn stop_preview(&self, ext: &RpcExtensions, project_id: String) -> RpcResult<()> {
        self.stop_preview_impl(ext, project_id).await
    }

    async fn preview_status(
        &self,
        ext: &RpcExtensions,
        project_id: String,
    ) -> RpcResult<LivePreview> {
        self.preview_status_impl(ext, project_id).await
    }

    // Settings that survive a restart (PLAN.md M20)
    async fn list_run_modes(&self, ext: &RpcExtensions) -> RpcResult<Vec<RunModeConfig>> {
        self.list_run_modes_impl(ext).await
    }

    async fn set_run_mode(
        &self,
        ext: &RpcExtensions,
        config: RunModeConfig,
    ) -> RpcResult<RunModeConfig> {
        self.set_run_mode_impl(ext, config).await
    }

    async fn get_backend(
        &self,
        ext: &RpcExtensions,
        run_id: String,
    ) -> RpcResult<Option<BackendKit>> {
        self.get_backend_impl(ext, run_id).await
    }

    async fn set_backend(
        &self,
        ext: &RpcExtensions,
        run_id: String,
        kit: BackendKit,
    ) -> RpcResult<()> {
        self.set_backend_impl(ext, run_id, kit).await
    }

    async fn list_preferences(
        &self,
        ext: &RpcExtensions,
        project_id: String,
    ) -> RpcResult<Vec<Preference>> {
        self.list_preferences_impl(ext, project_id).await
    }

    async fn record_preference(
        &self,
        ext: &RpcExtensions,
        signal: PreferenceSignal,
    ) -> RpcResult<Vec<Preference>> {
        self.record_preference_impl(ext, signal).await
    }

    async fn list_pairings(&self, ext: &RpcExtensions) -> RpcResult<Vec<PhonePairing>> {
        self.list_pairings_impl(ext).await
    }

    async fn create_pairing(
        &self,
        ext: &RpcExtensions,
        device_id: String,
        secret: String,
    ) -> RpcResult<PhonePairing> {
        self.create_pairing_impl(ext, device_id, secret).await
    }

    async fn revoke_pairing(
        &self,
        ext: &RpcExtensions,
        device_id: String,
    ) -> RpcResult<PhonePairing> {
        self.revoke_pairing_impl(ext, device_id).await
    }

    async fn list_routines(&self, ext: &RpcExtensions) -> RpcResult<Vec<Routine>> {
        self.list_routines_impl(ext).await
    }

    async fn get_routine(&self, ext: &RpcExtensions, id: String) -> RpcResult<Option<Routine>> {
        self.get_routine_impl(ext, id).await
    }

    async fn run_routine_now(&self, ext: &RpcExtensions, id: String) -> RpcResult<Routine> {
        self.run_routine_now_impl(ext, id).await
    }

    async fn pause_routine(&self, ext: &RpcExtensions, id: String) -> RpcResult<Routine> {
        self.pause_routine_impl(ext, id).await
    }

    async fn resume_routine(&self, ext: &RpcExtensions, id: String) -> RpcResult<Routine> {
        self.resume_routine_impl(ext, id).await
    }

    async fn subscribe_run(
        &self,
        ext: &RpcExtensions,
        run_id: Option<RunId>,
        since_seq: Option<u64>,
    ) -> RpcResult<RunStreamSync> {
        self.subscribe_run_impl(ext, run_id, since_seq).await
    }

    async fn advance_run(&self, ext: &RpcExtensions, run_id: RunId) -> RpcResult<Run> {
        self.advance_run_impl(ext, run_id).await
    }

    async fn cancel_run(
        &self,
        ext: &RpcExtensions,
        run_id: RunId,
        reason: Option<String>,
    ) -> RpcResult<Run> {
        self.cancel_run_impl(ext, run_id, reason).await
    }

    async fn resume_run(&self, ext: &RpcExtensions, run_id: RunId) -> RpcResult<Run> {
        self.resume_run_impl(ext, run_id).await
    }

    async fn followup_run(
        &self,
        ext: &RpcExtensions,
        run_id: RunId,
        prompt: String,
    ) -> RpcResult<Run> {
        self.followup_run_impl(ext, run_id, prompt).await
    }

    async fn list_run_artifacts(
        &self,
        ext: &RpcExtensions,
        run_id: RunId,
    ) -> RpcResult<Vec<Artifact>> {
        self.list_run_artifacts_impl(ext, run_id).await
    }

    // ============================================== Wave 2 (harnesses, skills,
    // plugins, custom models, integrations, files)
    //
    // Six surfaces, one shape of method: `authorize` first, then read or write the
    // [`SettingsStore`] row, then answer with the record the *stored* row decodes to.
    // That last half is the rule the whole batch follows — a method never returns the
    // value the caller handed it if the store would have kept something else, so a
    // reply is a promise a restart can check. Secret material is the exception by
    // design: it is filed in the OS keychain and the row keeps a reference, so a
    // reply carries the reference and never the value.
    //
    // Refusals split the same way everywhere: `-32602` when the caller's document or
    // path is what is wrong, `-32000` when the daemon's own store, keychain or
    // filesystem failed.

    async fn list_harnesses(&self, ext: &RpcExtensions) -> RpcResult<Vec<HarnessDefinition>> {
        self.list_harnesses_impl(ext).await
    }

    async fn get_harness(&self, ext: &RpcExtensions, id: String) -> RpcResult<HarnessDefinition> {
        self.get_harness_impl(ext, id).await
    }

    async fn import_harness(
        &self,
        ext: &RpcExtensions,
        content: String,
        format: Option<String>,
    ) -> RpcResult<HarnessDefinition> {
        self.import_harness_impl(ext, content, format).await
    }

    /// Point a project — or, with no project, the daemon as a whole — at a harness.
    ///
    /// The selection is stored under the project id, and `global` is the key an
    /// unprojected run reads, so a browser that has not picked a project yet can
    /// still set the default its first run will use. Selecting a name that is not
    /// installed is refused here rather than at the first gate: a run that starts
    /// against a harness nobody has would fail in the middle of spending, and the
    /// owner would learn about it from a pipeline rather than from a settings screen.
    async fn select_harness(
        &self,
        ext: &RpcExtensions,
        harness_id: String,
        project_id: Option<String>,
    ) -> RpcResult<()> {
        self.select_harness_impl(ext, harness_id, project_id).await
    }

    /// Remove a user-authored harness.
    ///
    /// A bundled preset is refused: the seed at daemon start would write it back, and
    /// a delete that quietly undoes itself at the next restart is a worse answer than
    /// one that says which rows it cannot remove. The `global` selection is cleared
    /// when it named the harness being removed, because that is the one key this
    /// method can read; per-project selections are left alone (the store has no way to
    /// enumerate them) and `harness.get` is what tells a run that its selection is
    /// gone.
    async fn delete_harness(&self, ext: &RpcExtensions, id: String) -> RpcResult<()> {
        self.delete_harness_impl(ext, id).await
    }

    async fn list_coding_backends(
        &self,
        ext: &RpcExtensions,
    ) -> RpcResult<Vec<CodingBackendSummary>> {
        self.list_coding_backends_impl(ext).await
    }

    async fn test_coding_backend(
        &self,
        ext: &RpcExtensions,
        name: String,
    ) -> RpcResult<CodingBackendDiagnostics> {
        self.test_coding_backend_impl(ext, name).await
    }

    async fn get_backend_routing(
        &self,
        ext: &RpcExtensions,
    ) -> RpcResult<crate::runtime::BackendRoutingsConfig> {
        self.get_backend_routing_impl(ext).await
    }

    async fn set_backend_routing(
        &self,
        ext: &RpcExtensions,
        default_backend: Option<String>,
        role_backends: Option<std::collections::HashMap<String, String>>,
        project_backends: Option<std::collections::HashMap<String, String>>,
    ) -> RpcResult<crate::runtime::BackendRoutingsConfig> {
        self.set_backend_routing_impl(ext, default_backend, role_backends, project_backends)
            .await
    }

    /// The installed skills, in the shape the owner manages them in.
    ///
    /// Every row, enabled or not: a screen that could only see the enabled ones could
    /// not switch one back on. `skill_catalog()` remains the in-process registry the
    /// agent loop runs, built from exactly these rows with the disabled ones dropped.
    async fn list_skills(&self, ext: &RpcExtensions) -> RpcResult<Vec<SkillDefinition>> {
        self.list_skills_impl(ext).await
    }

    async fn import_skill(
        &self,
        ext: &RpcExtensions,
        url_or_content: String,
        name: Option<String>,
    ) -> RpcResult<SkillDefinition> {
        self.import_skill_impl(ext, url_or_content, name).await
    }

    /// Set one skill's flag to the value the caller named.
    ///
    /// The store's `toggle_skill` *flips*, which is what a keyboard shortcut wants and
    /// is not what an `enabled: bool` parameter is: flipping a skill that is already
    /// in the requested state would switch it off. So the row is read, and flipped only
    /// when the flip is the change asked for.
    async fn toggle_skill(
        &self,
        ext: &RpcExtensions,
        id: String,
        enabled: bool,
    ) -> RpcResult<SkillDefinition> {
        self.toggle_skill_impl(ext, id, enabled).await
    }

    async fn delete_skill(&self, ext: &RpcExtensions, id: String) -> RpcResult<()> {
        self.delete_skill_impl(ext, id).await
    }

    async fn list_plugins(&self, ext: &RpcExtensions) -> RpcResult<Vec<PluginDefinition>> {
        self.list_plugins_impl(ext).await
    }

    async fn install_plugin(
        &self,
        ext: &RpcExtensions,
        manifest_json_or_url: String,
    ) -> RpcResult<PluginDefinition> {
        self.install_plugin_impl(ext, manifest_json_or_url).await
    }

    async fn toggle_plugin(
        &self,
        ext: &RpcExtensions,
        id: String,
        enabled: bool,
    ) -> RpcResult<PluginDefinition> {
        self.toggle_plugin_impl(ext, id, enabled).await
    }

    /// Replace a pack's environment map, filing anything secret in the keychain.
    ///
    /// The map the caller sends is the whole map, not a patch: a key that was there
    /// and is not named now is gone from the row, and the keychain entry beside it is
    /// left for the next `configure` of that key to overwrite — a remove of an entry
    /// this method cannot see the owner asking for would be a delete nobody requested.
    async fn configure_plugin(
        &self,
        ext: &RpcExtensions,
        id: String,
        env: HashMap<String, String>,
    ) -> RpcResult<PluginDefinition> {
        self.configure_plugin_impl(ext, id, env).await
    }

    async fn remove_plugin(&self, ext: &RpcExtensions, id: String) -> RpcResult<()> {
        self.remove_plugin_impl(ext, id).await
    }

    async fn list_custom_models(
        &self,
        ext: &RpcExtensions,
        provider: Option<String>,
    ) -> RpcResult<Vec<CustomModelRecord>> {
        self.list_custom_models_impl(ext, provider).await
    }

    /// Register one model by hand under one provider.
    ///
    /// The pair `(provider, model_id)` is the key, so this is an upsert: the second
    /// call with the same pair is the owner correcting a label or a window, not a
    /// second row. The provider does not have to be configured yet — an owner who
    /// knows the model id they want can write it down before the key arrives, and
    /// routing is what fails later, honestly, with its own refusal.
    async fn add_custom_model(
        &self,
        ext: &RpcExtensions,
        provider: String,
        model_id: String,
        display_name: Option<String>,
        context_window: Option<u32>,
    ) -> RpcResult<CustomModelRecord> {
        self.add_custom_model_impl(ext, provider, model_id, display_name, context_window)
            .await
    }

    async fn remove_custom_model(
        &self,
        ext: &RpcExtensions,
        provider: String,
        model_id: String,
    ) -> RpcResult<()> {
        self.remove_custom_model_impl(ext, provider, model_id).await
    }

    async fn list_integrations(&self, ext: &RpcExtensions) -> RpcResult<Vec<IntegrationStatus>> {
        self.list_integrations_impl(ext).await
    }

    /// Connect one external service.
    ///
    /// The token halves of the config ride the write once and are then gone from the
    /// daemon's view of the row: they are filed under `integration:{id}` and
    /// `integration:{id}:bot`, and what the settings database keeps is the non-secret
    /// half. `connected` therefore means "this daemon holds a credential or a webhook
    /// for it", which is the strongest claim a daemon that has not opened a socket can
    /// honestly make.
    async fn connect_integration(
        &self,
        ext: &RpcExtensions,
        id: String,
        config: IntegrationConfig,
    ) -> RpcResult<IntegrationStatus> {
        self.connect_integration_impl(ext, id, config).await
    }

    /// Disconnect one service: the credentials go, the shape stays.
    ///
    /// The workspace, channel and webhook the owner typed are kept so a re-connect is
    /// one token away rather than a form refilled, and `connected` is written false
    /// because the daemon no longer holds anything to connect with.
    async fn disconnect_integration(
        &self,
        ext: &RpcExtensions,
        id: String,
    ) -> RpcResult<IntegrationStatus> {
        self.disconnect_integration_impl(ext, id).await
    }

    /// Report what a daemon that opens no socket can know about an integration.
    ///
    /// This is the one method under the Spend tier, and the reason it does not spend:
    /// a check that reached GitHub or Slack would be the daemon using an owner's quota
    /// on their behalf, and this batch's rule is that no RPC quietly goes to a third
    /// party. So `ok` means "the configuration is complete and filed" and `unconfigured`
    /// names the half that is missing — and neither word claims a connection was made.
    /// A future live probe is a separate decision with its own budget, not a detail of
    /// this method.
    async fn test_integration(
        &self,
        ext: &RpcExtensions,
        id: String,
    ) -> RpcResult<IntegrationTestResult> {
        self.test_integration_impl(ext, id).await
    }

    async fn list_project_files(
        &self,
        ext: &RpcExtensions,
        project_id: String,
        path: Option<String>,
    ) -> RpcResult<Vec<FileEntry>> {
        self.list_project_files_impl(ext, project_id, path).await
    }

    async fn read_project_file(
        &self,
        ext: &RpcExtensions,
        project_id: String,
        path: String,
    ) -> RpcResult<FileContent> {
        self.read_project_file_impl(ext, project_id, path).await
    }

    async fn diff_project_file(
        &self,
        ext: &RpcExtensions,
        project_id: String,
        path: String,
        base_ref: String,
    ) -> RpcResult<FileDiff> {
        self.diff_project_file_impl(ext, project_id, path, base_ref)
            .await
    }

    async fn get_global_rules(&self, ext: &RpcExtensions) -> RpcResult<ToolRulesReply> {
        self.get_global_rules_impl(ext).await
    }

    async fn set_global_rules(
        &self,
        ext: &RpcExtensions,
        rules: ToolRulesReply,
    ) -> RpcResult<ToolRulesReply> {
        self.set_global_rules_impl(ext, rules).await
    }

    async fn get_project_rules(
        &self,
        ext: &RpcExtensions,
        project_id: String,
    ) -> RpcResult<ToolRulesReply> {
        self.get_project_rules_impl(ext, project_id).await
    }

    async fn set_project_rules(
        &self,
        ext: &RpcExtensions,
        project_id: String,
        rules: ToolRulesReply,
    ) -> RpcResult<ToolRulesReply> {
        self.set_project_rules_impl(ext, project_id, rules).await
    }
}
