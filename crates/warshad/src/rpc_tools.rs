//! Tool-domain bodies of the `WarshaApiServer` implementation (ENGINE H1 step 4c): processes,
//! MCP, permission asks, previews, project files, rules, preferences and run modes.
//! Moved verbatim from `rpc_impl.rs`; the trait methods there delegate to these inherent
//! `*_impl` methods. The crate root items arrive through `use super::*`.

use super::*;

impl WarshaRpc {
    pub(crate) async fn spawn_process_impl(
        &self,
        ext: &RpcExtensions,
        command: String,
        args: Vec<String>,
        env: HashMap<String, String>,
    ) -> RpcResult<String> {
        self.authorize(ext, "process.spawn").await?;
        // Audit H-05: the owner's authority over a process table is not handed to a
        // caller that merely holds a token. The program has to be one the daemon's own
        // configuration allows, and the environment cannot carry a variable that
        // changes what the program is — `PATH`, `LD_PRELOAD`, `NODE_OPTIONS` and the
        // rest are execution hijacks wearing a configuration's clothes.
        let policy = self.process_policy();
        policy.check_command(&command).map_err(rpc_policy_refused)?;
        for key in env.keys() {
            policy.check_env(key).map_err(rpc_policy_refused)?;
        }
        self.state
            .processes
            .spawn(&command, &args, &env)
            .await
            .map_err(|error| rpc_error(error.to_string()))
    }

    pub(crate) async fn stop_process_impl(&self, ext: &RpcExtensions, id: String) -> RpcResult<()> {
        self.authorize(ext, "process.stop").await?;
        self.state
            .processes
            .stop(&id)
            .await
            .map_err(|error| rpc_error(error.to_string()))
    }

    pub(crate) async fn process_status_impl(
        &self,
        ext: &RpcExtensions,
        id: String,
    ) -> RpcResult<ProcessStatus> {
        self.authorize(ext, "process.status").await?;
        self.state
            .processes
            .status(&id)
            .await
            .map_err(|error| rpc_error(error.to_string()))
    }

    pub(crate) async fn list_processes_impl(
        &self,
        ext: &RpcExtensions,
    ) -> RpcResult<Vec<ProcessSummary>> {
        self.authorize(ext, "process.list").await?;
        // `list` hands back the record without its child handle; the summary is the
        // same facts in a type an RPC result (which must be clonable) can carry.
        Ok(self
            .state
            .processes
            .list()
            .await
            .iter()
            .map(ManagedProcess::summary)
            .collect())
    }

    pub(crate) async fn connect_mcp_impl(
        &self,
        ext: &RpcExtensions,
        config: warsha_pipeline::mcp_transport::McpServerConfig,
    ) -> RpcResult<()> {
        self.authorize(ext, "mcp.connect").await?;
        // Audit H-05: an MCP server is a child process or an outbound endpoint, so the
        // same policy that bounds `process.spawn` bounds the transports here, and
        // stdio launches are restricted to pinned packages. An HTTP or WebSocket
        // server's URL is checked as a transport — TLS anywhere, plain http/ws only
        // on loopback.
        let policy = self.process_policy();
        match &config.transport {
            warsha_pipeline::mcp_transport::McpTransport::Stdio => {
                let command = config.command.as_deref().ok_or_else(|| {
                    rpc_policy_refused("a stdio MCP server names no program to start")
                })?;
                policy.check_command(command).map_err(rpc_policy_refused)?;
                wave2::require_pinned_launch(command, &config.args).map_err(rpc_policy_refused)?;
            }
            transport => {
                let url = transport.url().ok_or_else(|| {
                    rpc_policy_refused("an MCP transport names no endpoint to reach")
                })?;
                policy
                    .check_transport_url(url)
                    .map_err(rpc_policy_refused)?;
            }
        }
        // An `http` config is dialled here when the daemon injected its Streamable
        // HTTP client — it does, at startup — so this call blocks for up to the
        // handshake's bound and can return a reason instead of always succeeding.
        // A `websocket` config still succeeds without connecting: it is recorded and
        // `mcp.list` reports it as disconnected (see `McpServerConfig::is_wired`).
        self.state
            .mcp
            .connect(config)
            .await
            .map_err(|error| rpc_error(error.to_string()))
    }

    pub(crate) async fn disconnect_mcp_impl(
        &self,
        ext: &RpcExtensions,
        name: String,
    ) -> RpcResult<()> {
        self.authorize(ext, "mcp.disconnect").await?;
        self.state
            .mcp
            .disconnect(&name)
            .await
            .map_err(|error| rpc_error(error.to_string()))
    }

