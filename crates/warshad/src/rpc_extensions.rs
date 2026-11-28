//! Extension-domain bodies of the `WarshaApiServer` implementation (ENGINE H1 step 4b):
//! harnesses, skills, plugins, custom models, integrations and coding backends. Moved
//! verbatim from `rpc_impl.rs`; the trait methods there delegate to these inherent
//! `*_impl` methods. The crate root items arrive through `use super::*`.

use super::*;

impl WarshaRpc {
    pub(crate) async fn list_harnesses_impl(
        &self,
        ext: &RpcExtensions,
    ) -> RpcResult<Vec<HarnessDefinition>> {
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

    pub(crate) async fn get_harness_impl(
        &self,
        ext: &RpcExtensions,
        id: String,
    ) -> RpcResult<HarnessDefinition> {
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

    pub(crate) async fn import_harness_impl(
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

    pub(crate) async fn select_harness_impl(
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

    pub(crate) async fn delete_harness_impl(
        &self,
        ext: &RpcExtensions,
        id: String,
    ) -> RpcResult<()> {
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

    pub(crate) async fn list_coding_backends_impl(
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

    pub(crate) async fn test_coding_backend_impl(
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

    pub(crate) async fn get_backend_routing_impl(
        &self,
        ext: &RpcExtensions,
    ) -> RpcResult<crate::runtime::BackendRoutingsConfig> {
        self.authorize(ext, "backend_routing.get").await?;
        let config = load_backend_routings(&self.state.settings).await;
        Ok(config)
    }

    pub(crate) async fn set_backend_routing_impl(
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

    pub(crate) async fn list_skills_impl(
        &self,
        ext: &RpcExtensions,
    ) -> RpcResult<Vec<SkillDefinition>> {
        self.authorize(ext, "skills.list").await?;
        let rows = self.state.settings.list_skills().await.map_err(rpc_error)?;
        let mut skills = Vec::with_capacity(rows.len());
        for row in &rows {
            skills.push(wave2::skill_definition(row).map_err(rpc_error)?);
        }
        Ok(skills)
    }

    pub(crate) async fn import_skill_impl(
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

    pub(crate) async fn toggle_skill_impl(
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

    pub(crate) async fn delete_skill_impl(&self, ext: &RpcExtensions, id: String) -> RpcResult<()> {
        self.authorize(ext, "skills.delete").await?;
        self.stored_skill(&id).await?;
        self.state
            .settings
            .delete_skill(&id)
            .await
            .map_err(rpc_error)?;
        Ok(())
    }

    pub(crate) async fn list_plugins_impl(
        &self,
        ext: &RpcExtensions,
    ) -> RpcResult<Vec<PluginDefinition>> {
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

    pub(crate) async fn install_plugin_impl(
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

    pub(crate) async fn toggle_plugin_impl(
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

    pub(crate) async fn configure_plugin_impl(
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

    pub(crate) async fn remove_plugin_impl(
        &self,
        ext: &RpcExtensions,
        id: String,
    ) -> RpcResult<()> {
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

    pub(crate) async fn list_custom_models_impl(
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

    pub(crate) async fn add_custom_model_impl(
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

    pub(crate) async fn remove_custom_model_impl(
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

    pub(crate) async fn list_integrations_impl(
        &self,
        ext: &RpcExtensions,
    ) -> RpcResult<Vec<IntegrationStatus>> {
        self.authorize(ext, "integrations.list").await?;
        self.integration_statuses().await
    }

    pub(crate) async fn connect_integration_impl(
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

    pub(crate) async fn disconnect_integration_impl(
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

    pub(crate) async fn test_integration_impl(
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
}
