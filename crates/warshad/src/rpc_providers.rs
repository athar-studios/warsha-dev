//! Provider-domain bodies of the `WarshaApiServer` implementation (ENGINE H1 step 3).
//! Moved verbatim from `rpc_impl.rs`; the trait methods there delegate to these inherent
//! `*_impl` methods. The crate root items arrive through `use super::*`.

use super::*;

impl WarshaRpc {
    pub(crate) async fn list_providers_impl(
        &self,
        ext: &RpcExtensions,
    ) -> RpcResult<Vec<ProviderRecord>> {
        self.authorize(ext, "providers.list").await?;
        let stored = self
            .state
            .settings
            .load_providers()
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))?;
        let mut records = Vec::with_capacity(stored.len());
        for provider in stored {
            let has_stored_key = provider_has_stored_key(&self.state.secrets, &provider.name).await;
            let mut record = ProviderRecord::from(provider);
            record.has_stored_key = has_stored_key;
            records.push(record);
        }
        Ok(records)
    }

    pub(crate) async fn provider_auth_status_impl(
        &self,
        ext: &RpcExtensions,
        name: String,
    ) -> RpcResult<AuthStatus> {
        self.authorize(ext, "providers.auth_status").await?;
        let secret = self
            .state
            .secrets
            .load(&format!("integration:{name}"))
            .await;
        if secret.is_ok() {
            Ok(AuthStatus::Authenticated)
        } else {
            Ok(AuthStatus::Unauthenticated)
        }
    }

    pub(crate) async fn provider_auth_start_impl(
        &self,
        ext: &RpcExtensions,
        name: String,
    ) -> RpcResult<AuthStartResponse> {
        self.authorize(ext, "providers.auth_start").await?;
        let stored = self
            .state
            .settings
            .load_provider(&name)
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))?;
        let response = match stored.as_ref().map(|provider| provider.kind.as_str()) {
            // None of the provider kinds currently shipped support a hosted OAuth/PKCE flow;
            // report API-key auth honestly instead of returning a dead placeholder URL.
            Some(_) | None => AuthStartResponse::ApiKey { name },
        };
        Ok(response)
    }

    pub(crate) async fn provider_auth_complete_impl(
        &self,
        ext: &RpcExtensions,
        name: String,
        code: String,
    ) -> RpcResult<AuthCompleteResponse> {
        self.authorize(ext, "providers.auth_complete").await?;
        self.state
            .secrets
            .store(&format!("integration:{name}"), &code)
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))?;
        self.state.providers.write().await.reset_cooldown(&name);
        Ok(AuthCompleteResponse {
            message: "Successfully authenticated".to_string(),
        })
    }

    pub(crate) async fn provider_disconnect_impl(
        &self,
        ext: &RpcExtensions,
        name: String,
    ) -> RpcResult<()> {
        self.authorize(ext, "providers.disconnect").await?;
        self.state
            .secrets
            .delete(&format!("integration:{name}"))
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))?;
        self.state.providers.write().await.reset_cooldown(&name);
        Ok(())
    }

    pub(crate) async fn add_provider_impl(
        &self,
        ext: &RpcExtensions,
        name: String,
        kind: String,
        key: String,
        api_base: Option<String>,
    ) -> RpcResult<ProviderRecord> {
        self.authorize(ext, "providers.add").await?;
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err(provider_refusal("a provider needs a name"));
        }
        if name == MOCK_PROVIDER_ID || name == "mock-503" {
            // The daemon's own offline adapter holds this id, and every automated test
            // routes to it. A row that claimed it would be registered now and skipped
            // at the next start (`register_configured_providers` leaves the id to the
            // mock), which is a provider that works until it quietly does not.
            return Err(provider_refusal(format!(
                "{name} is the daemon's own offline provider; choose another name"
            )));
        }
        let kind = kind.trim().to_string();
        if kind.is_empty() {
            return Err(provider_refusal(format!(
                "provider {name} needs a kind (open_router, deep_seek, kimi, glm, qwen or custom)"
            )));
        }
        let api_base = non_empty(api_base);
        // A pasted key arrives with the whitespace the paste carried; the wizard's
        // own check trims before it judges, so the key that is stored and later sent
        // as a bearer token is the trimmed one and not a key with a newline in it.
        let key = key.trim().to_string();

        // The endpoint has to be known before anything is stored: the kind's
        // default when the caller sent none, and a refusal when there is none.
        let endpoint = api_base
            .clone()
            .or_else(|| default_base_url_for_kind(&kind).map(str::to_string));
        let Some(endpoint) = endpoint else {
            return Err(provider_refusal(format!(
                "no base URL is known for provider kind {kind}; send api_base with the provider"
            )));
        };

        // The two input checks that belong to `add`: the URL parses and the key is
        // usable. The model is not checked here — `add` has no model to check, and
        // `providers.test` is where that field is reported.
        let inputs = ProviderSetupWizard::test_connection(&endpoint, &key, "");
        if let Some(reason) = inputs.url_reason {
            return Err(provider_refusal(reason));
        }
        if let Some(reason) = inputs.key_reason {
            return Err(provider_refusal(reason));
        }

        let store_result =
            tokio::time::timeout(KEYCHAIN_TIMEOUT, self.state.secrets.store(&name, &key)).await;
        match store_result {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                if let Some(fallback) = warsha_providers::fallback_store() {
                    warn!(
                        "storing key in keychain failed ({e}); storing to encrypted fallback store"
                    );
                    fallback.store(&name, &key).await.map_err(|fb_err| {
                        provider_refusal(format!(
                            "the API key could not be stored in the OS keychain ({e}) or fallback store ({fb_err})"
                        ))
                    })?;
                } else {
                    return Err(provider_refusal(format!(
                        "the API key could not be stored in the OS keychain: {e}"
                    )));
                }
            }
            Err(_) => {
                if let Some(fallback) = warsha_providers::fallback_store() {
                    warn!("storing key in keychain timed out; storing to encrypted fallback store");
                    fallback.store(&name, &key).await.map_err(|fb_err| {
                        provider_refusal(format!(
                            "the API key could not be stored in the OS keychain (timed out) or fallback store ({fb_err})"
                        ))
                    })?;
                } else {
                    return Err(provider_refusal(format!(
                        "the API key could not be stored in the OS keychain: timed out after {}s",
                        KEYCHAIN_TIMEOUT.as_secs()
                    )));
                }
            }
        }

        let provider = StoredProvider {
            name: name.clone(),
            kind,
            api_base,
            enabled: true,
            created_at: u64::try_from(Utc::now().timestamp()).unwrap_or_default(),
            last_test_ok: None,
            last_test_at: None,
            last_test_error: None,
        };
        self.state
            .settings
            .save_provider(&provider)
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))?;

        // The row is the owner's half of a provider and the router is the callable half.
        // Registering the deferred adapter here is what makes a provider usable by the
        // next turn rather than only by the daemon after the next restart — and
        // registering calls nothing: the adapter is built on its first use, so a fresh
        // daemon with three configured providers still starts with no network at all.
        {
            let mut router = self.state.providers.write().await;
            router.register_provider(Box::new(DeferredProvider::new(
                provider.clone(),
                Arc::clone(&self.state.provider_factory),
            )));
            router.reset_cooldown(&name);
        }

        info!("provider {name} configured");
        Ok(provider.into())
    }

    pub(crate) async fn test_provider_impl(
        &self,
        ext: &RpcExtensions,
        name: String,
        model: Option<String>,
    ) -> RpcResult<HealthCheck> {
        self.authorize(ext, "providers.test").await?;
        let provider = self
            .state
            .settings
            .load_provider(&name)
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))?
            .ok_or_else(|| provider_refusal(format!("no provider named {name} is configured")))?;

        let tested_at = u64::try_from(Utc::now().timestamp()).unwrap_or_default();

        let Some(endpoint) = provider_endpoint(&provider) else {
            let reason = format!(
                "no base URL is known for provider kind {}; add it again with api_base",
                provider.kind
            );
            self.persist_provider_test_outcome(&name, false, tested_at, Some(&reason))
                .await;
            return Err(provider_refusal(reason));
        };
        let key = match self.state.secrets.load(&name).await {
            Ok(k) => k,
            Err(e) => {
                if let Some(fallback) = warsha_providers::fallback_store() {
                    match fallback.load(&name).await {
                        Ok(k) => k,
                        Err(fb_err) => {
                            let reason = format!(
                                "the API key for {name} could not be read from the OS keychain ({e}) or fallback store ({fb_err})"
                            );
                            self.persist_provider_test_outcome(
                                &name,
                                false,
                                tested_at,
                                Some(&reason),
                            )
                            .await;
                            return Err(provider_refusal(reason));
                        }
                    }
                } else {
                    let reason = format!(
                        "the API key for {name} could not be read from the OS keychain: {e}"
                    );
                    self.persist_provider_test_outcome(&name, false, tested_at, Some(&reason))
                        .await;
                    return Err(provider_refusal(reason));
                }
            }
        };

        // Resolve test probe model per D1:
        // 1. Explicit model param
        // 2. Custom model registered for this provider that is enabled
        // 3. Role routing targeting this provider
        // 4. OneClickProvider default
        // 5. Kind-specific fallback
        let resolved_model = self.resolve_test_model(&provider, model).await;

        // No network yet: a malformed URL or a key the owner typed with a space in
        // it is refused here, from the wizard's own checks.
        if let Some(reason) =
            ProviderSetupWizard::test_connection(&endpoint, &key, &resolved_model).reason()
        {
            let scrubbed = warsha_providers::scrub_key(&reason, &key);
            self.persist_provider_test_outcome(&name, false, tested_at, Some(&scrubbed))
                .await;
            return Err(provider_refusal(scrubbed));
        }

        let mut health = probe_endpoint(&endpoint, &key).await;
        if health.healthy {
            self.persist_provider_test_outcome(&name, true, tested_at, None)
                .await;
            self.state.providers.write().await.reset_cooldown(&name);
            return Ok(health);
        }

        let raw_reason = health
            .error
            .clone()
            .unwrap_or_else(|| "the connection test failed".to_string());
        let reason = warsha_providers::scrub_key(&raw_reason, &key);
        health.error = Some(reason.clone());
        self.persist_provider_test_outcome(&name, false, tested_at, Some(&reason))
            .await;
        warn!("provider {name} failed its connection test: {reason}");
        Err(ErrorObject::owned(-32000, reason, Some(health)))
    }

    pub(crate) async fn check_provider_models_impl(
        &self,
        ext: &RpcExtensions,
        name: String,
        api_base: Option<String>,
        key: Option<String>,
        kind: Option<String>,
    ) -> RpcResult<warsha_providers::ModelCatalogDiff> {
        self.authorize(ext, "providers.check_models").await?;

        let api_base = api_base.filter(|s| !s.trim().is_empty()).or_else(|| {
            kind.as_deref()
                .filter(|s| !s.trim().is_empty())
                .and_then(default_base_url_for_kind)
                .map(str::to_string)
        });
        let key = key.filter(|s| !s.trim().is_empty());

        if let (Some(url), Some(k)) = (&api_base, &key) {
            let discovered = warsha_providers::probe_models(url, k).await.map_err(|e| {
                let sanitized =
                    warsha_core::mask_secrets(&warsha_providers::scrub_key(&e.to_string(), k));
                rpc_error(sanitized)
            })?;
            return Ok(warsha_providers::reconcile_model_catalog(
                &name,
                &[],
                discovered,
            ));
        }

        let current_catalog = self
            .state
            .settings
            .list_custom_models()
            .await
            .unwrap_or_default()
            .into_iter()
            .filter(|m| m.provider.eq_ignore_ascii_case(&name))
            .map(|m| warsha_providers::ModelInfo {
                id: m.model_id.clone(),
                name: m.display_name.unwrap_or(m.model_id),
                provider: m.provider,
                context_window: m.context_window.unwrap_or(0),
                max_output: 0,
                pricing: warsha_core::ModelPricing {
                    input_per_million_cents: 0,
                    output_per_million_cents: 0,
                },
            })
            .collect::<Vec<_>>();

        // A handle rather than a lock held across the catalog call (audit N-3): this
        // reaches the provider over the network, and a read guard kept through it would
        // make an owner's `providers.add` wait for a round trip it has nothing to do with.
        let router = {
            let guard = self.state.providers.read().await;
            (*guard).clone()
        };

        let stored_key = self.state.secrets.load(&name).await.ok();

        router
            .check_model_updates(&name, &current_catalog)
            .await
            .map_err(|e| {
                let err_str = e.to_string();
                let scrubbed = if let Some(ref k) = stored_key {
                    warsha_providers::scrub_key(&err_str, k)
                } else {
                    err_str
                };
                let sanitized = warsha_core::mask_secrets(&scrubbed);
                rpc_error(sanitized)
            })
    }

    pub(crate) async fn provider_quota_impl(
        &self,
        ext: &RpcExtensions,
        name: String,
    ) -> RpcResult<ProviderQuotaStatus> {
        self.authorize(ext, "providers.quota").await?;
        let kind = self
            .state
            .settings
            .load_providers()
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))?
            .into_iter()
            .find(|p| p.name == name)
            .map(|p| p.kind)
            .ok_or_else(|| rpc_error(format!("provider `{name}` is not configured")))?;
        // A handle rather than a lock held across the quota call (audit N-3): the
        // adapter may reach the network, and a read guard kept through it would make
        // an owner's `providers.add` wait for a round trip it has nothing to do with.
        let router = {
            let guard = self.state.providers.read().await;
            (*guard).clone()
        };
        let provider = router
            .get_provider(&name)
            .ok_or_else(|| rpc_error(format!("provider `{name}` is not registered")))?;
        let quota = provider.get_quota().await.unwrap_or(ProviderQuota::Unknown);
        // The local estimate from the daemon's own cost records: spend against the
        // recorded per-run ceiling is not a provider quota, so a quota the provider
        // does not expose stays Unknown rather than wearing an invented limit.
        let state = match quota {
            ProviderQuota::Reported { used, limit } => QuotaState::Reported { used, limit },
            ProviderQuota::Estimated { usage_percentage } => {
                QuotaState::Estimated { usage_percentage }
            }
            ProviderQuota::Unknown => QuotaState::Unknown,
        };
        Ok(ProviderQuotaStatus {
            provider: name,
            kind,
            quota: state,
            checked_at: Some(Utc::now().to_rfc3339()),
        })
    }

    pub(crate) async fn remove_provider_impl(
        &self,
        ext: &RpcExtensions,
        name: String,
    ) -> RpcResult<ProviderRecord> {
        self.authorize(ext, "providers.remove").await?;
        let provider = self
            .state
            .settings
            .load_provider(&name)
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))?
            .ok_or_else(|| provider_refusal(format!("no provider named {name} is configured")))?;

        // The keychain goes first, and if that fails or times out, the fallback store
        // is cleaned up as well.
        let delete_res = self.state.secrets.delete(&name).await;
        if let Err(e) = delete_res {
            if let Some(fallback) = warsha_providers::fallback_store() {
                fallback.delete(&name).await.map_err(|fb_err| {
                    provider_refusal(format!(
                        "the API key could not be removed from the OS keychain ({e}) or fallback store ({fb_err})"
                    ))
                })?;
            } else {
                return Err(provider_refusal(format!(
                    "the API key could not be removed from the OS keychain: {e}"
                )));
            }
        } else if let Some(fallback) = warsha_providers::fallback_store() {
            let _ = fallback.delete(&name).await;
        }
        self.state
            .settings
            .delete_provider(&name)
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))?;

        // The router may already hold a built adapter with this provider's key inside
        // its request headers, and a row that is gone does not take that back. Putting a
        // refusal under the same id is what makes removal a removal rather than the
        // deletion of the record that it happened.
        {
            let mut router = self.state.providers.write().await;
            router.register_provider(Box::new(RemovedProvider::new(&name)));
            router.reset_cooldown(&name);
        }

        // Routings that name it go too, written through in the same order `roles.set`
        // uses: stored first, then honoured. Leaving them would leave a role whose every
        // call fails with a provider the owner can no longer see in `providers.list`.
        let (table, dropped) = {
            let current = self.state.role_routings.read().await;
            let mut table: HashMap<Role, RoleRoutingConfig> = HashMap::clone(&current);
            let mut dropped: Vec<String> = Vec::new();
            table.retain(|_, config| {
                let names = config
                    .targets
                    .iter()
                    .any(|target| target.provider_id.trim() == name);
                if names {
                    dropped.push(config.role.clone());
                }
                !names
            });
            (table, dropped)
        };
        if !dropped.is_empty() {
            store_role_routings(&self.state.settings, &table)
                .await
                .map_err(|error| {
                    rpc_error(format!(
                        "the provider is removed but its routings could not be rewritten: {error}"
                    ))
                })?;
            *self.state.role_routings.write().await = table;
            info!(
                "routings for {} role(s) dropped with provider {name}",
                dropped.len()
            );
        }

        info!("provider {name} removed");
        Ok(provider.into())
    }
}