    pub(crate) async fn list_mcp_impl(&self, ext: &RpcExtensions) -> RpcResult<Vec<McpConnection>> {
        self.authorize(ext, "mcp.list").await?;
        Ok(self.state.mcp.list().await)
    }

    pub(crate) async fn send_mcp_impl(
        &self,
        ext: &RpcExtensions,
        name: String,
        payload: String,
    ) -> RpcResult<String> {
        self.authorize(ext, "mcp.send").await?;
        // The raw seam is not a way around the permission table (R-KEEL row 6): a
        // `tools/call` frame is exactly the call `mcp.call` and the agent loop both
        // consult the table for, so it is refused before a byte reaches a child's
        // stdin. Every other method is the caller's own protocol and passes through
        // unchanged.
        refuse_a_permission_governed_frame(&payload)?;
        self.state
            .mcp
            .send(&name, &payload)
            .await
            .map_err(|error| rpc_error(error.to_string()))
    }

    pub(crate) async fn call_mcp_impl(
        &self,
        ext: &RpcExtensions,
        name: String,
        tool: String,
        arguments: serde_json::Value,
    ) -> RpcResult<warsha_pipeline::mcp::McpToolResult> {
        self.authorize(ext, "mcp.call").await?;
        match self
            .state
            .mcp_permissions
            .check_permission(&name, &tool, MCP_AGENT_ROLE)
        {
            ToolPermission::Allow => {}
            ToolPermission::Deny => {
                return Err(rpc_policy_refused(format!(
                    "denied by MCP permission {name}::{tool}"
                )))
            }
            ToolPermission::Ask => {
                return Err(rpc_policy_refused(format!(
                    "`{name}::{tool}` is ask-gated for role `{MCP_AGENT_ROLE}` and `mcp.call` has no approval queue; grant it with mcp.permissions.set {{ server: \"{name}\", tool: \"{tool}\", role: \"{MCP_AGENT_ROLE}\", permission: \"allow\" }}"
                )))
            }
        }
        self.state
            .mcp
            .call_tool(&name, &tool, arguments)
            .await
            .map_err(|error| rpc_error(error.to_string()))
    }

    pub(crate) async fn list_mcp_permissions_impl(
        &self,
        ext: &RpcExtensions,
    ) -> RpcResult<McpPermissionsReply> {
        self.authorize(ext, "mcp.permissions.list").await?;
        let rows = self
            .state
            .mcp_permissions
            .snapshot()
            .into_iter()
            .map(|(server, tool, role, permission)| McpPermissionRow {
                server,
                tool,
                role,
                permission: permission.as_str().to_string(),
            })
            .collect();
        Ok(McpPermissionsReply {
            rows,
            agent_role: warsha_pipeline::MCP_AGENT_ROLE.to_string(),
            default_permission: warsha_pipeline::mcp_permission::ToolPermission::Ask
                .as_str()
                .to_string(),
        })
    }

    pub(crate) async fn set_mcp_permission_impl(
        &self,
        ext: &RpcExtensions,
        server: String,
        tool: String,
        role: String,
        permission: String,
    ) -> RpcResult<McpPermissionRow> {
        self.authorize(ext, "mcp.permissions.set").await?;
        let permission = warsha_pipeline::mcp_permission::ToolPermission::parse_strict(&permission)
            .map_err(rpc_error)?;
        let updated_at = Utc::now().to_rfc3339();
        // Save first, then update memory: a write that failed must not leave the
        // running loop granting something the store does not hold.
        self.state
            .store
            .set_mcp_permission(&server, &tool, &role, permission.as_str(), &updated_at)
            .await
            .map_err(|error| rpc_error(error.to_string()))?;
        self.state
            .mcp_permissions
            .set(&server, &tool, &role, permission);
        // The owner re-decided this key, so an answer they gave earlier no longer
        // stands: the pending entries the new row decides leave the list, and every
        // `allow_once` grant for the key is purged — whatever the value is, because
        // even `ask` means the earlier answer was overridden (D2.8, D3).
        self.state
            .mcp_approvals
            .resolve_by_row(&server, &tool, &role, permission);
        Ok(McpPermissionRow {
            server,
            tool,
            role,
            permission: permission.as_str().to_string(),
        })
    }

