//! Project, permission, swarm-setting and secretary bodies of the `WarshaApiServer` implementation (ENGINE H1 step 5h).
//! Moved verbatim from `rpc_impl.rs`; the trait methods there delegate to these inherent
//! `*_impl` methods. The crate root items arrive through `use super::*`.

use super::*;

impl WarshaRpc {
    pub(crate) async fn health_impl(&self, ext: &RpcExtensions) -> RpcResult<HealthResponse> {
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

    pub(crate) async fn list_projects_impl(&self, ext: &RpcExtensions) -> RpcResult<Vec<Project>> {
        self.authorize(ext, "projects.list").await?;
        self.state
            .store
            .list_projects()
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn create_project_impl(
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

    pub(crate) async fn project_github_status_impl(
        &self,
        ext: &RpcExtensions,
    ) -> RpcResult<GithubStatus> {
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

    pub(crate) async fn prepare_project_folder_impl(
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

    pub(crate) async fn prepare_existing_folder_impl(
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

    pub(crate) async fn project_repo_status_impl(
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

    pub(crate) async fn clone_project_repo_impl(
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

    pub(crate) async fn create_github_repo_impl(
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

    pub(crate) async fn get_default_permissions_impl(
        &self,
        ext: &RpcExtensions,
    ) -> RpcResult<ProjectPermissions> {
        self.authorize(ext, "permissions.defaults.get").await?;
        self.state
            .store
            .get_default_permissions()
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))
    }

    pub(crate) async fn set_default_permissions_impl(
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

    pub(crate) async fn get_project_permissions_impl(
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

    pub(crate) async fn set_project_permissions_impl(
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

    pub(crate) async fn get_swarm_settings_impl(
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

    pub(crate) async fn set_swarm_settings_impl(
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

    pub(crate) async fn get_project_impl(
        &self,
        ext: &RpcExtensions,
        id: ProjectId,
    ) -> RpcResult<Project> {
        self.authorize(ext, "projects.get").await?;
        self.state
            .store
            .get_project(id)
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))
    }

    pub(crate) async fn attach_project_impl(
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

    pub(crate) async fn estimate_run_impl(
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

    pub(crate) async fn interview_start_impl(
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

    pub(crate) async fn interview_turn_impl(
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

    pub(crate) async fn interview_status_impl(
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

    pub(crate) async fn secretary_thread_list_impl(
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

    #[allow(clippy::needless_question_mark)]
    pub(crate) async fn secretary_thread_turns_impl(
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

    pub(crate) async fn secretary_chat_impl(
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

    pub(crate) async fn secretary_provisioning_list_impl(
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
}
