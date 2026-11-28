//! Run, session, gate and artifact bodies of the `WarshaApiServer` implementation (ENGINE H1 step 5j).
//! Moved verbatim from `rpc_impl.rs`; the trait methods there delegate to these inherent
//! `*_impl` methods. The crate root items arrive through `use super::*`.

use super::*;

impl WarshaRpc {
    pub(crate) async fn start_run_impl(
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

    pub(crate) async fn run_worktree_impl(
        &self,
        ext: &RpcExtensions,
        run_id: RunId,
    ) -> RpcResult<Option<RunWorktree>> {
        self.authorize(ext, "runs.worktree").await?;
        Ok(self.state.run_worktrees.read().await.get(&run_id).cloned())
    }

    pub(crate) async fn gate_evidence_impl(
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

    pub(crate) async fn get_run_impl(&self, ext: &RpcExtensions, run_id: RunId) -> RpcResult<Run> {
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

    pub(crate) async fn list_runs_impl(
        &self,
        ext: &RpcExtensions,
        project_id: ProjectId,
    ) -> RpcResult<Vec<Run>> {
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

    pub(crate) async fn get_events_impl(
        &self,
        ext: &RpcExtensions,
        run_id: RunId,
    ) -> RpcResult<Vec<Event>> {
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

    pub(crate) async fn list_sessions_impl(
        &self,
        ext: &RpcExtensions,
        run_id: RunId,
    ) -> RpcResult<Vec<Session>> {
        self.authorize(ext, "sessions.list").await?;
        self.state
            .store
            .list_sessions_by_run(run_id)
            .await
            .map_err(rpc_error)
    }

    pub(crate) async fn list_run_subtasks_impl(
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

    pub(crate) async fn ask_start_impl(
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

    pub(crate) async fn ask_turn_impl(
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

    pub(crate) async fn ask_status_impl(
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

    pub(crate) async fn session_tail_impl(
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

    pub(crate) async fn session_stop_impl(
        &self,
        ext: &RpcExtensions,
        session_id: SessionId,
    ) -> RpcResult<()> {
        self.authorize(ext, "sessions.stop").await?;
        self.state
            .pipeline
            .live_sessions
            .stop_session(session_id, &self.state.store)
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))
    }

    pub(crate) async fn approve_gate_impl(
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

    pub(crate) async fn reject_gate_impl(
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

    pub(crate) async fn request_changes_impl(
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

    pub(crate) async fn list_gates_impl(
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

    pub(crate) async fn subscribe_run_impl(
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

    pub(crate) async fn advance_run_impl(
        &self,
        ext: &RpcExtensions,
        run_id: RunId,
    ) -> RpcResult<Run> {
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

    pub(crate) async fn cancel_run_impl(
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

    pub(crate) async fn resume_run_impl(
        &self,
        ext: &RpcExtensions,
        run_id: RunId,
    ) -> RpcResult<Run> {
        self.authorize(ext, "runs.resume").await?;
        self.state
            .pipeline
            .resume_run(run_id)
            .await
            .map_err(rpc_error)
    }

    pub(crate) async fn followup_run_impl(
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

    pub(crate) async fn list_run_artifacts_impl(
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
}