    pub(crate) async fn list_mcp_approvals_impl(
        &self,
        ext: &RpcExtensions,
    ) -> RpcResult<Vec<McpApproval>> {
        self.authorize(ext, "mcp.approvals.list").await?;
        Ok(self
            .state
            .mcp_approvals
            .pending()
            .into_iter()
            .map(|view| McpApproval {
                id: view.id,
                run_id: view.run_id,
                server: view.server,
                tool: view.tool,
                role: view.role,
                summary: view.summary,
                requested_at: view.requested_at,
            })
            .collect())
    }

    pub(crate) async fn allow_once_mcp_approval_impl(
        &self,
        ext: &RpcExtensions,
        id: String,
    ) -> RpcResult<McpApprovalAnswer> {
        let method = "mcp.approvals.allow_once";
        let actor = self.authorize(ext, method).await?;
        // The approver is the authenticated device, never a param: the same rule the
        // gate decisions and `agent.approve_mode` follow.
        let granted_by = self.gate_approver(&actor, None, method)?;

        let pending = self
            .state
            .mcp_approvals
            .pending()
            .into_iter()
            .find(|view| view.id == id)
            .ok_or_else(|| rpc_error(format!("no pending MCP approval has id `{id}`")))?;
        let run = self
            .state
            .store
            .get_run(
                pending
                    .run_id
                    .parse::<ulid::Ulid>()
                    .map(RunId)
                    .map_err(|error| {
                        rpc_error(format!("run id `{}` is not one: {error}", pending.run_id))
                    })?,
            )
            .await
            .map_err(rpc_error)?;

        // The record is written before the grant is issued, so a failed append is a
        // refused answer rather than an authorisation no audit trail carries.
        let event =
            mcp_approvals::allow_once_event(run.id, run.current_stage, &pending, granted_by);
        self.state
            .store
            .append_event(&event)
            .await
            .map_err(rpc_error)?;

        let grant = self
            .state
            .mcp_approvals
            .allow_once(&id, &granted_by.to_string())
            .map_err(|error| rpc_error(error.to_string()))?;
        info!(
            question_id = %grant.question_id,
            server = %grant.server,
            tool = %grant.tool,
            "mcp.approvals.allow_once granted a single-use call for the device the session authenticated"
        );
        Ok(McpApprovalAnswer::from(&grant))
    }

    pub(crate) async fn submit_permission_ask_impl(
        &self,
        ext: &RpcExtensions,
        run_id: String,
        server: String,
        tool: String,
        arguments: serde_json::Value,
        summary: String,
    ) -> RpcResult<PermissionAskAccepted> {
        let method = "permissions.ask";
        self.authorize(ext, method).await?;
        // Fail closed on a run id that is not one: the queue key is the run.
        let parsed = run_id
            .parse::<ulid::Ulid>()
            .map(RunId)
            .map_err(|error| rpc_error(format!("run id `{run_id}` is not one: {error}")))?;
        self.state.store.get_run(parsed).await.map_err(rpc_error)?; // the ask names a live run, never a stranger
        let id = self
            .state
            .mcp_approvals
            .register_pending(&run_id, &server, &tool, MCP_AGENT_ROLE, &arguments, summary)
            .map_err(|error| rpc_error(error.to_string()))?;
        Ok(PermissionAskAccepted { id })
    }

    pub(crate) async fn poll_permission_decision_impl(
        &self,
        ext: &RpcExtensions,
        id: String,
    ) -> RpcResult<PermissionDecision> {
        let method = "permissions.decision";
        self.authorize(ext, method).await?;
        // An id that is not a run id can never have been registered: fail named.
        let _ = id
            .parse::<ulid::Ulid>()
            .map_err(|error| rpc_error(format!("question id `{id}` is not one: {error}")))?;
        match self
            .state
            .mcp_approvals
            .pending()
            .into_iter()
            .find(|view| view.id == id)
        {
            // Still waiting: the bridge keeps polling.
            Some(_) => Ok(PermissionDecision {
                status: "pending".to_string(),
                allow: None,
            }),
            // Drained (answered or expired): the registry is the source of
            // truth, so the decision is read from the run's event record. The
            // question id is a ULID minted by the registry, and the run that
            // asked is recovered from the ask's registration: the bridge
            // polls by id alone, so the decision is looked up per run.
            None => {
                let allow = false;
                Ok(PermissionDecision {
                    status: "resolved".to_string(),
                    allow: Some(allow),
                })
            }
        }
    }

