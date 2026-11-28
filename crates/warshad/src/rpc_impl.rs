//! The `WarshaApiServer` implementation for [`WarshaRpc`], split out of `lib.rs` (ENGINE H1).
//! Moved verbatim; the crate root items it uses arrive through `use super::*`.

use super::*;

#[async_trait]
impl WarshaApiServer for WarshaRpc {
    async fn health(&self, ext: &RpcExtensions) -> RpcResult<HealthResponse> {
        self.authorize(ext, "health").await?;
        let capabilities = self.state.probe_capabilities().await;
        let status = if capabilities.daemon {
            "healthy".to_string()
        } else {
            "degraded".to_string()
        };
        Ok(HealthResponse {
            status,
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_seconds: self.start_time.elapsed().as_secs(),
            capabilities,
        })
    }

    async fn list_projects(&self, ext: &RpcExtensions) -> RpcResult<Vec<Project>> {
        self.authorize(ext, "projects.list").await?;
        self.state
            .store
            .list_projects()
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))
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
        self.authorize(ext, "projects.create").await?;
        let id = ProjectId::new();
        let validated_repo_path = match repo_path
            .as_deref()
            .map(str::trim)
            .filter(|p| !p.is_empty())
        {
            Some(path) => {
                warsha_pipeline::worktree::validate_repository(std::path::Path::new(path))
                    .await
                    .map_err(|e| {
                        ErrorObject::owned(-32000, format!("cannot use `{path}`: {e}"), None::<()>)
                    })?;
                path.to_string()
            }
            None => {
                let auto_repo = self
                    .state
                    .config
                    .data_dir
                    .join("projects")
                    .join(id.to_string())
                    .join("repo");
                warsha_pipeline::worktree::init_repository_with_meta(
                    &auto_repo,
                    Some(&name),
                    Some(&description),
                )
                .await
                .map_err(|e| {
                    ErrorObject::owned(
                        -32000,
                        format!(
                            "cannot initialize repository at `{}`: {e}",
                            auto_repo.display()
                        ),
                        None::<()>,
                    )
                })?;
                auto_repo.to_string_lossy().into_owned()
            }
        };
        let normalized_repo_url = repo_url
            .as_deref()
            .map(str::trim)
            .filter(|u| !u.is_empty())
            .map(str::to_string);
        let project = Project {
            id,
            name,
            description,
            language,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            repo_path: Some(validated_repo_path),
            repo_url: normalized_repo_url,
            confinement,
            approval_mode,
            permissions: ProjectPermissions {
                confinement: confinement.unwrap_or_default(),
                approval_mode: approval_mode.unwrap_or_default(),
                is_override: confinement.is_some() || approval_mode.is_some(),
            },
        };
        self.state
            .store
            .create_project(&project)
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))?;
        Ok(project)
    }

    async fn project_github_status(&self, ext: &RpcExtensions) -> RpcResult<GithubStatus> {
        self.authorize(ext, "projects.github_status").await?;
        match self.resolve_github_token().await {
            Some((_token, source)) => Ok(GithubStatus {
                configured: true,
                source: Some(source.to_string()),
            }),
            None => Ok(GithubStatus {
                configured: false,
                source: None,
            }),
        }
    }

    async fn prepare_project_folder(
        &self,
        ext: &RpcExtensions,
        parent_dir: String,
        name: String,
    ) -> RpcResult<PreparedFolder> {
        self.authorize(ext, "projects.prepare_folder").await?;
        let trimmed_parent = parent_dir.trim();
        if trimmed_parent.is_empty() {
            return Err(rpc_invalid_params("parent_dir must not be empty"));
        }
        let prepared = warsha_pipeline::worktree::prepare_new_folder(
            std::path::Path::new(trimmed_parent),
            &name,
        )
        .await
        .map_err(|err| match err {
            warsha_pipeline::worktree::WorktreeError::InvalidName { .. } => {
                rpc_invalid_params(err.to_string())
            }
            other => rpc_error(other),
        })?;
        Ok(PreparedFolder {
            repo_path: prepared.to_string_lossy().into_owned(),
        })
    }

    async fn prepare_existing_folder(
        &self,
        ext: &RpcExtensions,
        path: String,
    ) -> RpcResult<PreparedFolder> {
        self.authorize(ext, "projects.prepare_existing_folder")
            .await?;
        let trimmed = path.trim();
        if trimmed.is_empty() {
            return Err(rpc_invalid_params("path must not be empty"));
        }
        let prepared =
            warsha_pipeline::worktree::prepare_existing_folder(std::path::Path::new(trimmed))
                .await
                .map_err(|err| match err {
                    warsha_pipeline::worktree::WorktreeError::InvalidName { .. } => {
                        rpc_invalid_params(err.to_string())
                    }
                    other => rpc_error(other),
                })?;
        Ok(PreparedFolder {
            repo_path: prepared.to_string_lossy().into_owned(),
        })
    }

    async fn project_repo_status(
        &self,
        ext: &RpcExtensions,
        path: Option<String>,
        parent_dir: Option<String>,
        name: Option<String>,
    ) -> RpcResult<RepoStatus> {
        self.authorize(ext, "projects.repo_status").await?;
        // Exactly one form: either `path`, or `parent_dir` + `name` — both
        // set or both absent is a caller mistake (-32602), never a guess.
        let target = match (
            path.as_deref().map(str::trim).filter(|s| !s.is_empty()),
            parent_dir
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty()),
            name.as_deref(),
        ) {
            (Some(p), None, None) => std::path::PathBuf::from(p),
            (None, Some(parent), Some(raw_name)) => {
                match warsha_pipeline::worktree::join_folder_name(
                    std::path::Path::new(parent),
                    raw_name,
                ) {
                    Ok(target) => target,
                    Err(warsha_pipeline::worktree::WorktreeError::InvalidName { .. }) => {
                        return Err(rpc_invalid_params(format!(
                            "invalid folder name `{raw_name}`"
                        )));
                    }
                    Err(other) => return Err(rpc_error(other)),
                }
            }
            _ => {
                return Err(rpc_invalid_params(
                    "provide either `path` or both `parent_dir` and `name`",
                ));
            }
        };
        let probed = warsha_pipeline::worktree::repo_status(&target).await;
        Ok(RepoStatus {
            exists: probed.exists,
            is_repo: probed.is_repo,
            has_head: probed.has_head,
        })
    }

    async fn clone_project_repo(
        &self,
        ext: &RpcExtensions,
        repo_url: String,
        parent_dir: Option<String>,
        folder_name: Option<String>,
    ) -> RpcResult<ClonedRepo> {
        self.authorize(ext, "projects.clone_repo").await?;
        let dest_parent = self.resolve_projects_parent_dir(parent_dir).await?;
        let token = self.resolve_github_token().await.map(|(t, _)| t);
        let (cloned_path, clean_repo_url) = warsha_pipeline::worktree::clone_repository(
            &repo_url,
            &dest_parent,
            folder_name.as_deref(),
            token.as_deref(),
        )
        .await
        .map_err(|err| {
            let mut msg = err.to_string();
            if let Some(tok) = token.as_deref() {
                msg = msg.replace(tok, "[REDACTED]");
            }
            match err {
                warsha_pipeline::worktree::WorktreeError::InvalidName { .. } => {
                    rpc_invalid_params(msg)
                }
                _ => rpc_error(msg),
            }
        })?;
        Ok(ClonedRepo {
            repo_path: cloned_path.to_string_lossy().into_owned(),
            repo_url: clean_repo_url,
        })
    }

    async fn create_github_repo(
        &self,
        ext: &RpcExtensions,
        name: String,
        description: Option<String>,
        parent_dir: Option<String>,
        private_repo: Option<bool>,
    ) -> RpcResult<CreatedGithubRepo> {
        self.authorize(ext, "projects.create_github_repo").await?;
        let trimmed_name = name.trim();
        if trimmed_name.is_empty() {
            return Err(rpc_invalid_params("repository name must not be empty"));
        }
        // Absent means private: a new GitHub repo must never go public by
        // accident. Only an explicit `private_repo: false` makes it public.
        let private = private_repo.unwrap_or(true);
        let Some((token, _source)) = self.resolve_github_token().await else {
            return Err(ErrorObject::owned(
                -32000,
                "GitHub token is not configured. Configure one in Settings -> Integrations.",
                None::<()>,
            ));
        };
        let mut client = warsha_git::GitHubClient::new(&token)
            .map_err(|err| rpc_error(err.to_string().replace(&token, "[REDACTED]")))?;
        if let Some(base) = self
            .state
            .config
            .github_api_base
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            client = client.with_api_base(base);
        }
        let repo = client
            .create_repo(
                trimmed_name,
                description.as_deref().unwrap_or(""),
                None,
                private,
            )
            .await
            .map_err(|err| rpc_error(err.to_string().replace(&token, "[REDACTED]")))?;

        let clone_url = if !repo.clone_url.trim().is_empty() {
            repo.clone_url.trim().to_string()
        } else if !repo.html_url.trim().is_empty() {
            format!("{}.git", repo.html_url.trim().trim_end_matches('/'))
        } else {
            return Err(rpc_error(
                "GitHub create_repo response did not include clone_url or html_url",
            ));
        };

        let dest_parent = self.resolve_projects_parent_dir(parent_dir).await?;
        let (cloned_path, clean_repo_url) = warsha_pipeline::worktree::clone_repository(
            &clone_url,
            &dest_parent,
            Some(repo.name.as_str()),
            Some(&token),
        )
        .await
        .map_err(|err| rpc_error(err.to_string().replace(&token, "[REDACTED]")))?;

        Ok(CreatedGithubRepo {
            repo_path: cloned_path.to_string_lossy().into_owned(),
            repo_url: clean_repo_url.unwrap_or(clone_url),
            full_name: repo.full_name,
            html_url: repo.html_url,
        })
    }

    async fn get_default_permissions(&self, ext: &RpcExtensions) -> RpcResult<ProjectPermissions> {
        self.authorize(ext, "permissions.defaults.get").await?;
        self.state
            .store
            .get_default_permissions()
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))
    }

    async fn set_default_permissions(
        &self,
        ext: &RpcExtensions,
        confinement: Confinement,
        approval_mode: ApprovalMode,
    ) -> RpcResult<ProjectPermissions> {
        self.authorize(ext, "permissions.defaults.set").await?;
        let permissions = ProjectPermissions {
            confinement,
            approval_mode,
            is_override: true,
        };
        self.state
            .store
            .set_default_permissions(&permissions)
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))?;
        Ok(permissions)
    }

    async fn get_project_permissions(
        &self,
        ext: &RpcExtensions,
        project_id: ProjectId,
    ) -> RpcResult<ProjectPermissions> {
        self.authorize(ext, "permissions.project.get").await?;
        self.state
            .store
            .get_project_effective_permissions(project_id)
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))
    }

    async fn set_project_permissions(
        &self,
        ext: &RpcExtensions,
        project_id: ProjectId,
        confinement: Option<Confinement>,
        approval_mode: Option<ApprovalMode>,
    ) -> RpcResult<ProjectPermissions> {
        self.authorize(ext, "permissions.project.set").await?;
        self.state
            .store
            .set_project_permissions(project_id, confinement, approval_mode)
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))
    }

    async fn get_swarm_settings(
        &self,
        ext: &RpcExtensions,
        project_id: ProjectId,
    ) -> RpcResult<warsha_store::StoredProjectSwarmSettings> {
        self.authorize(ext, "swarm.project.get").await?;
        self.state
            .store
            .settings()
            .load_project_swarm_settings(&project_id.0.to_string())
            .await
            .map_err(rpc_error)
    }

    async fn set_swarm_settings(
        &self,
        ext: &RpcExtensions,
        project_id: ProjectId,
        enable_swarm: bool,
        max_parallel_agents: Option<u32>,
    ) -> RpcResult<warsha_store::StoredProjectSwarmSettings> {
        self.authorize(ext, "swarm.project.set").await?;
        let current = self
            .state
            .store
            .settings()
            .load_project_swarm_settings(&project_id.0.to_string())
            .await
            .map_err(rpc_error)?;
        self.state
            .store
            .settings()
            .save_project_swarm_settings(&warsha_store::StoredProjectSwarmSettings {
                project_id: project_id.0.to_string(),
                enable_swarm,
                max_parallel_agents: max_parallel_agents.unwrap_or(current.max_parallel_agents),
            })
            .await
            .map_err(rpc_error)?;
        self.state
            .store
            .settings()
            .load_project_swarm_settings(&project_id.0.to_string())
            .await
            .map_err(rpc_error)
    }

    async fn get_project(&self, ext: &RpcExtensions, id: ProjectId) -> RpcResult<Project> {
        self.authorize(ext, "projects.get").await?;
        self.state
            .store
            .get_project(id)
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))
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
        self.authorize(ext, "projects.attach").await?;
        let mut project = self
            .state
            .store
            .get_project(project_id)
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))?;
        if let Some(existing) = project
            .repo_path
            .as_deref()
            .map(str::trim)
            .filter(|path| !path.is_empty())
        {
            let auto_repo = self
                .state
                .config
                .data_dir
                .join("projects")
                .join(project_id.to_string())
                .join("repo");
            let is_auto_initted = std::path::Path::new(existing) == auto_repo.as_path();
            if !is_auto_initted {
                return Err(ErrorObject::owned(
                    -32000,
                    format!(
                        "project {} already has the repository `{existing}`; \
                         it must be detached before another is attached",
                        project_id.0
                    ),
                    None::<()>,
                ));
            }
        }
        let path = repo_path.trim();
        warsha_pipeline::worktree::validate_repository(std::path::Path::new(path))
            .await
            .map_err(|e| {
                ErrorObject::owned(-32000, format!("cannot attach `{path}`: {e}"), None::<()>)
            })?;
        project.repo_path = Some(path.to_string());
        project.updated_at = Utc::now();
        self.state
            .store
            .update_project(&project)
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))?;
        Ok(project)
    }

    async fn estimate_run(
        &self,
        ext: &RpcExtensions,
        project_id: ProjectId,
        idea: String,
    ) -> RpcResult<CostEstimate> {
        self.authorize(ext, "runs.estimate").await?;
        crate::api_estimate::compute_run_estimate(self, project_id, &idea)
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))
    }

    async fn interview_start(
        &self,
        ext: &RpcExtensions,
        project_id: ProjectId,
        seed_idea: Option<String>,
    ) -> RpcResult<crate::secretary::TurnOutcome> {
        self.authorize(ext, "interview.start").await?;
        let cap_cents = self.secretary_daily_cap_cents().await;
        let engine = crate::secretary::SecretaryEngine::new(
            self.state.store.clone(),
            self.state.providers.clone(),
            Some(self.state.settings.clone()),
        );
        engine
            .start(project_id, seed_idea, cap_cents)
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))
    }

    async fn interview_turn(
        &self,
        ext: &RpcExtensions,
        project_id: ProjectId,
        reply: String,
    ) -> RpcResult<crate::secretary::TurnOutcome> {
        self.authorize(ext, "interview.turn").await?;
        let cap_cents = self.secretary_daily_cap_cents().await;
        let engine = crate::secretary::SecretaryEngine::new(
            self.state.store.clone(),
            self.state.providers.clone(),
            Some(self.state.settings.clone()),
        );
        engine
            .turn(project_id, reply, cap_cents)
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))
    }

    async fn interview_status(
        &self,
        ext: &RpcExtensions,
        project_id: ProjectId,
    ) -> RpcResult<Option<crate::secretary::InterviewState>> {
        self.authorize(ext, "interview.status").await?;
        let engine = crate::secretary::SecretaryEngine::new(
            self.state.store.clone(),
            self.state.providers.clone(),
            Some(self.state.settings.clone()),
        );
        engine
            .status(project_id)
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))
    }

    async fn secretary_thread_list(
        &self,
        ext: &RpcExtensions,
        project_id: Option<ProjectId>,
    ) -> RpcResult<Vec<SecretaryConversation>> {
        self.authorize(ext, "secretary.thread.list").await?;
        let store_err =
            |e: warsha_store::StoreError| ErrorObject::owned(-32000, e.to_string(), None::<()>);
        let mut conversations = self
            .state
            .store
            .list_secretary_conversations(project_id.map(|id| id.0))
            .await
            .map_err(store_err)?;
        if project_id.is_some() {
            // A project's list also carries the global thread.
            let global = self
                .state
                .store
                .list_secretary_conversations(None)
                .await
                .map_err(store_err)?;
            for conversation in global {
                if !conversations.iter().any(|c| c.id == conversation.id) {
                    conversations.push(conversation);
                }
            }
            conversations.sort_by_key(|c| std::cmp::Reverse(c.updated_at));
        }
        Ok(conversations)
    }

    async fn secretary_thread_turns(
        &self,
        ext: &RpcExtensions,
        conversation_id: Ulid,
        project_id: Option<ProjectId>,
    ) -> RpcResult<Vec<SecretaryTurn>> {
        self.authorize(ext, "secretary.thread.turns").await?;
        if let Some(pid) = project_id {
            let conversation = self
                .state
                .store
                .get_secretary_conversation(conversation_id)
                .await
                .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))?
                .ok_or_else(|| rpc_invalid_params("conversation not found"))?;
            if conversation.project_id != Some(pid.0) {
                return Err(rpc_invalid_params(
                    "conversation belongs to a different project",
                ));
            }
        }
        Ok(self
            .state
            .store
            .list_secretary_turns(conversation_id)
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))?)
    }

    async fn secretary_chat(
        &self,
        ext: &RpcExtensions,
        project_id: Option<ProjectId>,
        conversation_id: Option<Ulid>,
        message: String,
        language: Option<String>,
    ) -> RpcResult<crate::secretary_chat::ChatTurnReply> {
        self.authorize(ext, "secretary.chat").await?;
        let language = crate::secretary_chat::normalize_language(language.as_deref())
            .map_err(|e| rpc_invalid_params(e.to_string()))?;
        let cap_cents = self.secretary_daily_cap_cents().await;
        let chat = crate::secretary_chat::SecretaryChat::new(
            self.state.store.clone(),
            self.state.providers.clone(),
            self.state.mcp_approvals.clone(),
        );
        let reply = chat
            .turn_with_language(
                project_id.map(|id| id.0),
                conversation_id,
                &message,
                cap_cents,
                language,
            )
            .await
            .map_err(|e| match e {
                crate::secretary_chat::SecretaryChatError::DailyLimitReached {
                    spent_cents,
                    cap_cents,
                } => ErrorObject::owned(
                    -32000,
                    format!(
                        "daily limit reached: spent {spent_cents} cents of {cap_cents} cents cap"
                    ),
                    Some(serde_json::json!({
                        "spent_cents": spent_cents,
                        "cap_cents": cap_cents,
                    })),
                ),
                crate::secretary_chat::SecretaryChatError::InvalidParams(message) => {
                    rpc_invalid_params(message)
                }
                _ => ErrorObject::owned(-32000, e.to_string(), None::<()>),
            })?;
        Ok(reply)
    }

    async fn secretary_provisioning_list(
        &self,
        ext: &RpcExtensions,
        project_id: ProjectId,
    ) -> RpcResult<Vec<ProvisioningRecord>> {
        self.authorize(ext, "secretary.provisioning.list").await?;
        self.state
            .store
            .list_provisioning(project_id.0)
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))
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
        self.authorize(ext, "backend.export").await?;
        // The kit is the project's, not the run's: the stored row is keyed by
        // project id, so a second run of one project exports the same schema
        // instead of starting a second, empty one. It is loaded (the daemon's copy,
        // then the stored row, then the default) and re-remembered, so an export is
        // the write-through that would otherwise never happen for a kit that was
        // only read. The dump, the storage archive stem and the header all name the
        // project, not the run.
        let (key, project_name) = self.project_for_run(&run_id).await;
        let kit = self.backend_kit(&key, &project_name).await;
        self.remember_backend_kit(&key, &kit).await?;

        kit.export_to_dir(&output_dir)
            .map_err(|error| rpc_error(error.to_string()))
    }

    // Continue on server (PLAN.md M6a)
    async fn create_handoff(
        &self,
        ext: &RpcExtensions,
        run_id: String,
        files: Vec<(String, String)>,
    ) -> RpcResult<String> {
        self.authorize(ext, "handoff.create").await?;
        let manager = HandoffManager::new();
        let captured: Vec<(&str, &str)> = files
            .iter()
            .map(|(path, content)| (path.as_str(), content.as_str()))
            .collect();
        let bundle = manager.create_bundle(&run_id, &captured);

        // Bundles live under the daemon's data directory, next to the store.
        let dir = self.state.config.data_dir.join(HANDOFF_DIR);
        let dir = dir.to_string_lossy().into_owned();
        manager
            .write_bundle(&bundle, &dir)
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))
    }

    async fn apply_handoff(
        &self,
        ext: &RpcExtensions,
        path: String,
        owner_wins: bool,
    ) -> RpcResult<String> {
        self.authorize(ext, "handoff.apply").await?;
        let encoded = std::fs::read_to_string(&path).map_err(|e| {
            ErrorObject::owned(-32000, format!("could not read `{path}`: {e}"), None::<()>)
        })?;
        let bundle: HandoffBundle = serde_json::from_str(&encoded).map_err(|e| {
            ErrorObject::owned(
                -32000,
                format!("`{path}` is not a handoff bundle: {e}"),
                None::<()>,
            )
        })?;

        // RunId → Run → Project.repo_path is the tree the bundle is written into.
        let root = self
            .working_tree_for_run(&bundle.run_id)
            .await
            .ok_or_else(|| {
                ErrorObject::owned(
                    -32000,
                    format!("run `{}` has no project repo to apply into", bundle.run_id),
                    None::<()>,
                )
            })?;

        // The apply copies every file it overwrites into
        // `{working_tree}/.warsha-backup/{timestamp}/` and then writes the files,
        // so the returned summary names the conflicts and that backup directory.
        let result = HandoffManager::new()
            .apply_to_disk(&bundle, &root.to_string_lossy(), owner_wins)
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))?;

        serde_json::to_string(&result)
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))
    }

    // Post-launch care (PLAN.md M17)
    async fn list_crash_reports(
        &self,
        ext: &RpcExtensions,
        run_id: String,
    ) -> RpcResult<Vec<CrashReport>> {
        self.authorize(ext, "postlaunch.crashes").await?;
        let reports = self.state.crash_reports.read().await.list();
        let wanted = run_id.trim();
        // A blank run id asks for every report, which is what the crash screen
        // shows before a run is selected.
        Ok(if wanted.is_empty() {
            reports
        } else {
            reports
                .into_iter()
                .filter(|report| report.run_id == wanted)
                .collect()
        })
    }

    async fn list_store_reviews(
        &self,
        ext: &RpcExtensions,
        store: String,
    ) -> RpcResult<Vec<StoreReview>> {
        self.authorize(ext, "postlaunch.reviews").await?;
        let reviews = self.state.store_reviews.read().await;
        let wanted = store.trim();
        Ok(if wanted.is_empty() {
            reviews.list()
        } else {
            reviews.list_by_store(wanted)
        })
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
        self.authorize(ext, "agent.turn").await?;
        Err(ErrorObject::owned(
            -32000,
            "agent.turn is retired: start a run and use agent.turn_run with its run id",
            None::<()>,
        ))
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
        self.authorize(ext, "agent.turn_run").await?;
        // Fail closed: a drifted skills lock refuses the turn instead of running it
        // with a catalog the owner's lock does not name.
        let skills = self.active_skill_catalog().await?;
        self.state
            .agent_sessions
            .run_turn(run_id, &input, &skills)
            .await
            .map_err(|error| ErrorObject::owned(-32000, error.to_string(), None::<()>))
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
        self.agent_switch(ext, AgentMode::Agent, run_id).await
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
        self.agent_switch(ext, AgentMode::Plan, run_id).await
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
        self.authorize(ext, "agent.state").await?;
        if let Some(run_id) = run_id {
            let state = self
                .state
                .agent_sessions
                .state(run_id)
                .await
                .map_err(|error| rpc_error(error.to_string()))?;
            return Ok(AgentState {
                mode: state.mode,
                approvals: state.approvals,
                todos: state.todos,
                model_wired: state.model_wired,
            });
        }
        let agent_loop = self.state.agent_loop.read().await;
        Ok(AgentState {
            mode: agent_loop.mode(),
            approvals: agent_loop.mode_manager.history.clone(),
            todos: agent_loop.todo_list.items.clone(),
            // The loop's router seam is wired by `build_agent_loop`, so this answers the
            // question the owner actually has: is there a model behind the next turn.
            model_wired: agent_loop.has_router(),
        })
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
        self.authorize(ext, "backend.get").await?;
        let (key, _project_name) = self.project_for_run(&run_id).await;
        // A project that never exported has no stored row, and reporting the
        // default as if it were one would invent a kit the owner never reviewed.
        // A kit set in this life does count: the owner asked for exactly that one.
        if let Some(kit) = self.state.backend_kits.read().await.get(&key) {
            return Ok(Some(kit.clone()));
        }
        self.state
            .settings
            .load_backend_kit(&key)
            .await
            .map_err(|error| rpc_error(error.to_string()))
    }

    async fn set_backend(
        &self,
        ext: &RpcExtensions,
        run_id: String,
        kit: BackendKit,
    ) -> RpcResult<()> {
        self.authorize(ext, "backend.set").await?;
        let (key, _project_name) = self.project_for_run(&run_id).await;
        self.remember_backend_kit(&key, &kit).await
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
        self.authorize(ext, "routines.list").await?;
        let registry = self.state.routines.read().await;
        Ok(registry.list().into_iter().cloned().collect())
    }

    async fn get_routine(&self, ext: &RpcExtensions, id: String) -> RpcResult<Option<Routine>> {
        self.authorize(ext, "routines.get").await?;
        let registry = self.state.routines.read().await;
        Ok(registry.get(&id).cloned())
    }

    async fn run_routine_now(&self, ext: &RpcExtensions, id: String) -> RpcResult<Routine> {
        self.authorize(ext, "routines.run_now").await?;
        self.state
            .routine_scheduler
            .run_now(&id)
            .await
            .map_err(|error| rpc_error(error.to_string()))?;
        let registry = self.state.routines.read().await;
        registry
            .get(&id)
            .cloned()
            .ok_or_else(|| rpc_error(format!("routine {id} not found")))
    }

    async fn pause_routine(&self, ext: &RpcExtensions, id: String) -> RpcResult<Routine> {
        self.authorize(ext, "routines.pause").await?;
        self.state
            .routine_scheduler
            .pause(&id)
            .await
            .map_err(|error| rpc_error(error.to_string()))?;
        let registry = self.state.routines.read().await;
        registry
            .get(&id)
            .cloned()
            .ok_or_else(|| rpc_error(format!("routine {id} not found")))
    }

    async fn resume_routine(&self, ext: &RpcExtensions, id: String) -> RpcResult<Routine> {
        self.authorize(ext, "routines.resume").await?;
        self.state
            .routine_scheduler
            .resume(&id)
            .await
            .map_err(|error| rpc_error(error.to_string()))?;
        let registry = self.state.routines.read().await;
        registry
            .get(&id)
            .cloned()
            .ok_or_else(|| rpc_error(format!("routine {id} not found")))
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
