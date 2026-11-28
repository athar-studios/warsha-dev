//! Roles, usage, device and budget bodies of the `WarshaApiServer` implementation (ENGINE H1 step 4a).
//! Moved verbatim from `rpc_impl.rs`; the trait methods there delegate to these inherent
//! `*_impl` methods. The crate root items arrive through `use super::*`.

use super::*;

impl WarshaRpc {
    pub(crate) async fn usage_by_dimension_impl(
        &self,
        ext: &RpcExtensions,
        dimension: String,
        project_id: Option<ProjectId>,
    ) -> RpcResult<Vec<UsageRow>> {
        self.authorize(ext, "usage.by_dimension").await?;
        let tokens = self
            .state
            .store
            .aggregate_tokens_by(&dimension, "%Y-%m-%d", project_id)
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))?;
        let costs = self
            .state
            .store
            .aggregate_costs_by(&dimension, project_id)
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))?;
        // One row per (dimension, key): the cost map answers the cents, the token map
        // answers the token sums and the request count. A key in one map but not the
        // other is still a row — zeros where the other aggregate has nothing.
        let mut keys: Vec<String> = costs.keys().cloned().collect();
        for (dim, _) in tokens.keys() {
            keys.push(dim.clone());
        }
        keys.sort();
        keys.dedup();
        Ok(keys
            .into_iter()
            .map(|key| {
                let cost_cents = costs.get(&key).copied().unwrap_or(0);
                let tokens_row = tokens
                    .iter()
                    .find(|((dim, _), _)| dim == &key)
                    .map(|(_, sums)| *sums);
                match tokens_row {
                    Some((prompt, completion, cache_read, cache_write, requests)) => UsageRow {
                        dimension: dimension.clone(),
                        key,
                        cost_cents,
                        prompt_tokens: prompt,
                        completion_tokens: completion,
                        cache_read_tokens: cache_read,
                        cache_write_tokens: cache_write,
                        requests,
                    },
                    None => UsageRow {
                        dimension: dimension.clone(),
                        key,
                        cost_cents,
                        prompt_tokens: 0,
                        completion_tokens: 0,
                        cache_read_tokens: 0,
                        cache_write_tokens: 0,
                        requests: 0,
                    },
                }
            })
            .collect())
    }

    pub(crate) async fn usage_by_time_impl(
        &self,
        ext: &RpcExtensions,
        granularity: String,
        project_id: Option<ProjectId>,
    ) -> RpcResult<Vec<UsageTimeRow>> {
        self.authorize(ext, "usage.by_time").await?;
        let time_format = match granularity.as_str() {
            "day" => "%Y-%m-%d",
            "month" => "%Y-%m",
            other => {
                return Err(rpc_error(format!(
                    "granularity `{other}` is not `day` or `month`"
                )));
            }
        };
        let tokens = self
            .state
            .store
            .aggregate_tokens_by("project_id", time_format, project_id)
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))?;
        // The token aggregate groups by (project, period): every project's rows are
        // summed here into one row per period, which is what the usage page's chart
        // reads. A project filter already narrows the query, so the merge is only for
        // the unfiltered case.
        let mut periods: BTreeMap<String, (u64, u64, u64, u64, u64)> = BTreeMap::new();
        for ((_, period), sums) in tokens {
            let entry = periods.entry(period).or_insert((0, 0, 0, 0, 0));
            entry.0 += sums.0;
            entry.1 += sums.1;
            entry.2 += sums.2;
            entry.3 += sums.3;
            entry.4 += sums.4;
        }
        let cost_rows = self
            .state
            .store
            .aggregate_costs_by_time(time_format, project_id)
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))?;
        Ok(periods
            .into_iter()
            .map(
                |(period, (prompt, completion, cache_read, cache_write, requests))| UsageTimeRow {
                    cost_cents: cost_rows.get(&period).copied().unwrap_or(0),
                    period,
                    prompt_tokens: prompt,
                    completion_tokens: completion,
                    cache_read_tokens: cache_read,
                    cache_write_tokens: cache_write,
                    requests,
                },
            )
            .collect())
    }

    pub(crate) async fn usage_subscription_window_impl(
        &self,
        ext: &RpcExtensions,
    ) -> RpcResult<SubscriptionWindow> {
        self.authorize(ext, "usage.subscription_window").await?;
        let (five_hour_requests, five_hour_cost) = self
            .state
            .store
            .costs_in_window("5h")
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))?;
        let (seven_day_requests, seven_day_cost) = self
            .state
            .store
            .costs_in_window("7d")
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))?;
        Ok(SubscriptionWindow {
            five_hour: WindowUsage {
                cost_cents: five_hour_cost,
                requests: five_hour_requests,
            },
            seven_day: WindowUsage {
                cost_cents: seven_day_cost,
                requests: seven_day_requests,
            },
        })
    }

    pub(crate) async fn list_devices_impl(
        &self,
        ext: &RpcExtensions,
    ) -> RpcResult<Vec<warsha_remote::DeviceIdentity>> {
        self.authorize(ext, "devices.list").await?;
        Ok(self.state.devices.list_devices().await)
    }

    pub(crate) async fn pair_device_impl(
        &self,
        ext: &RpcExtensions,
        pairing_data: warsha_remote::PairingData,
    ) -> RpcResult<()> {
        // Owner-only (audit H2): a paired device must not be able to enrol another.
        self.authorize(ext, "devices.pair").await?;

        // A pairing consumes an offer, and the offer's deadline is the one the daemon
        // enforces from then on (audit H1). Refuse a stale one before writing anything,
        // so an expired QR leaves neither a device row nor a pairing record behind.
        if pairing_data.is_expired() {
            return Err(rpc_error(
                "this pairing has expired; mint a fresh one from the daemon's pairing \
                 screen (`devices.pair-start`) or with `warsha pair`"
                    .to_string(),
            ));
        }
        let expires_at = u64::try_from(pairing_data.expires_at.timestamp())
            .map_err(|_| rpc_error("this pairing's expiry is before the Unix epoch".to_string()))?;

        let device = warsha_remote::DeviceIdentity {
            device_id: pairing_data.device_id,
            static_public: pairing_data.static_public,
            name: "Paired Device".to_string(),
            created_at: Utc::now(),
            revoked: false,
            revoked_at: None,
        };
        // The registry is rebuilt from the `devices` table on the next start, so the
        // row has to land before the in-memory copy: a crash between the two leaves a
        // device the owner can still see, never one they paired and then lost.
        self.persist_device(&device).await?;
        self.state
            .devices
            .add_device(device)
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))?;

        {
            let mut pairings = self.state.pairings.write().await;
            if pairings.get(&pairing_data.device_id.to_string()).is_none() {
                pairings
                    .create_with_expiry(
                        &pairing_data.device_id.to_string(),
                        &pairing_data.secret_token(),
                        now_epoch_secs(),
                        Some(expires_at),
                    )
                    .map_err(|e| rpc_error(e.to_string()))?;
            }
        }
        self.save_pairings().await?;
        Ok(())
    }

    pub(crate) async fn start_pairing_impl(
        &self,
        ext: &RpcExtensions,
    ) -> RpcResult<warsha_remote::PairingOffer> {
        // Owner-only, deliberately (see `crates/warshad/src/auth.rs`): this is the method
        // that mints the credential the daemon will later trust a caller as, so a caller
        // that is already only a paired device must not be able to hand out a second one.
        self.authorize(ext, "devices.pair-start").await?;

        // The address a phone is sent to, taken from the socket the daemon actually bound
        // and not from the configuration that asked for it: a link may never advertise an
        // address the daemon is not listening on, which is the one way parts A and B of
        // this change could disagree and produce a pairing that cannot be used.
        let server_origin = self.state.pairing_origin.get().cloned().ok_or_else(|| {
            rpc_error(
                "this daemon has no resolved listening origin to advertise yet; a pairing link \
                 would send a phone to an address it cannot reach. If the daemon is bound to a \
                 wildcard address (0.0.0.0), set `advertised_host` to the address your phone \
                 reaches this machine at"
                    .to_string(),
            )
        })?;
        let loopback_only = origin_is_loopback(&server_origin);

        let device = warsha_remote::DeviceIdentity::generate("Pairing in progress".to_string())
            .map_err(|error| rpc_error(error.to_string()))?;
        // `relay_url` is `None` on purpose. The daemon keeps a `RunModeConfig` per mode and
        // no single one of them is "the mode this daemon serves" — there is no active-mode
        // state to read — and the relay path carries encrypted frames rather than this RPC
        // surface, so naming one would be a guess about where the phone connects. The
        // origin in the pairing link is what a phone dials today; see DECISIONS/PLAN for the
        // relay as its own milestone.
        let offer = warsha_remote::PairingOffer::new(&device, &server_origin, None, loopback_only)
            .map_err(|error| rpc_error(error.to_string()))?;
        // The stored pairing dies with the offer: the same deadline the QR carries is
        // written beside the secret, so a restart cannot turn a 10-minute credential
        // into a permanent one (audit H1).
        let expires_at = u64::try_from(offer.expires_at().timestamp())
            .map_err(|_| rpc_error("this pairing's expiry is before the Unix epoch".to_string()))?;

        // The secret itself is never logged: the origin and the expiry are what an owner
        // and a reviewer need, and a pairing line in a log is otherwise a bearer token in
        // a file that gets shipped to support (audit N-1). The identity is persisted
        // first for the same reason `devices.pair` persists it: a restart must still
        // list the device the offer enrolled.
        self.persist_device(&device).await?;
        self.state
            .devices
            .add_device(device.clone())
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))?;

        {
            let mut pairings = self.state.pairings.write().await;
            pairings
                .create_with_expiry(
                    &device.device_id.to_string(),
                    &offer.pairing.secret_token(),
                    now_epoch_secs(),
                    Some(expires_at),
                )
                .map_err(|error| rpc_error(error.to_string()))?;
        }
        self.save_pairings().await?;

        info!(
            "minted a pairing for device {} at {server_origin}, valid until {}",
            offer.pairing.device_id,
            offer.expires_at().to_rfc3339()
        );
        Ok(offer)
    }

    pub(crate) async fn revoke_device_impl(
        &self,
        ext: &RpcExtensions,
        device_id: DeviceId,
    ) -> RpcResult<()> {
        self.authorize(ext, "devices.revoke").await?;
        // Persist the revocation before the registry flips, so a restart cannot
        // resurrect a device the owner already cut off. The row is read back from
        // the registry first: a device that was never paired has nothing to revoke.
        let mut device = self
            .state
            .devices
            .get_device(device_id)
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))?;
        device.revoked = true;
        device.revoked_at = Some(Utc::now());
        self.persist_device(&device).await?;
        self.state
            .devices
            .revoke_device(device_id)
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))?;

        {
            let mut pairings = self.state.pairings.write().await;
            pairings
                .revoke(&device_id.to_string())
                .map_err(|e| rpc_error(e.to_string()))?;
        }
        self.save_pairings().await?;
        Ok(())
    }

    pub(crate) async fn list_roles_impl(
        &self,
        ext: &RpcExtensions,
    ) -> RpcResult<Vec<RoleRoutingConfig>> {
        self.authorize(ext, "roles.list").await?;
        let routings = self.state.role_routings.read().await;
        let mut configs: Vec<RoleRoutingConfig> = routings.values().cloned().collect();
        configs.sort_by(|left, right| left.role.cmp(&right.role));
        Ok(configs)
    }

    pub(crate) async fn set_role_impl(
        &self,
        ext: &RpcExtensions,
        config: RoleRoutingConfig,
    ) -> RpcResult<()> {
        self.authorize(ext, "roles.set").await?;
        let (role, routing) = routing_for(&config).map_err(rpc_error)?;
        // Normalised through the core routing, so what is stored is what was honoured:
        // a caller who sent `INTAKE` or a target with a trailing space gets the same
        // table back from `roles.list` as the router is running.
        let mut stored = routing_config(role, &routing);
        stored.daily_spend_cap_cents = config.daily_spend_cap_cents;

        for target in &stored.targets {
            if target.provider_id == MOCK_PROVIDER_ID
                || target.provider_id == "mock-503"
                || target.provider_id == "fake"
            {
                continue;
            }
            let configured = self
                .state
                .settings
                .load_provider(&target.provider_id)
                .await
                .map_err(|error| rpc_error(error.to_string()))?;
            if configured.is_none() {
                return Err(rpc_policy_refused(format!(
                    "role {} names provider {}, which is not configured",
                    stored.role, target.provider_id
                )));
            }
        }

        // Order is the whole of the durability story: the table is written to the
        // daemon's settings *before* the router is told, so a settings write that fails
        // changes nothing at all — no live routing that a restart forgets, and no
        // stored routing the running daemon does not honour.
        let table = {
            let current = self.state.role_routings.read().await;
            let mut table: HashMap<Role, RoleRoutingConfig> = HashMap::clone(&current);
            table.insert(role, stored.clone());
            table
        };
        store_role_routings(&self.state.settings, &table)
            .await
            .map_err(|error| {
                rpc_error(format!(
                    "the routing for {} could not be stored, so nothing was changed: {error}",
                    stored.role
                ))
            })?;

        // The router holds the adapter a routing names by the time the routing exists,
        // because a provider added by `providers.add` registered its adapter then. The
        // agent role's per-run ceiling, when the owner named one, is installed against
        // the key agent turns are metered under: the router checks it before every
        // attempt, so the cap written here is the cap that stops a paid call (H-07).
        //
        // A routing that names no ceiling replaces the old one with a ceiling that cannot
        // be reached rather than leaving the previous cap in place: the router has no way
        // to forget a budget, and a silently retained cap is a setting the owner removed
        // and the daemon still enforces.
        {
            let mut router = self.state.providers.write().await;
            if role == AGENT_ROLE {
                let cents = stored.budget_cents_per_run.unwrap_or(u64::MAX);
                router.set_budget(AGENT_METER_KEY, cents);
            }
            router.set_role_routing(routing);
        }
        *self.state.role_routings.write().await = table;

        info!("role {} routed", stored.role);
        Ok(())
    }

    pub(crate) async fn show_budget_impl(
        &self,
        ext: &RpcExtensions,
        project_id: ProjectId,
    ) -> RpcResult<BudgetStatus> {
        self.authorize(ext, "budget.show").await?;
        self.state
            .store
            .get_project(project_id)
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))?;
        let runs = self
            .state
            .store
            .list_runs(project_id)
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))?;
        if let Some(run) = runs.first() {
            Ok(BudgetStatus {
                project_id,
                budget_cents: run.budget_cents,
                spent_cents: run.spent_cents,
                remaining_cents: run.budget_cents.map(|b| b.saturating_sub(run.spent_cents)),
            })
        } else {
            let pending_cents = PENDING_PROJECT_BUDGETS
                .read()
                .ok()
                .and_then(|p| p.get(&project_id).copied());
            Ok(BudgetStatus {
                project_id,
                budget_cents: pending_cents,
                spent_cents: 0,
                remaining_cents: pending_cents,
            })
        }
    }

    pub(crate) async fn set_budget_impl(
        &self,
        ext: &RpcExtensions,
        project_id: ProjectId,
        cents: u64,
    ) -> RpcResult<()> {
        self.authorize(ext, "budget.set").await?;
        self.state
            .store
            .get_project(project_id)
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))?;
        let runs = self
            .state
            .store
            .list_runs(project_id)
            .await
            .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))?;
        if let Some(run) = runs.first() {
            self.state
                .pipeline
                .set_run_budget(run.id, cents)
                .await
                .map_err(|e| ErrorObject::owned(-32000, e.to_string(), None::<()>))?;
            if let Ok(mut pending) = PENDING_PROJECT_BUDGETS.write() {
                pending.remove(&project_id);
            }
        } else if let Ok(mut pending) = PENDING_PROJECT_BUDGETS.write() {
            pending.insert(project_id, cents);
        }
        Ok(())
    }

    pub(crate) async fn list_pairings_impl(
        &self,
        ext: &RpcExtensions,
    ) -> RpcResult<Vec<PhonePairing>> {
        self.authorize(ext, "pairing.list").await?;
        Ok(self.state.pairings.read().await.list())
    }

    pub(crate) async fn create_pairing_impl(
        &self,
        ext: &RpcExtensions,
        device_id: String,
        secret: String,
    ) -> RpcResult<PhonePairing> {
        self.authorize(ext, "pairing.create").await?;
        let pairing = {
            let mut pairings = self.state.pairings.write().await;
            pairings
                .create(&device_id, &secret, now_epoch_secs())
                .map_err(|error| rpc_error(error.to_string()))?
        };
        self.save_pairings().await?;
        Ok(pairing)
    }

    pub(crate) async fn revoke_pairing_impl(
        &self,
        ext: &RpcExtensions,
        device_id: String,
    ) -> RpcResult<PhonePairing> {
        self.authorize(ext, "pairing.revoke").await?;
        let pairing = {
            let mut pairings = self.state.pairings.write().await;
            pairings
                .revoke(&device_id)
                .map_err(|error| rpc_error(error.to_string()))?
        };
        self.save_pairings().await?;
        Ok(pairing)
    }
}
