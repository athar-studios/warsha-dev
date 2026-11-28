//! Agent workflow, handoff, backend-kit, post-launch and routine bodies of the `WarshaApiServer`
//! implementation (ENGINE H1 step 5k). Moved verbatim from `rpc_impl.rs`; the trait methods
//! there delegate to these inherent `*_impl` methods. The crate root items arrive through
//! `use super::*`.

use super::*;

impl WarshaRpc {
    pub(crate) async fn export_backend_impl(
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

    pub(crate) async fn create_handoff_impl(
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

    pub(crate) async fn apply_handoff_impl(
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

    pub(crate) async fn list_crash_reports_impl(
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

    pub(crate) async fn list_store_reviews_impl(
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

    pub(crate) async fn agent_turn_impl(
        &self,
        ext: &RpcExtensions,
        _input: String,
    ) -> RpcResult<AgentTurnResult> {
        self.authorize(ext, "agent.turn").await?;
        Err(ErrorObject::owned(
            -32000,
            "agent.turn is retired: start a run and use agent.turn_run with its run id",
            None::<()>,
        ))
    }

    pub(crate) async fn agent_turn_run_impl(
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

    pub(crate) async fn approve_agent_mode_impl(
        &self,
        ext: &RpcExtensions,
        run_id: Option<RunId>,
    ) -> RpcResult<ModeTransition> {
        self.agent_switch(ext, AgentMode::Agent, run_id).await
    }

    pub(crate) async fn return_agent_to_plan_impl(
        &self,
        ext: &RpcExtensions,
        run_id: Option<RunId>,
    ) -> RpcResult<ModeTransition> {
        self.agent_switch(ext, AgentMode::Plan, run_id).await
    }

    pub(crate) async fn agent_state_impl(
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

    pub(crate) async fn get_backend_impl(
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

    pub(crate) async fn set_backend_impl(
        &self,
        ext: &RpcExtensions,
        run_id: String,
        kit: BackendKit,
    ) -> RpcResult<()> {
        self.authorize(ext, "backend.set").await?;
        let (key, _project_name) = self.project_for_run(&run_id).await;
        self.remember_backend_kit(&key, &kit).await
    }

    pub(crate) async fn list_routines_impl(&self, ext: &RpcExtensions) -> RpcResult<Vec<Routine>> {
        self.authorize(ext, "routines.list").await?;
        let registry = self.state.routines.read().await;
        Ok(registry.list().into_iter().cloned().collect())
    }

    pub(crate) async fn get_routine_impl(
        &self,
        ext: &RpcExtensions,
        id: String,
    ) -> RpcResult<Option<Routine>> {
        self.authorize(ext, "routines.get").await?;
        let registry = self.state.routines.read().await;
        Ok(registry.get(&id).cloned())
    }

    pub(crate) async fn run_routine_now_impl(
        &self,
        ext: &RpcExtensions,
        id: String,
    ) -> RpcResult<Routine> {
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

    pub(crate) async fn pause_routine_impl(
        &self,
        ext: &RpcExtensions,
        id: String,
    ) -> RpcResult<Routine> {
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

    pub(crate) async fn resume_routine_impl(
        &self,
        ext: &RpcExtensions,
        id: String,
    ) -> RpcResult<Routine> {
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
}