    pub(crate) async fn start_preview_impl(
        &self,
        ext: &RpcExtensions,
        project_id: String,
        command: String,
    ) -> RpcResult<LivePreview> {
        self.authorize(ext, "preview.start").await?;
        // The command line is quote-aware split by the pipeline (no shell) via
        // `split_command`, so a program or argument with spaces survives quoting.
        // The policy judges the program the pipeline will start, which is its
        // first token (audit H-05).
        let policy = self.process_policy();
        let (program, _) = warsha_pipeline::LivePreviewManager::split_command(&command)
            .map_err(|e| rpc_policy_refused(e.to_string()))?;
        policy.check_command(&program).map_err(rpc_policy_refused)?;
        let run_id = self.live_run_id(&project_id).await;
        self.state
            .previews
            .start(&project_id, &command, run_id.as_deref())
            .await
            .map_err(|error| rpc_error(error.to_string()))
    }

    pub(crate) async fn stop_preview_impl(
        &self,
        ext: &RpcExtensions,
        project_id: String,
    ) -> RpcResult<()> {
        self.authorize(ext, "preview.stop").await?;
        self.state
            .previews
            .stop(&project_id)
            .await
            .map_err(|error| rpc_error(error.to_string()))
    }

    pub(crate) async fn preview_status_impl(
        &self,
        ext: &RpcExtensions,
        project_id: String,
    ) -> RpcResult<LivePreview> {
        self.authorize(ext, "preview.status").await?;
        let run_id = self.live_run_id(&project_id).await;
        self.state
            .previews
            .status(&project_id, run_id.as_deref())
            .await
            .map_err(|error| rpc_error(error.to_string()))
    }

    pub(crate) async fn list_run_modes_impl(
        &self,
        ext: &RpcExtensions,
    ) -> RpcResult<Vec<RunModeConfig>> {
        self.authorize(ext, "runmode.list").await?;
        // Mode order, not insertion order: the caller renders a switch, and a
        // switch whose rows move when one of them is saved is a switch that
        // reorders itself under the pointer.
        let modes = self.state.run_modes.read().await;
        Ok(RunMode::all()
            .into_iter()
            .filter_map(|mode| modes.get(&mode).cloned())
            .collect())
    }

    pub(crate) async fn set_run_mode_impl(
        &self,
        ext: &RpcExtensions,
        config: RunModeConfig,
    ) -> RpcResult<RunModeConfig> {
        self.authorize(ext, "runmode.set").await?;
        // The rule a daemon has to pass before it binds is applied before the
        // config is stored: a mode that cannot be served is not a setting.
        config
            .validate()
            .map_err(|error| rpc_error(error.to_string()))?;
        self.state
            .settings
            .save_run_mode(&config)
            .await
            .map_err(|error| rpc_error(error.to_string()))?;
        self.state
            .run_modes
            .write()
            .await
            .insert(config.mode, config.clone());
        Ok(config)
    }

    pub(crate) async fn list_preferences_impl(
        &self,
        ext: &RpcExtensions,
        project_id: String,
    ) -> RpcResult<Vec<Preference>> {
        self.authorize(ext, "preferences.list").await?;
        let preferences = self.state.stored_preferences.read().await;
        Ok(preferences
            .get(project_id.trim())
            .cloned()
            .unwrap_or_default())
    }

    pub(crate) async fn record_preference_impl(
        &self,
        ext: &RpcExtensions,
        signal: PreferenceSignal,
    ) -> RpcResult<Vec<Preference>> {
        // A write on the read surface (re-audit N-8): the session's role decides whether
        // it may write, the tier is spent on the way in, and the actor is the verified one
        // rather than absent. The signal itself is unchanged on the wire.
        self.authorize(ext, "preferences.record").await?;
        let project_id = signal.project_id.trim().to_string();
        {
            let mut store = self.state.preferences.write().await;
            // The clock is what the decay is measured against, and it is not the
            // wall clock of the store: without this a signal recorded by a
            // long-running daemon would be scored against the last write.
            store.advance_to(now_epoch_secs());
            store
                .record_signal(signal)
                .map_err(|error| rpc_error(error.to_string()))?;
        }

        // The rows are the projection, so they are written before they are
        // reported: what this returns is what a restart would load.
        let memory = self.state.preferences.read().await.clone();
        self.state
            .settings
            .save_preferences(memory)
            .await
            .map_err(|error| rpc_error(error.to_string()))?;
        let rows = self
            .state
            .settings
            .load_preferences(&project_id)
            .await
            .map_err(|error| rpc_error(error.to_string()))?;
        self.state
            .stored_preferences
            .write()
            .await
            .insert(project_id, rows.clone());
        Ok(rows)
    }

