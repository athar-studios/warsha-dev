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
        self.authorize(ext, "runs.start").await?;
        let run_mode = run_mode
            .and_then(|m| warsha_core::RunMode::from_str(&m).ok())
            .unwrap_or_default();

        let pending_budget = PENDING_PROJECT_BUDGETS
            .read()
            .ok()
            .and_then(|p| p.get(&project_id).copied());
        let effective_start_budget = budget_cents.or(pending_budget);

        let mut run = self
            .state
            .pipeline
            .start_run_in_mode_with_budget(
                project_id,
                &idea,
                run_mode,
                spec_file_path.as_deref(),
                effective_start_budget,
            )
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))?;

        // D4: a Plan or SpecFile run stops at the GateB plan-approval gate by
        // construction — it must not get a worktree (zero workspace changes
        // before approval) and must not get an agent session loop yet. Both are
        // created lazily when the owner approves GateB and the run advances.
        if run.run_mode == warsha_core::RunMode::Plan
            || run.run_mode == warsha_core::RunMode::SpecFile
        {
            return apply_start_budget(&self.state, project_id, run, budget_cents).await;
        }

        // Wire active harness + enabled skills + enabled plugins into run start context (F2)
        let active_harness = self
            .state
            .settings
            .get_active_harness(&project_id.to_string())
            .await
            .ok()
            .flatten()
            .unwrap_or_else(|| "autonomous-factory".to_string());
        let enabled_skills: Vec<String> = self
            .state
            .settings
            .list_skills()
            .await
            .unwrap_or_default()
            .into_iter()
            .filter(|s| s.enabled)
            .map(|s| s.id)
            .collect();
        let enabled_plugins: Vec<String> = self
            .state
            .settings
            .list_plugins()
            .await
            .unwrap_or_default()
            .into_iter()
            .filter(|p| p.enabled)
            .map(|p| p.id)
            .collect();

        tracing::info!(
            target: "audit",
            run_id = %run.id.0,
            stage = ?run.current_stage,
            harness = %active_harness,
            skills = ?enabled_skills,
            plugins = ?enabled_plugins,
            "run start audit context"
        );

        // ENGINE E3 wiring: a project that names a repository gets a per-run
        // worktree, so two runs cannot edit one checkout. A run whose worktree
        // cannot be created is cancelled rather than left to execute in the shared
        // workspace — the failure is visible instead of silently sharing files.
        let project = self
            .state
            .store
            .get_project(project_id)
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))?;

        let effective = self
            .state
            .store
            .get_project_effective_permissions(project_id)
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))?;

        if let Some(repo_path) = project
            .repo_path
            .as_deref()
            .map(str::trim)
            .filter(|path| !path.is_empty())
        {
            let root = self.state.config.data_dir.join("worktrees");
            match warsha_pipeline::create_run_worktree(
                std::path::Path::new(repo_path),
                &root,
                &run.id.0.to_string(),
                "HEAD",
            )
            .await
            {
                Ok(worktree) => {
                    run.worktree_path = Some(worktree.path.to_string_lossy().into_owned());
                    run.worktree_branch = Some(format!("warsha/run-{}", run.id.0));
                    if let Err(e) = self.state.store.update_run(&run).await {
                        tracing::warn!("failed to persist run worktree path: {e}");
                    }
                    self.state
                        .run_worktrees
                        .write()
                        .await
                        .insert(run.id, worktree);
                }
                Err(error) => {
                    let _ = self
                        .state
                        .pipeline
                        .cancel_run(run.id, Some("worktree creation failed".to_string()))
                        .await;
                    return Err(ErrorObject::owned(
                        -32000,
                        format!(
                            "the run was cancelled because its worktree could not be created: {error}"
                        ),
                        None::<()>,
                    ));
                }
            }
        }

        // ENGINE E2 wiring: each run gets its own loop bound to its own checkout
        // (the worktree when it has one, else the daemon workspace), inserted under
        // the run id. `agent.turn_run` runs the turn there; `agent.turn`, the legacy
        // single conversation, is retired by OD-E2 and refuses.
        let workspace = match self.state.run_worktrees.read().await.get(&run.id) {
            Some(worktree) => worktree.path.clone(),
            None => self.state.config.data_dir.join(AGENT_WORKSPACE_DIR),
        };
        let backend_kit = Arc::new(RwLock::new(default_backend_kit("run")));
        // The run's own loop records every MCP tool call it makes on the run's event
        // log (R-EXT-2 A1): the seam is the store's recorder, and it is attached here
        // because this is the one loop builder that knows both the run id and the
        // store. The daemon-wide loop above has neither and records nothing.
        let per_run_loop = build_agent_loop_in(
            &workspace,
            &self.state.config.allowed_commands,
            self.state.providers.clone(),
            self.state.mcp.clone(),
            self.state.mcp_permissions.clone(),
            self.state.mcp_approvals.clone(),
            Some(run.id.0.to_string()),
            backend_kit,
            Some(effective),
        )
        .with_mcp_observer(Arc::new(StoreMcpCallObserver::new(
            self.state.store.clone(),
        )));
        if self
            .state
            .agent_sessions
            .insert(run.id, per_run_loop)
            .is_err()
        {
            warn!(run_id = %run.id.0, "the run already had an agent session");
        }

        let run = apply_start_budget(&self.state, project_id, run, budget_cents).await?;

        // F-R20-1 / R21-B2 / M-1: Dispatch initial Idea (intake) stage in a spawned task
        // when the budget is not exactly zero (Plan and SpecFile modes do not reach this
        // path; SpecFile takes the budget and Plan executes intake). A zero budget is
        // already exhausted, so intake is not dispatched and the run stays at Idea.
        // Intake runs asynchronously, advancing to GateA on success or surfacing
        // Paused + provider_required immediately when unrouted, without blocking runs.start
        // on a slow or hung provider. The task handle is tracked so shutdown can abort it.
        if run.budget_cents != Some(0) {
            let pipeline = self.state.pipeline.clone();
            let run_id = run.id;
            let handle = tokio::spawn(async move {
                if let Err(err) = pipeline.advance_to_next_gate(run_id).await {
                    warn!(run_id = %run_id.0, "async intake dispatch failed: {err}");
                }
            });
            // Prune finished handles and still track the new one if the mutex
            // was poisoned.
            let mut tasks = self
                .state
                .intake_tasks
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            tasks.retain(|task| !task.is_finished());
            tasks.push(handle);
        }

        Ok(run)
    }

    async fn run_worktree(
        &self,
        ext: &RpcExtensions,
        run_id: RunId,
    ) -> RpcResult<Option<RunWorktree>> {
        self.authorize(ext, "runs.worktree").await?;
        Ok(self.state.run_worktrees.read().await.get(&run_id).cloned())
    }

    /// Collect the diff, changed paths and one measured test run for Gate C.
    async fn gate_evidence(
        &self,
        ext: &RpcExtensions,
        run_id: RunId,
        program: String,
        args: Vec<String>,
    ) -> RpcResult<warsha_pipeline::GateEvidence> {
        self.authorize(ext, "gates.evidence").await?;
        let run = self
            .state
            .store
            .get_run(run_id)
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))?;
        if run.current_stage != Stage::GateC {
            return Err(ErrorObject::owned(
                -32000,
                format!(
                    "gate evidence is for Gate C; the run is at {:?}",
                    run.current_stage
                ),
                None::<()>,
            ));
        }
        let work_dir =
            {
                let worktrees = self.state.run_worktrees.read().await;
                match worktrees.get(&run_id) {
                    Some(worktree) => worktree.path.clone(),
                    None => return Err(ErrorObject::owned(
                        -32000,
                        "the run has no worktree; Gate C evidence needs a repository-backed run"
                            .to_string(),
                        None::<()>,
                    )),
                }
            };
        // The test program is the owner's, but it is still judged by the same
        // command policy as RunCommand: no shell, the allow-list, no traversal.
        let policy = CommandPolicy::from_config(&self.state.config.allowed_commands);
        policy
            .check_program(&program)
            .map_err(|e| ErrorObject::owned(-32005, e.to_string(), None::<()>))?;
        let evidence =
            warsha_pipeline::collect_gate_evidence(&policy, &work_dir, "HEAD", &program, &args)
                .await
                .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))?;
        // The preview joins the diff and the tests (ENGINE E5). It is read from what
        // the manager already tracks for the run's project and it is read in that
        // run's context (R-BRIDGE-B5 D4): evidence is a run's own evidence, so a
        // server another live run owns is not this run's preview. It is never
        // started: a run with no preview is the explicit `None` the caller sees,
        // not an error and not a server launched to fill the gap.
        let preview = self
            .state
            .previews
            .status(&run.project_id.to_string(), Some(&run.id.0.to_string()))
            .await
            .ok();
        Ok(evidence.with_preview(preview))
    }

    async fn get_run(&self, ext: &RpcExtensions, run_id: RunId) -> RpcResult<Run> {
        self.authorize(ext, "runs.get").await?;
        let run = self
            .state
            .pipeline
            .check_stale_run(run_id)
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))?;
        if run.status == RunStatus::Failed {
            self.state.mcp_approvals.purge_run(&run.id.0.to_string());
            self.state.previews.stop_run(&run.id.0.to_string()).await;
        }
        Ok(run)
    }

    async fn list_runs(&self, ext: &RpcExtensions, project_id: ProjectId) -> RpcResult<Vec<Run>> {
        self.authorize(ext, "runs.list").await?;
        let runs = self
            .state
            .store
            .list_runs(project_id)
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))?;
        let mut checked_runs = Vec::with_capacity(runs.len());
        for run in runs {
            if run.status == RunStatus::Running {
                let checked = self
                    .state
                    .pipeline
                    .check_stale_run(run.id)
                    .await
                    .unwrap_or(run);
                if checked.status == RunStatus::Failed {
                    self.state
                        .mcp_approvals
                        .purge_run(&checked.id.0.to_string());
                    self.state
                        .previews
                        .stop_run(&checked.id.0.to_string())
                        .await;
                }
                checked_runs.push(checked);
            } else {
                checked_runs.push(run);
            }
        }
        Ok(checked_runs)
    }

    async fn get_events(&self, ext: &RpcExtensions, run_id: RunId) -> RpcResult<Vec<Event>> {
        self.authorize(ext, "runs.events").await?;
        let token = self.state.config.auth_token.clone();
        let mut events = self
            .state
            .store
            .get_events(run_id)
            .await
            .map_err(rpc_error)?;
        // The store holds the payload as it was written; the wire never sees a secret.
        for event in &mut events {
            scrub_event_payload(&mut event.payload, &token);
        }
        Ok(events)
    }

    async fn list_sessions(&self, ext: &RpcExtensions, run_id: RunId) -> RpcResult<Vec<Session>> {
        self.authorize(ext, "sessions.list").await?;
        self.state
            .store
            .list_sessions_by_run(run_id)
            .await
            .map_err(rpc_error)
    }

    async fn list_run_subtasks(
        &self,
        ext: &RpcExtensions,
        run_id: RunId,
    ) -> RpcResult<Vec<warsha_core::RunSubtask>> {
        self.authorize(ext, "runs.subtasks").await?;
        self.state
            .store
            .list_run_subtasks(run_id)
            .await
            .map_err(rpc_error)
    }

    async fn ask_start(
        &self,
        ext: &RpcExtensions,
        project_id: ProjectId,
        _idea: String,
    ) -> RpcResult<warsha_core::SessionId> {
        self.authorize(ext, "ask.start").await?;

        // 1. Create Ask-mode Session (role Secretary)
        let session_id = warsha_core::SessionId::new();
        let session = warsha_core::Session {
            id: session_id,
            run_id: None, // Migration 016
            project_id,
            role: warsha_core::Role::Secretary,
            backend: "native".to_string(),
            provider: "default".to_string(),
            model: "default".to_string(),
            status: warsha_core::SessionStatus::Working,
            started_at: chrono::Utc::now(),
            ended_at: None,
            parent_session_id: None,
            cli_session_id: None,
        };
        self.state
            .store
            .create_session(&session)
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))?;

        // 2. Build ReadOnly Agent Loop
        let permissions = warsha_core::ProjectPermissions {
            confinement: warsha_core::Confinement::ReadOnly,
            approval_mode: warsha_core::ApprovalMode::NeverAsk,
            is_override: true,
        };

        let workspace = self.state.config.data_dir.join(AGENT_WORKSPACE_DIR);
        let backend_kit = Arc::new(RwLock::new(default_backend_kit("ask")));

        let per_ask_loop = build_agent_loop_in(
            &workspace,
            &self.state.config.allowed_commands,
            self.state.providers.clone(),
            self.state.mcp.clone(),
            self.state.mcp_permissions.clone(),
            self.state.mcp_approvals.clone(),
            None, // No run_id for Ask mode
            backend_kit,
            Some(permissions),
        );

        // Store session loop
        self.state
            .agent_sessions
            .insert(warsha_core::RunId(session_id.0), per_ask_loop)
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))?;

        Ok(session_id)
    }

    async fn ask_turn(
        &self,
        ext: &RpcExtensions,
        project_id: ProjectId,
        session_id: String,
        reply: String,
    ) -> RpcResult<crate::secretary::TurnOutcome> {
        self.authorize(ext, "ask.turn").await?;
        let sid = warsha_core::SessionId(
            ulid::Ulid::from_str(&session_id)
                .map_err(|_| ErrorObject::owned(-32000, "invalid session id", None::<()>))?,
        );
        let run_id = warsha_core::RunId(sid.0);

        // Use public accessor - run_turn is public
        let outcome = self
            .state
            .agent_sessions
            .run_turn(run_id, &reply, &[])
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))?;

        self.state
            .store
            .record_role_cost(
                project_id,
                warsha_core::Role::Secretary,
                "native",
                "default",
                0,
                0,
                0,
                chrono::Utc::now(),
            )
            .await
            .ok();

        Ok(crate::secretary::TurnOutcome::NextQuestion {
            question: format!("Turn finished: {:?}", outcome),
            topic: crate::secretary::Topic::Audience,
            state: crate::secretary::InterviewState::new(project_id, None),
        })
    }

    async fn ask_status(
        &self,
        ext: &RpcExtensions,
        _project_id: ProjectId,
        session_id: String,
    ) -> RpcResult<crate::secretary::TurnOutcome> {
        self.authorize(ext, "ask.status").await?;
        let sid = warsha_core::SessionId(
            ulid::Ulid::from_str(&session_id)
                .map_err(|_| ErrorObject::owned(-32000, "invalid session id", None::<()>))?,
        );
        let session = self
            .state
            .store
            .get_session(sid)
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))?
            .ok_or_else(|| ErrorObject::owned(-32000, "session not found", None::<()>))?;

        Ok(crate::secretary::TurnOutcome::NextQuestion {
            question: format!("Session status: {:?}", session.status),
            topic: crate::secretary::Topic::Audience,
            state: crate::secretary::InterviewState::new(session.project_id, None),
        })
    }

    async fn session_tail(
        &self,
        ext: &RpcExtensions,
        session_id: SessionId,
    ) -> RpcResult<Vec<String>> {
        self.authorize(ext, "sessions.tail").await?;
        let token = self.state.config.auth_token.clone();
        let tail = self.state.pipeline.live_sessions.get_tail(session_id).await;
        let scrubbed = tail
            .into_iter()
            .map(|line| {
                if crate::state::redact_string(&line, &token) {
                    crate::state::REDACTED.to_string()
                } else {
                    line
                }
            })
            .collect();
        Ok(scrubbed)
    }

    async fn session_stop(&self, ext: &RpcExtensions, session_id: SessionId) -> RpcResult<()> {
        self.authorize(ext, "sessions.stop").await?;
        self.state
            .pipeline
            .live_sessions
            .stop_session(session_id, &self.state.store)
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))
    }

    async fn approve_gate(
        &self,
        ext: &RpcExtensions,
        run_id: RunId,
        stage: Stage,
        device_id: Option<DeviceId>,
    ) -> RpcResult<Run> {
        let actor = self.authorize(ext, "gates.approve").await?;
        let approver = self.gate_approver(&actor, device_id, "gates.approve")?;
        self.state
            .pipeline
            .approve_gate(run_id, stage, approver)
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))
    }

    async fn reject_gate(
        &self,
        ext: &RpcExtensions,
        run_id: RunId,
        stage: Stage,
        reason: String,
        device_id: Option<DeviceId>,
    ) -> RpcResult<Run> {
        let actor = self.authorize(ext, "gates.reject").await?;
        let approver = self.gate_approver(&actor, device_id, "gates.reject")?;
        self.state
            .pipeline
            .reject_gate(run_id, stage, reason, approver)
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))
    }

    async fn request_changes(
        &self,
        ext: &RpcExtensions,
        run_id: RunId,
        stage: Stage,
        notes: String,
        device_id: Option<DeviceId>,
    ) -> RpcResult<Run> {
        let actor = self.authorize(ext, "gates.changes").await?;
        let approver = self.gate_approver(&actor, device_id, "gates.changes")?;
        self.state
            .pipeline
            .request_changes(run_id, stage, notes, approver)
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))
    }

    async fn list_gates(
        &self,
        ext: &RpcExtensions,
        run_id: RunId,
        device_id: Option<DeviceId>,
    ) -> RpcResult<Vec<GateSummary>> {
        self.authorize(ext, "gates.list").await?;
        // Accepted for the gates block's legacy param shape, and believed for nothing:
        // this read records no identity, so there is no session claim to compare it to
        // and no caller-named device that changes what is answered.
        let _ = device_id;
        // Read from `Run.gate_states` through the store's run getter — **not** from
        // `Store::list_gates`. The `gates` table's only writer, `create_gate`, has no
        // production caller, so a table-backed read would answer `[]` for every run and
        // turn every queue into a silent lie (the trap DECISIONS.md 2026-11-03 #25
        // records; `Run.gate_states` is what the pipeline seeds on `start_run` and what
        // `gates.approve`/`reject`/`changes` mutate, so it is the live truth).
        let run = self
            .state
            .store
            .get_run(run_id)
            .await
            .map_err(|e| rpc_error(e.to_string()))?;
        Ok(Stage::all()
            .iter()
            .filter(|stage| stage.is_gate())
            .map(|stage| GateSummary {
                stage: *stage,
                state: run
                    .gate_states
                    .get(stage)
                    .cloned()
                    .unwrap_or_default()
                    .into(),
            })
            .collect())
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

    async fn stop_process(&self, ext: &RpcExtensions, id: String) -> RpcResult<()> {
        self.authorize(ext, "process.stop").await?;
        self.state
            .processes
            .stop(&id)
            .await
            .map_err(|error| rpc_error(error.to_string()))
    }

    async fn process_status(&self, ext: &RpcExtensions, id: String) -> RpcResult<ProcessStatus> {
        self.authorize(ext, "process.status").await?;
        self.state
            .processes
            .status(&id)
            .await
            .map_err(|error| rpc_error(error.to_string()))
    }

    async fn list_processes(&self, ext: &RpcExtensions) -> RpcResult<Vec<ProcessSummary>> {
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

    async fn connect_mcp(
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

    async fn disconnect_mcp(&self, ext: &RpcExtensions, name: String) -> RpcResult<()> {
        self.authorize(ext, "mcp.disconnect").await?;
        self.state
            .mcp
            .disconnect(&name)
            .await
            .map_err(|error| rpc_error(error.to_string()))
    }

    async fn list_mcp(&self, ext: &RpcExtensions) -> RpcResult<Vec<McpConnection>> {
        self.authorize(ext, "mcp.list").await?;
        Ok(self.state.mcp.list().await)
    }

    async fn send_mcp(
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

    async fn list_mcp_permissions(&self, ext: &RpcExtensions) -> RpcResult<McpPermissionsReply> {
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

    async fn set_mcp_permission(
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

    async fn list_mcp_approvals(&self, ext: &RpcExtensions) -> RpcResult<Vec<McpApproval>> {
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

    async fn allow_once_mcp_approval(
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
    async fn poll_permission_decision(
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

    async fn stop_preview(&self, ext: &RpcExtensions, project_id: String) -> RpcResult<()> {
        self.authorize(ext, "preview.stop").await?;
        self.state
            .previews
            .stop(&project_id)
            .await
            .map_err(|error| rpc_error(error.to_string()))
    }

    async fn preview_status(
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

    // Settings that survive a restart (PLAN.md M20)
    async fn list_run_modes(&self, ext: &RpcExtensions) -> RpcResult<Vec<RunModeConfig>> {
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

    async fn set_run_mode(
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
        self.authorize(ext, "preferences.list").await?;
        let preferences = self.state.stored_preferences.read().await;
        Ok(preferences
            .get(project_id.trim())
            .cloned()
            .unwrap_or_default())
    }

    async fn record_preference(
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
        self.authorize(ext, "runs.subscribe").await?;
        // No cursor means "start from where the stream is now", so nothing is replayed;
        // `0` means "everything the ring still holds".
        let sync = self.state.events.sync(since_seq.unwrap_or(0), run_id).await;
        let run = match run_id {
            Some(id) => self.state.store.get_run(id).await.ok(),
            None => None,
        };
        Ok(RunStreamSync {
            seq: sync.current_seq,
            gap_detected: sync.gap_detected,
            resync_required: sync.resync_required,
            dropped_count: sync.dropped_count,
            run,
            events: sync.frames,
        })
    }

    async fn advance_run(&self, ext: &RpcExtensions, run_id: RunId) -> RpcResult<Run> {
        self.authorize(ext, "runs.advance").await?;
        // No mock routing is seeded here: W5's pipeline dispatch answers an unrouted
        // role with the deterministic `FakeProvider` (or the env-configured model), so
        // a fresh daemon with no provider still advances a run offline with rich stage
        // artifacts. `advance_to_next_gate` walks the run to the next undecided gate
        // (passing approved ones) and marks a run that reaches `Feedback` completed.
        let run = self
            .state
            .pipeline
            .advance_to_next_gate(run_id)
            .await
            .map_err(rpc_error)?;

        // Record factory context on advance (F2)
        if let Ok(active_harness) = self
            .state
            .settings
            .get_active_harness(&run.project_id.to_string())
            .await
        {
            let harness_id = active_harness.unwrap_or_else(|| "autonomous-factory".to_string());
            let enabled_skills: Vec<String> = self
                .state
                .settings
                .list_skills()
                .await
                .unwrap_or_default()
                .into_iter()
                .filter(|s| s.enabled)
                .map(|s| s.id)
                .collect();
            let enabled_plugins: Vec<String> = self
                .state
                .settings
                .list_plugins()
                .await
                .unwrap_or_default()
                .into_iter()
                .filter(|p| p.enabled)
                .map(|p| p.id)
                .collect();

            tracing::info!(
                target: "audit",
                run_id = %run.id.0,
                stage = ?run.current_stage,
                harness = %harness_id,
                skills = ?enabled_skills,
                plugins = ?enabled_plugins,
                "run advance audit context"
            );
        }

        // The same rule `runs.cancel` follows, for the run that finishes instead of
        // being cancelled: a terminal run has no next call, so its last question and
        // any unspent grant are dead weight — and an unspent grant is an authority
        // nobody will be asked about again (D2.8).
        if matches!(
            run.status,
            RunStatus::Completed | RunStatus::Failed | RunStatus::Cancelled
        ) {
            self.state.mcp_approvals.purge_run(&run.id.0.to_string());
            // The run that just ended also stops the dev server it owns
            // (R-BRIDGE-B5 D2): a preview is a run's, not the project's, so it dies
            // with the run instead of being inherited by the next one. Best-effort
            // by construction — `stop_run` returns no error — so a dev server that
            // will not die is logged and never fails this close.
            self.state.previews.stop_run(&run.id.0.to_string()).await;
        }

        Ok(run)
    }

    async fn cancel_run(
        &self,
        ext: &RpcExtensions,
        run_id: RunId,
        reason: Option<String>,
    ) -> RpcResult<Run> {
        let actor = self.authorize(ext, "runs.cancel").await?;
        let reason = Some(reason.unwrap_or_else(|| "Cancelled by user".to_string()));
        // The run's `run_finished` event says which authenticated device cancelled it
        // (R-EXT-2 A2). The device is the session's, never a parameter: the caller cannot
        // name itself.
        let cancelled = self
            .state
            .pipeline
            .cancel_run_with_device(run_id, reason, actor.device_id())
            .await
            .map_err(rpc_error)?;
        // A cancelled run has no next call, so its pending questions and unspent
        // grants are dead weight — and an unspent grant is an authority nobody will
        // be asked about again (D2.8).
        self.state.mcp_approvals.purge_run(&run_id.0.to_string());
        // And the same close stops the preview it owns (R-BRIDGE-B5 D2), on the
        // same best-effort rule as the terminal arm above.
        self.state.previews.stop_run(&run_id.0.to_string()).await;
        Ok(cancelled)
    }

    async fn resume_run(&self, ext: &RpcExtensions, run_id: RunId) -> RpcResult<Run> {
        self.authorize(ext, "runs.resume").await?;
        self.state
            .pipeline
            .resume_run(run_id)
            .await
            .map_err(rpc_error)
    }

    async fn followup_run(
        &self,
        ext: &RpcExtensions,
        run_id: RunId,
        prompt: String,
    ) -> RpcResult<Run> {
        self.authorize(ext, "runs.followup").await?;
        let trimmed = prompt.trim();
        if trimmed.is_empty() {
            return Err(ErrorObject::owned(
                -32602,
                "runs.followup needs a non-empty prompt",
                None::<()>,
            ));
        }
        let ctrl = self.state.pipeline.control_for(run_id).await;
        let _state_guard = ctrl.state_lock.lock().await;
        if *ctrl.cancel_tx.borrow() {
            return Err(rpc_error("Cannot apply followup to a cancelled run"));
        }
        let (mut run, version) = self
            .state
            .store
            .get_run_versioned(run_id)
            .await
            .map_err(rpc_error)?;
        match run.status {
            RunStatus::Cancelled => {
                return Err(rpc_error("Cannot apply followup to a cancelled run"));
            }
            RunStatus::Completed | RunStatus::Failed => {
                return Err(rpc_error(format!(
                    "Cannot apply followup to a terminal ({:?}) run",
                    run.status
                )));
            }
            RunStatus::Running | RunStatus::Paused => {}
        }
        let content = format!(
            "Follow-up applied at stage {:?}: {}",
            run.current_stage, trimmed
        );
        let now = Utc::now();
        run.updated_at = now;
        run.artifacts.insert(
            "followup".to_string(),
            Artifact {
                name: "followup".to_string(),
                content: content.clone(),
                stage: run.current_stage,
                role: None,
                created_at: now,
                updated_at: now,
                version: 1,
            },
        );
        self.state
            .store
            .update_run_cas(&run, version)
            .await
            .map_err(rpc_error)?;
        // Persist the durable artifact file and event log only after the CAS state update succeeds.
        self.state
            .store
            .write_artifact(run.project_id, "followup", &content)
            .await
            .map_err(rpc_error)?;
        let event = Event {
            id: ulid::Ulid::new(),
            run_id,
            stage: run.current_stage,
            role: None,
            kind: EventKind::AgentStep,
            payload: serde_json::json!({
                "action": "followup",
                "prompt": trimmed,
                "response": content,
            }),
            cost_cents: None,
            tokens: None,
            created_at: now,
            device_id: None,
        };
        self.state
            .store
            .append_event(&event)
            .await
            .map_err(rpc_error)?;
        Ok(run)
    }

    async fn list_run_artifacts(
        &self,
        ext: &RpcExtensions,
        run_id: RunId,
    ) -> RpcResult<Vec<Artifact>> {
        self.authorize(ext, "runs.artifacts").await?;
        let run = self.state.store.get_run(run_id).await.map_err(rpc_error)?;
        let mut artifacts: Vec<Artifact> = run.artifacts.values().cloned().collect();
        artifacts.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(artifacts)
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
        self.authorize(ext, "harness.list").await?;
        let active_id = self
            .state
            .settings
            .get_active_harness(ACTIVE_HARNESS_GLOBAL_KEY)
            .await
            .map_err(rpc_error)?
            .unwrap_or_else(|| "autonomous-factory".to_string());
        let rows = self
            .state
            .settings
            .list_harnesses()
            .await
            .map_err(rpc_error)?;
        Ok(rows
            .iter()
            .map(|row| {
                let mut def = wave2::harness_definition(row);
                def.active = def.id == active_id;
                def
            })
            .collect())
    }

    async fn get_harness(&self, ext: &RpcExtensions, id: String) -> RpcResult<HarnessDefinition> {
        self.authorize(ext, "harness.get").await?;
        let active_id = self
            .state
            .settings
            .get_active_harness(ACTIVE_HARNESS_GLOBAL_KEY)
            .await
            .map_err(rpc_error)?
            .unwrap_or_else(|| "autonomous-factory".to_string());
        let mut def = wave2::harness_definition(&self.stored_harness(&id).await?);
        def.active = def.id == active_id;
        Ok(def)
    }

    async fn import_harness(
        &self,
        ext: &RpcExtensions,
        content: String,
        format: Option<String>,
    ) -> RpcResult<HarnessDefinition> {
        self.authorize(ext, "harness.import").await?;
        let definition =
            wave2::parse_harness_import(&content, format.as_deref()).map_err(rpc_invalid_params)?;
        let row = wave2::harness_row(&definition).map_err(rpc_error)?;
        self.state
            .settings
            .save_harness(&row)
            .await
            .map_err(rpc_error)?;
        let active_id = self
            .state
            .settings
            .get_active_harness(ACTIVE_HARNESS_GLOBAL_KEY)
            .await
            .map_err(rpc_error)?
            .unwrap_or_else(|| "autonomous-factory".to_string());
        let mut def = wave2::harness_definition(&self.stored_harness(&row.id).await?);
        def.active = def.id == active_id;
        Ok(def)
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
        self.authorize(ext, "harness.select").await?;
        self.stored_harness(&harness_id).await?;
        let key = project_id
            .as_deref()
            .map(str::trim)
            .filter(|project| !project.is_empty())
            .unwrap_or(ACTIVE_HARNESS_GLOBAL_KEY);
        self.state
            .settings
            .set_active_harness(key, &harness_id)
            .await
            .map_err(rpc_error)
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
        self.authorize(ext, "harness.delete").await?;
        let stored = self.stored_harness(&id).await?;
        if stored.is_preset {
            return Err(rpc_invalid_params(format!(
                "`{id}` is a bundled harness; harness.delete removes the ones you imported, \
                 and a preset returns at the next start because the daemon ships it"
            )));
        }
        self.state
            .settings
            .delete_harness(&id)
            .await
            .map_err(rpc_error)?;
        if self
            .state
            .settings
            .get_active_harness(ACTIVE_HARNESS_GLOBAL_KEY)
            .await
            .map_err(rpc_error)?
            .as_deref()
            == Some(id.as_str())
        {
            self.state
                .settings
                .clear_active_harness(ACTIVE_HARNESS_GLOBAL_KEY)
                .await
                .map_err(rpc_error)?;
        }
        Ok(())
    }

    async fn list_coding_backends(
        &self,
        ext: &RpcExtensions,
    ) -> RpcResult<Vec<CodingBackendSummary>> {
        self.authorize(ext, "coding_backends.list").await?;
        let catalog = warsha_backends::catalog::EngineCatalog::builtin();
        let summaries = catalog
            .entries()
            .iter()
            .map(|entry| CodingBackendSummary {
                name: entry.name.clone(),
                display_name: entry.display_name.clone(),
                command: entry.command.clone(),
                installed: entry.is_installed(),
                tier: match entry.tier {
                    warsha_backends::catalog::VerificationTier::A => "A".to_string(),
                    warsha_backends::catalog::VerificationTier::B => "B".to_string(),
                    warsha_backends::catalog::VerificationTier::C => "C".to_string(),
                },
                version: None,
            })
            .collect();
        Ok(summaries)
    }

    async fn test_coding_backend(
        &self,
        ext: &RpcExtensions,
        name: String,
    ) -> RpcResult<CodingBackendDiagnostics> {
        self.authorize(ext, "coding_backends.test").await?;
        let catalog = warsha_backends::catalog::EngineCatalog::builtin();
        let entry = catalog.get(&name).ok_or_else(|| {
            jsonrpsee::types::ErrorObject::owned(
                -32602,
                format!("unknown coding backend `{name}`"),
                None::<()>,
            )
        })?;

        let diag = entry.check_health(std::time::Duration::from_secs(3)).await;

        Ok(CodingBackendDiagnostics {
            name: diag.name,
            command: diag.command,
            installed: diag.installed,
            ready: diag.ready,
            version: diag.version,
            status: diag.status,
            auth_required: diag.auth_required,
        })
    }

    async fn get_backend_routing(
        &self,
        ext: &RpcExtensions,
    ) -> RpcResult<crate::runtime::BackendRoutingsConfig> {
        self.authorize(ext, "backend_routing.get").await?;
        let config = load_backend_routings(&self.state.settings).await;
        Ok(config)
    }

    async fn set_backend_routing(
        &self,
        ext: &RpcExtensions,
        default_backend: Option<String>,
        role_backends: Option<std::collections::HashMap<String, String>>,
        project_backends: Option<std::collections::HashMap<String, String>>,
    ) -> RpcResult<crate::runtime::BackendRoutingsConfig> {
        self.authorize(ext, "backend_routing.set").await?;
        let mut config = load_backend_routings(&self.state.settings).await;

        if let Some(def) = default_backend {
            let trimmed = def.trim();
            if trimmed.is_empty() || trimmed == "native" || trimmed == "builtin" {
                config.default_backend = None;
            } else {
                config.default_backend = Some(trimmed.to_string());
            }
        }
        if let Some(roles) = role_backends {
            config.role_backends = roles;
        }
        if let Some(projects) = project_backends {
            config.project_backends = projects;
        }

        store_backend_routings(&self.state.settings, &config)
            .await
            .map_err(|e| {
                ErrorObject::owned(
                    -32000,
                    format!("failed to persist backend routings: {e}"),
                    None::<()>,
                )
            })?;

        let registry = self.state.pipeline.backend_registry();
        registry.set_default_backend(config.default_backend.clone());

        for (role_str, backend_name) in &config.role_backends {
            if let Ok(role) = serde_json::from_value::<warsha_core::Role>(
                serde_json::Value::String(role_str.to_lowercase()),
            ) {
                registry.set_role_backend(role, backend_name);
            }
        }
        for (project_str, backend_name) in &config.project_backends {
            if let Ok(project_uuid) = project_str.parse::<uuid::Uuid>() {
                registry.set_project_backend(warsha_core::ProjectId(project_uuid), backend_name);
            }
        }

        Ok(config)
    }

    /// The installed skills, in the shape the owner manages them in.
    ///
    /// Every row, enabled or not: a screen that could only see the enabled ones could
    /// not switch one back on. `skill_catalog()` remains the in-process registry the
    /// agent loop runs, built from exactly these rows with the disabled ones dropped.
    async fn list_skills(&self, ext: &RpcExtensions) -> RpcResult<Vec<SkillDefinition>> {
        self.authorize(ext, "skills.list").await?;
        let rows = self.state.settings.list_skills().await.map_err(rpc_error)?;
        let mut skills = Vec::with_capacity(rows.len());
        for row in &rows {
            skills.push(wave2::skill_definition(row).map_err(rpc_error)?);
        }
        Ok(skills)
    }

    async fn import_skill(
        &self,
        ext: &RpcExtensions,
        url_or_content: String,
        name: Option<String>,
    ) -> RpcResult<SkillDefinition> {
        self.authorize(ext, "skills.import").await?;
        let definition = wave2::parse_skill_import(&url_or_content, name.as_deref())
            .map_err(rpc_invalid_params)?;
        let row = wave2::skill_row(&definition).map_err(rpc_error)?;
        self.state
            .settings
            .save_skill(&row)
            .await
            .map_err(rpc_error)?;
        let stored = self
            .state
            .settings
            .get_skill(&row.id)
            .await
            .map_err(rpc_error)?
            .ok_or_else(|| {
                rpc_error(format!(
                    "the skill `{}` was not in the store after saving",
                    row.id
                ))
            })?;
        wave2::skill_definition(&stored).map_err(rpc_error)
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
        self.authorize(ext, "skills.toggle").await?;
        let row = self.stored_skill(&id).await?;
        if row.enabled != enabled {
            self.state
                .settings
                .toggle_skill(&id)
                .await
                .map_err(rpc_error)?;
        }
        let stored = self
            .state
            .settings
            .get_skill(&id)
            .await
            .map_err(rpc_error)?
            .ok_or_else(|| {
                rpc_error(format!(
                    "the skill `{id}` vanished while it was being enabled"
                ))
            })?;
        wave2::skill_definition(&stored).map_err(rpc_error)
    }

    async fn delete_skill(&self, ext: &RpcExtensions, id: String) -> RpcResult<()> {
        self.authorize(ext, "skills.delete").await?;
        self.stored_skill(&id).await?;
        self.state
            .settings
            .delete_skill(&id)
            .await
            .map_err(rpc_error)?;
        Ok(())
    }

    async fn list_plugins(&self, ext: &RpcExtensions) -> RpcResult<Vec<PluginDefinition>> {
        self.authorize(ext, "plugins.list").await?;
        let rows = self
            .state
            .settings
            .list_plugins()
            .await
            .map_err(rpc_error)?;
        let mut plugins = Vec::with_capacity(rows.len());
        for row in &rows {
            plugins.push(wave2::plugin_definition(row).map_err(rpc_error)?);
        }
        Ok(plugins)
    }

    async fn install_plugin(
        &self,
        ext: &RpcExtensions,
        manifest_json_or_url: String,
    ) -> RpcResult<PluginDefinition> {
        self.authorize(ext, "plugins.install").await?;
        let mut definition =
            wave2::parse_plugin_manifest(&manifest_json_or_url).map_err(rpc_invalid_params)?;
        // KEEL K5b: the pack's program is judged before its environment is filed
        // and before any row exists, so a refused pack leaves nothing behind.
        self.check_plugin_launch(&definition)?;
        // A manifest may name a token outright, and this is the seam that decides
        // where it lives: the keychain holds the value, the row holds the reference.
        let env = self
            .file_plugin_env(&definition.id, &definition.env)
            .await?;
        definition.env = env;
        let row = wave2::plugin_row(&definition).map_err(rpc_error)?;
        self.state
            .settings
            .save_plugin(&row)
            .await
            .map_err(rpc_error)?;
        let stored = self
            .state
            .settings
            .get_plugin(&row.id)
            .await
            .map_err(rpc_error)?
            .ok_or_else(|| {
                rpc_error(format!(
                    "the plugin `{}` was not in the store after saving",
                    row.id
                ))
            })?;
        wave2::plugin_definition(&stored).map_err(rpc_error)
    }

    async fn toggle_plugin(
        &self,
        ext: &RpcExtensions,
        id: String,
        enabled: bool,
    ) -> RpcResult<PluginDefinition> {
        self.authorize(ext, "plugins.toggle").await?;
        let row = self.stored_plugin(&id).await?;
        let checked = wave2::plugin_definition(&row).map_err(rpc_error)?;
        // Enabling is the moment a row becomes a process. A row written before
        // this check existed must not launch a program — or fetch a package — the
        // policy would refuse today; disabling only stops a process and is always
        // allowed (KEEL K5b).
        let mut launch_env = HashMap::new();
        if enabled {
            self.check_plugin_launch(&checked)?;
            launch_env = self.resolved_plugin_env(&checked).await?;
        }
        // Whether this call is the one that rewrites the row — the flag the rollback below
        // keys on, because a toggle to the state the row already has wrote nothing to undo.
        let flipped = row.enabled != enabled;
        if flipped {
            self.state
                .settings
                .toggle_plugin(&id)
                .await
                .map_err(rpc_error)?;
        }
        let updated = self.stored_plugin(&id).await?;
        let def = wave2::plugin_definition(&updated).map_err(rpc_error)?;
        // Sync with MCP manager (F2). The child gets exactly the pack's resolved
        // environment on top of the process manager's floor; its failure is the
        // RPC's failure, not a warning in a log nobody reads.
        if enabled && def.transport == "stdio" {
            if let Some(cmd) = &def.command {
                if !cmd.trim().is_empty() {
                    let mcp_cfg = warsha_pipeline::mcp_transport::McpServerConfig::new_stdio(
                        &def.id,
                        cmd,
                        def.args.clone(),
                    );
                    if let Err(error) = self
                        .state
                        .mcp
                        .connect_with(
                            mcp_cfg,
                            warsha_pipeline::mcp_transport::McpLaunchOptions::with_env(launch_env),
                        )
                        .await
                    {
                        // The row was persisted before the spawn, so a connect that fails
                        // would leave it saying `enabled` with no process behind it, and
                        // nothing re-derives that until the next toggle (R-KEEL
                        // checkpoint row 16). Restore the state the row had before this
                        // call — and still return the connect's own error, which is the
                        // RPC's contract. A rollback that fails too is named inside that
                        // error rather than swallowed.
                        let rollback = if flipped {
                            self.state.settings.toggle_plugin(&id).await.err()
                        } else {
                            None
                        };
                        return Err(match rollback {
                            None => rpc_error(error.to_string()),
                            Some(rollback_error) => rpc_error(format!(
                                "{error} (the row could not be put back the way it was: \
                                 {rollback_error})"
                            )),
                        });
                    }
                }
            }
        } else if !enabled
            && def.transport == "stdio"
            && def
                .command
                .as_deref()
                .map(|c| !c.trim().is_empty())
                .unwrap_or(false)
        {
            if let Err(error) = self.state.mcp.disconnect(&def.id).await {
                // The mirror of the connect above, in the other direction (R-KEEL
                // checkpoint row 16, the disable half): the row was persisted
                // `disabled` before the child was stopped, so a disconnect that fails
                // would leave it saying disabled with a live process behind it, and
                // nothing re-derives that until the next toggle. Restore the state the
                // row had before this call — and still return the disconnect's own
                // error, which is the RPC's contract.
                let rollback = if flipped {
                    self.state.settings.toggle_plugin(&id).await.err()
                } else {
                    None
                };
                return Err(match rollback {
                    None => rpc_error(error.to_string()),
                    Some(rollback_error) => rpc_error(format!(
                        "{error} (the row could not be put back the way it was: \
                         {rollback_error})"
                    )),
                });
            }
        }
        Ok(def)
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
        self.authorize(ext, "plugins.configure").await?;
        let mut row = self.stored_plugin(&id).await?;
        // KEEL K5b: judge the program and the variable names before a value is
        // filed in the keychain or the row is rewritten. A refused configure must
        // not leave half its environment stored.
        let mut candidate = wave2::plugin_definition(&row).map_err(rpc_error)?;
        candidate.env = env.clone();
        self.check_plugin_launch(&candidate)?;
        let references = self.file_plugin_env(&id, &env).await?;
        // The row is about to lose its previous environment, so keep it: a connect with the
        // new one that fails must not leave the row describing a child that was never
        // started (R-KEEL checkpoint row 16, the `configure` half).
        let previous_env_json = row.env_json.clone();
        row.env_json = serde_json::to_string(&references).map_err(rpc_error)?;
        self.state
            .settings
            .save_plugin(&row)
            .await
            .map_err(rpc_error)?;
        let updated = self.stored_plugin(&id).await?;
        let def = wave2::plugin_definition(&updated).map_err(rpc_error)?;
        // Sync with MCP manager if enabled (F2), with the references resolved
        // against the keychain only now, for this one child.
        if def.enabled && def.transport == "stdio" {
            if let Some(cmd) = &def.command {
                if !cmd.trim().is_empty() {
                    let launch_env = self.resolved_plugin_env(&def).await?;
                    let mcp_cfg = warsha_pipeline::mcp_transport::McpServerConfig::new_stdio(
                        &def.id,
                        cmd,
                        def.args.clone(),
                    );
                    if let Err(error) = self
                        .state
                        .mcp
                        .connect_with(
                            mcp_cfg,
                            warsha_pipeline::mcp_transport::McpLaunchOptions::with_env(launch_env),
                        )
                        .await
                    {
                        // Put the previous environment back: the caller still gets the
                        // connect's error, but the row is not left naming an environment
                        // whose child never started. The keychain entries the new
                        // environment filed stay, for the next configure of those keys to
                        // overwrite — the same rule a key this call stopped naming follows.
                        row.env_json = previous_env_json;
                        let rollback = self.state.settings.save_plugin(&row).await.err();
                        return Err(match rollback {
                            None => rpc_error(error.to_string()),
                            Some(rollback_error) => rpc_error(format!(
                                "{error} (the previous environment could not be restored: \
                                 {rollback_error})"
                            )),
                        });
                    }
                }
            }
        }
        Ok(def)
    }

    async fn remove_plugin(&self, ext: &RpcExtensions, id: String) -> RpcResult<()> {
        self.authorize(ext, "plugins.remove").await?;
        let row = self.stored_plugin(&id).await?;
        // Keychain first, exactly as `providers.remove` does it: a row deleted while
        // its secrets stayed behind would be an address nothing could reach again.
        let def = wave2::plugin_definition(&row).map_err(rpc_error)?;
        for account in wave2::filed_secret_accounts(&def.env) {
            self.state.secrets.delete(&account).await.map_err(|error| {
                rpc_error(format!(
                    "the secret `{account}` could not be removed from the OS keychain: {error}"
                ))
            })?;
        }
        self.state
            .settings
            .delete_plugin(&id)
            .await
            .map_err(rpc_error)?;
        Ok(())
    }

    async fn list_custom_models(
        &self,
        ext: &RpcExtensions,
        provider: Option<String>,
    ) -> RpcResult<Vec<CustomModelRecord>> {
        self.authorize(ext, "models.list").await?;
        let rows = self
            .state
            .settings
            .list_custom_models()
            .await
            .map_err(rpc_error)?;
        let wanted = provider
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string);
        Ok(rows
            .iter()
            .map(wave2::custom_model_record)
            .filter(|record| match &wanted {
                Some(name) => &record.provider == name,
                None => true,
            })
            .collect())
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
        self.authorize(ext, "models.add").await?;
        if provider.trim().is_empty() || model_id.trim().is_empty() {
            return Err(rpc_invalid_params(
                "models.add needs both a provider and a model id: the pair is the row's key",
            ));
        }
        let record = CustomModelRecord {
            provider: provider.trim().to_string(),
            model_id: model_id.trim().to_string(),
            display_name: display_name
                .map(|name| name.trim().to_string())
                .filter(|name| !name.is_empty()),
            context_window,
            enabled: true,
        };
        self.state
            .settings
            .save_custom_model(&wave2::custom_model_row(&record))
            .await
            .map_err(rpc_error)?;
        let stored = self
            .state
            .settings
            .get_custom_model(&record.provider, &record.model_id)
            .await
            .map_err(rpc_error)?
            .ok_or_else(|| rpc_error("the custom model was not in the store after saving"))?;
        Ok(wave2::custom_model_record(&stored))
    }

    async fn remove_custom_model(
        &self,
        ext: &RpcExtensions,
        provider: String,
        model_id: String,
    ) -> RpcResult<()> {
        self.authorize(ext, "models.remove").await?;
        self.state
            .settings
            .delete_custom_model(&provider, &model_id)
            .await
            .map_err(rpc_error)?;
        Ok(())
    }

    async fn list_integrations(&self, ext: &RpcExtensions) -> RpcResult<Vec<IntegrationStatus>> {
        self.authorize(ext, "integrations.list").await?;
        self.integration_statuses().await
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
        self.authorize(ext, "integrations.connect").await?;
        let id = id.trim().to_string();
        if id.is_empty() {
            return Err(rpc_invalid_params(
                "integrations.connect needs an integration id to file the configuration under",
            ));
        }
        let token_account = wave2::integration_secret_account(&id);
        let bot_account = wave2::integration_bot_secret_account(&id);
        match config
            .token
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
        {
            Some(token) => self
                .state
                .secrets
                .store(&token_account, token)
                .await
                .map_err(|e| {
                    rpc_error(format!(
                        "the token could not be stored in the OS keychain: {e}"
                    ))
                })?,
            None => self
                .state
                .secrets
                .delete(&token_account)
                .await
                .map_err(|e| rpc_error(format!("the stale token could not be removed: {e}")))?,
        }
        match config
            .bot_token
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
        {
            Some(bot_token) => self
                .state
                .secrets
                .store(&bot_account, bot_token)
                .await
                .map_err(|e| {
                    rpc_error(format!(
                        "the bot token could not be stored in the OS keychain: {e}"
                    ))
                })?,
            None => {
                self.state.secrets.delete(&bot_account).await.map_err(|e| {
                    rpc_error(format!("the stale bot token could not be removed: {e}"))
                })?
            }
        }

        let stored_token = config
            .token
            .as_deref()
            .map(str::trim)
            .is_some_and(|t| !t.is_empty());
        let stored_bot = config
            .bot_token
            .as_deref()
            .map(str::trim)
            .is_some_and(|t| !t.is_empty());
        let webhook = config
            .webhook_url
            .as_deref()
            .map(str::trim)
            .is_some_and(|url| !url.is_empty());
        let redacted = wave2::redact_config(&config);
        let name = match self
            .state
            .settings
            .get_integration(&id)
            .await
            .map_err(rpc_error)?
        {
            Some(row) if !row.name.trim().is_empty() => row.name,
            _ => wave2::integration_name(&id),
        };
        let row = wave2::integration_row(
            &id,
            &name,
            stored_token || stored_bot || webhook,
            &redacted,
            Some(now_epoch_secs()),
        )
        .map_err(rpc_error)?;
        self.state
            .settings
            .save_integration(&row, Some(&row.config_json))
            .await
            .map_err(rpc_error)?;
        self.integration_status_of(&id).await
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
        self.authorize(ext, "integrations.disconnect").await?;
        let row = self.stored_integration(&id).await?;
        for account in [
            wave2::integration_secret_account(&row.id),
            wave2::integration_bot_secret_account(&row.id),
        ] {
            self.state.secrets.delete(&account).await.map_err(|e| {
                rpc_error(format!(
                    "the credential could not be removed from the OS keychain: {e}"
                ))
            })?;
        }
        let config = wave2::integration_config(&row).map_err(rpc_error)?;
        let disconnected =
            wave2::integration_row(&row.id, &row.name, false, &config, row.configured_at)
                .map_err(rpc_error)?;
        self.state
            .settings
            .save_integration(&disconnected, Some(&disconnected.config_json))
            .await
            .map_err(rpc_error)?;
        self.integration_status_of(&row.id).await
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
        self.authorize(ext, "integrations.test").await?;
        let row = self.stored_integration(&id).await?;
        let config = wave2::integration_config(&row).map_err(rpc_error)?;
        let has_token = self
            .state
            .secrets
            .load(&wave2::integration_secret_account(&row.id))
            .await
            .is_ok();
        let has_bot_token = self
            .state
            .secrets
            .load(&wave2::integration_bot_secret_account(&row.id))
            .await
            .is_ok();
        let has_webhook = config
            .webhook_url
            .as_ref()
            .map(|url| !url.trim().is_empty())
            .unwrap_or(false);
        let missing: Vec<&str> = match row.id.as_str() {
            "github" => {
                if !has_token {
                    vec!["an access token"]
                } else {
                    Vec::new()
                }
            }
            "slack" => {
                if !has_webhook && !has_bot_token {
                    vec!["a webhook URL or bot token"]
                } else {
                    Vec::new()
                }
            }
            _ => {
                if !has_token && !has_webhook && !has_bot_token {
                    vec!["an access token, webhook URL, or bot token"]
                } else {
                    Vec::new()
                }
            }
        };
        Ok(IntegrationTestResult {
            status: if missing.is_empty() {
                "ok".to_string()
            } else {
                "unconfigured".to_string()
            },
            message: if missing.is_empty() {
                format!(
                    "`{}` is configured and its credential is filed in the OS keychain; \
                     no live request was made, so this says nothing about the service",
                    row.id
                )
            } else {
                format!(
                    "`{}` is missing {}: integrations.connect writes the configuration, \
                     and no live request is made from here",
                    row.id,
                    missing.join(" or ")
                )
            },
        })
    }

    async fn list_project_files(
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

    async fn read_project_file(
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

    async fn diff_project_file(
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

    async fn get_global_rules(&self, ext: &RpcExtensions) -> RpcResult<ToolRulesReply> {
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

    async fn set_global_rules(
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

    async fn get_project_rules(
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

    async fn set_project_rules(
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