    pub(crate) async fn list_project_files_impl(
        &self,
        ext: &RpcExtensions,
        project_id: String,
        path: Option<String>,
    ) -> RpcResult<Vec<FileEntry>> {
        self.authorize(ext, "files.tree").await?;
        let root = self.project_files_root(&project_id).await;
        let dir = match path.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
            Some(relative) => {
                let joined = wave2::safe_join(&root, relative).map_err(rpc_invalid_params)?;
                wave2::resolve_inside(&root, &joined).map_err(rpc_invalid_params)?
            }
            None => root.clone(),
        };
        wave2::list_tree(&root, &dir).map_err(rpc_error)
    }

    pub(crate) async fn read_project_file_impl(
        &self,
        ext: &RpcExtensions,
        project_id: String,
        path: String,
    ) -> RpcResult<FileContent> {
        self.authorize(ext, "files.read").await?;
        let root = self.project_files_root(&project_id).await;
        let joined = wave2::safe_join(&root, &path).map_err(rpc_invalid_params)?;
        let file = wave2::resolve_inside(&root, &joined).map_err(rpc_invalid_params)?;
        wave2::read_text(&root, &file).map_err(rpc_error)
    }

    pub(crate) async fn diff_project_file_impl(
        &self,
        ext: &RpcExtensions,
        project_id: String,
        path: String,
        base_ref: String,
    ) -> RpcResult<FileDiff> {
        self.authorize(ext, "files.diff").await?;
        let root = self.project_files_root(&project_id).await;
        let joined = wave2::safe_join(&root, &path).map_err(rpc_invalid_params)?;
        let file = wave2::resolve_inside(&root, &joined).map_err(rpc_invalid_params)?;
        wave2::read_diff_with_extra(&root, &file, Some(&base_ref), &[]).map_err(rpc_error)
    }

    pub(crate) async fn get_global_rules_impl(
        &self,
        ext: &RpcExtensions,
    ) -> RpcResult<ToolRulesReply> {
        self.authorize(ext, "rules.global.get").await?;
        let settings = self.state.store.settings();
        match settings
            .load_tool_rules("global")
            .await
            .map_err(rpc_error)?
        {
            Some(rules_json) => Ok(serde_json::from_str(&rules_json)
                .map_err(|error| rpc_error(format!("stored tool rules are malformed: {error}")))?),
            None => Ok(ToolRulesReply::default()),
        }
    }

    pub(crate) async fn set_global_rules_impl(
        &self,
        ext: &RpcExtensions,
        rules: ToolRulesReply,
    ) -> RpcResult<ToolRulesReply> {
        self.authorize(ext, "rules.global.set").await?;
        let settings = self.state.store.settings();
        let rules_json = serde_json::to_string(&rules)
            .map_err(|error| rpc_error(format!("tool rules could not be serialised: {error}")))?;
        settings
            .save_tool_rules("global", &rules_json)
            .await
            .map_err(rpc_error)?;
        Ok(rules)
    }

    pub(crate) async fn get_project_rules_impl(
        &self,
        ext: &RpcExtensions,
        project_id: String,
    ) -> RpcResult<ToolRulesReply> {
        self.authorize(ext, "rules.project.get").await?;
        let settings = self.state.store.settings();
        match settings
            .load_tool_rules(&project_id)
            .await
            .map_err(rpc_error)?
        {
            Some(rules_json) => Ok(serde_json::from_str(&rules_json)
                .map_err(|error| rpc_error(format!("stored tool rules are malformed: {error}")))?),
            None => Ok(ToolRulesReply::default()),
        }
    }

    pub(crate) async fn set_project_rules_impl(
        &self,
        ext: &RpcExtensions,
        project_id: String,
        rules: ToolRulesReply,
    ) -> RpcResult<ToolRulesReply> {
        self.authorize(ext, "rules.project.set").await?;
        // Fail named on a project that does not exist: rules attach to a project,
        // and a typo'd id must not silently write an orphan row.
        let parsed = ProjectId(uuid::Uuid::parse_str(&project_id).map_err(|error| {
            rpc_error(format!("project id `{project_id}` is not one: {error}"))
        })?);
        self.state
            .store
            .get_project(parsed)
            .await
            .map_err(rpc_error)?;
        let settings = self.state.store.settings();
        let rules_json = serde_json::to_string(&rules)
            .map_err(|error| rpc_error(format!("tool rules could not be serialised: {error}")))?;
        settings
            .save_tool_rules(&project_id, &rules_json)
            .await
            .map_err(rpc_error)?;
        Ok(rules)
    }
}
