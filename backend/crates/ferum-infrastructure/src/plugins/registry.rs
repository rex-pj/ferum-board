use std::collections::HashMap;
use std::sync::{Arc, Mutex};
#[cfg(feature = "script_plugins")]
use std::time::Duration;

use async_trait::async_trait;
use uuid::Uuid;

use super::circuit_breaker::CircuitBreaker;
use ferum_application::ports::{
    CacheService, HookContext, HookDecision, PluginHookRuntime, PluginLifecycle, PluginRpcRuntime,
    PluginUiRuntime, UiSlotEntry,
};
use ferum_application::shared::AppError;
use ferum_domain::models::plugin::{PluginStatus, PluginTier};
use ferum_domain::repositories::notification_repository::NotificationRepository;
use ferum_domain::repositories::plugin_db_repository::PluginDbGateway;
use ferum_domain::repositories::plugin_repository::PluginRepository;
use ferum_domain::repositories::plugin_storage_repository::PluginStorageRepository;
use ferum_domain::repositories::user_repository::UserRepository;

#[cfg(feature = "script_plugins")]
use super::script_runtime::ScriptPluginRuntime;

/// Entry in the in-memory dispatch table — one per (plugin, hook_name).
struct HookEntry {
    plugin_id: Uuid,
    hook_id: Uuid,
    plugin_slug: String,
    tier: PluginTier,
    circuit_breaker: Arc<CircuitBreaker>,
}

/// Concrete implementation of PluginRuntime.
/// Maintains an in-memory dispatch table and per-plugin Script runtimes.
pub struct PluginRegistry {
    plugin_repo: Arc<dyn PluginRepository>,
    /// hook_name → ordered Vec<HookEntry> (by priority ASC)
    dispatch_table: Mutex<HashMap<String, Vec<HookEntry>>>,
    circuit_threshold: u32,
    /// Max ms allowed for a before-hook (enforced for Tier 2 Script plugins)
    #[cfg(feature = "script_plugins")]
    hook_timeout_ms: u64,
    /// Cache service — passed to Script plugin runtimes for Ferum.cache.* ops
    #[cfg(feature = "script_plugins")]
    cache: Arc<dyn CacheService>,
    /// Durable storage — passed to Script plugin runtimes for Ferum.storage.* ops
    #[cfg(feature = "script_plugins")]
    plugin_storage: Arc<dyn PluginStorageRepository>,
    /// Passed to Script plugin runtimes for the curated Ferum.forum.* API
    #[cfg(feature = "script_plugins")]
    user_repo: Arc<dyn UserRepository>,
    #[cfg(feature = "script_plugins")]
    notification_repo: Arc<dyn NotificationRepository>,
    /// Scoped Postgres access to each plugin's own `plugin_{slug}` schema
    #[cfg(feature = "script_plugins")]
    db_gateway: Arc<dyn PluginDbGateway>,
    /// Tier 2 — one JS runtime per active Script plugin
    #[cfg(feature = "script_plugins")]
    script_runtimes: dashmap::DashMap<Uuid, Arc<ScriptPluginRuntime>>,
}

impl PluginRegistry {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        plugin_repo: Arc<dyn PluginRepository>,
        cache: Arc<dyn CacheService>,
        plugin_storage: Arc<dyn PluginStorageRepository>,
        user_repo: Arc<dyn UserRepository>,
        notification_repo: Arc<dyn NotificationRepository>,
        db_gateway: Arc<dyn PluginDbGateway>,
        hook_timeout_ms: u64,
        circuit_threshold: u32,
    ) -> Self {
        #[cfg(not(feature = "script_plugins"))]
        let _ = (cache, plugin_storage, user_repo, notification_repo, db_gateway, hook_timeout_ms);

        Self {
            plugin_repo,
            dispatch_table: Mutex::new(HashMap::new()),
            circuit_threshold,
            #[cfg(feature = "script_plugins")]
            hook_timeout_ms,
            #[cfg(feature = "script_plugins")]
            cache,
            #[cfg(feature = "script_plugins")]
            plugin_storage,
            #[cfg(feature = "script_plugins")]
            user_repo,
            #[cfg(feature = "script_plugins")]
            notification_repo,
            #[cfg(feature = "script_plugins")]
            db_gateway,
            #[cfg(feature = "script_plugins")]
            script_runtimes: dashmap::DashMap::new(),
        }
    }

    /// Load all active plugins from DB at startup.
    pub async fn load_from_db(&self) -> Result<(), AppError> {
        let plugins = self.plugin_repo.list().await?;
        let mut table: HashMap<String, Vec<HookEntry>> = HashMap::new();

        for plugin in plugins.iter().filter(|p| p.status == PluginStatus::Active) {
            let hooks = self.plugin_repo.hooks_for_plugin(plugin.id).await?;
            for hook in hooks.iter().filter(|h| h.is_active) {
                table.entry(hook.hook_name.clone()).or_default().push(HookEntry {
                    plugin_id: plugin.id,
                    hook_id: hook.id,
                    plugin_slug: plugin.slug.clone(),
                    tier: plugin.tier.clone(),
                    // Circuit breaker always starts fresh — in-memory only.
                    // plugin.circuit_open in DB is admin-visibility only and resets on restart.
                    circuit_breaker: Arc::new(CircuitBreaker::new(
                        self.circuit_threshold,
                        3600,
                        300,
                    )),
                });
            }

            // Start Script runtime for active Tier 2 plugins that declare server-side
            // hooks or RPC actions. Plugins with neither are UI-slot-only; their
            // bundle.js is browser code and must not be evaluated in boa_engine.
            #[cfg(feature = "script_plugins")]
            if plugin.tier == PluginTier::Script && Self::manifest_needs_script_runtime(&plugin.manifest) {
                match self.init_script_runtime(plugin) {
                    Ok(rt) => {
                        self.script_runtimes.insert(plugin.id, Arc::new(rt));
                    }
                    Err(e) => {
                        tracing::error!(
                            plugin = %plugin.slug,
                            error = ?e,
                            "Failed to start script runtime at startup"
                        );
                    }
                }
            }
        }

        *self.dispatch_table.lock().unwrap() = table;
        tracing::info!("Plugin registry loaded from DB");
        Ok(())
    }

    // ─── Script runtime helpers ───────────────────────────────────────────────────

    /// Returns true only when the manifest declares at least one server-side hook
    /// or RPC action — either requires the boa_engine runtime to be running.
    /// Plugins with neither are UI-slot-only and must not be executed in
    /// boa_engine — their bundle.js is browser-only code.
    #[cfg(feature = "script_plugins")]
    fn manifest_needs_script_runtime(manifest: &serde_json::Value) -> bool {
        let has_entries = |field: &str| {
            manifest
                .get("capabilities")
                .and_then(|c| c.get(field))
                .and_then(|h| h.as_array())
                .map(|arr| !arr.is_empty())
                .unwrap_or(false)
        };
        has_entries("hooks") || has_entries("rpc")
    }

    #[cfg(feature = "script_plugins")]
    fn init_script_runtime(
        &self,
        plugin: &ferum_domain::models::plugin::Plugin,
    ) -> Result<ScriptPluginRuntime, AppError> {
        let bundle_js =
            ScriptPluginRuntime::load_bundle(&plugin.install_path, &plugin.manifest)?;

        // A host is only reachable if it was both declared by the manifest AND
        // granted by the admin at install time — never trust the manifest alone,
        // since a plugin author could ship a wider allowlist than was reviewed.
        let manifest_allowlist =
            ScriptPluginRuntime::http_allowlist_from_manifest(&plugin.manifest);
        let granted_allowlist =
            ScriptPluginRuntime::http_allowlist_from_capabilities(&plugin.granted_capabilities);
        let http_allowlist: Vec<String> = manifest_allowlist
            .into_iter()
            .filter(|host| granted_allowlist.contains(host))
            .collect();

        // Same manifest ∩ granted trust model for the curated Ferum.forum.* API.
        let manifest_api = ScriptPluginRuntime::api_names_from_manifest(&plugin.manifest);
        let granted_api_set =
            ScriptPluginRuntime::api_names_from_capabilities(&plugin.granted_capabilities);
        let granted_api: Vec<String> = manifest_api
            .into_iter()
            .filter(|a| granted_api_set.contains(a))
            .collect();

        // db is an all-or-nothing capability (the schema itself is already the
        // isolation boundary — there's no finer-grained "which tables" to gate).
        let db_enabled = ScriptPluginRuntime::db_requested_by_manifest(&plugin.manifest)
            && ScriptPluginRuntime::db_granted_by_capabilities(&plugin.granted_capabilities);

        ScriptPluginRuntime::new(
            plugin.slug.clone(),
            bundle_js,
            plugin.id,
            plugin.config.clone(),
            self.plugin_repo.clone(),
            self.cache.clone(),
            self.plugin_storage.clone(),
            self.user_repo.clone(),
            self.notification_repo.clone(),
            granted_api,
            self.db_gateway.clone(),
            db_enabled,
            http_allowlist,
        )
    }
}

// ─── PluginHookRuntime implementation ────────────────────────────────────────

#[async_trait]
impl PluginHookRuntime for PluginRegistry {
    async fn dispatch_before_hook(
        &self,
        hook: &str,
        ctx: &HookContext,
    ) -> Result<HookDecision, AppError> {
        // Snapshot entries without holding the lock across await points.
        // hook_id is stored in HookEntry to avoid a DB query per hook execution.
        let entries: Vec<(Uuid, Uuid, String, PluginTier, Arc<CircuitBreaker>)> = {
            let table = self.dispatch_table.lock().unwrap();
            table
                .get(hook)
                .map(|v| {
                    v.iter()
                        .map(|e| (e.plugin_id, e.hook_id, e.plugin_slug.clone(), e.tier.clone(), e.circuit_breaker.clone()))
                        .collect()
                })
                .unwrap_or_default()
        };

        for (plugin_id, _hook_id, plugin_slug, tier, cb) in entries {
            if cb.is_open() {
                tracing::warn!(plugin = %plugin_slug, hook, "Circuit open — skipping");
                continue;
            }

            match tier {
                PluginTier::Manifest => {
                    // Tier 1 has no before-hooks — no-op
                    cb.record_success();
                }

                PluginTier::Script => {
                    #[cfg(feature = "script_plugins")]
                    {
                        let hook_id = _hook_id;
                        let rt = self.script_runtimes.get(&plugin_id);
                        if let Some(rt) = rt {
                            let ctx_json = serde_json::to_string(ctx).unwrap_or_default();
                            let start = std::time::Instant::now();

                            let result = tokio::time::timeout(
                                Duration::from_millis(self.hook_timeout_ms),
                                rt.execute_hook(hook, ctx_json),
                            )
                            .await;

                            let elapsed_ms = start.elapsed().as_millis() as i32;

                            match result {
                                Ok(decision) => {
                                    cb.record_success();

                                    // Update rolling average latency (fire-and-forget).
                                    // hook_id comes from the dispatch table snapshot — no DB query needed.
                                    let repo = self.plugin_repo.clone();
                                    tokio::spawn(async move {
                                        let _ = repo.update_hook_avg_ms(hook_id, elapsed_ms).await;
                                    });

                                    if let HookDecision::Deny { .. } = &decision {
                                        return Ok(decision);
                                    }
                                }
                                Err(_timeout) => {
                                    if cb.record_failure() {
                                        tracing::warn!(
                                            plugin = %plugin_slug,
                                            hook,
                                            "Hook timeout — circuit opened"
                                        );
                                        let _ = self.plugin_repo.update_circuit_open(plugin_id, true).await;
                                    }
                                    tracing::warn!(
                                        plugin = %plugin_slug,
                                        hook,
                                        timeout_ms = self.hook_timeout_ms,
                                        "Script hook timed out — allowing"
                                    );
                                }
                            }
                        } else {
                            tracing::warn!(
                                plugin = %plugin_slug,
                                "No script runtime found for active plugin — allowing"
                            );
                        }
                    }
                    #[cfg(not(feature = "script_plugins"))]
                    {
                        let _ = (plugin_id, plugin_slug, ctx);
                        cb.record_success();
                    }
                }

                PluginTier::Service => {
                    // Tier 3 — implemented in Milestone 3
                    tracing::debug!(plugin = %plugin_slug, hook, "Tier 3 hook (M3) — allowing");
                    cb.record_success();
                }
            }
        }

        Ok(HookDecision::Allow)
    }

    async fn dispatch_after_event(&self, _event_type: &str, _payload: serde_json::Value) {
        // Tier 1: handled by existing WebhookSubscriber in EventBus — nothing to do here.

        #[cfg(feature = "script_plugins")]
        {
            // Collect Script plugins subscribed to this event_type
            let runtimes: Vec<(String, Arc<ScriptPluginRuntime>)> = {
                let table = self.dispatch_table.lock().unwrap();
                table
                    .get(_event_type)
                    .map(|entries| {
                        entries
                            .iter()
                            .filter(|e| e.tier == PluginTier::Script)
                            .filter_map(|e| {
                                self.script_runtimes
                                    .get(&e.plugin_id)
                                    .map(|rt| (e.plugin_slug.clone(), rt.clone()))
                            })
                            .collect()
                    })
                    .unwrap_or_default()
            };

            if !runtimes.is_empty() {
                let payload_json = serde_json::to_string(&_payload).unwrap_or_default();
                for (slug, rt) in runtimes {
                    let et = _event_type.to_string();
                    let pj = payload_json.clone();
                    tokio::spawn(async move {
                        tracing::debug!(plugin = %slug, event = %et, "Dispatching after-event");
                        rt.dispatch_event(&et, pj).await;
                    });
                }
            }
        }
    }

}

// ─── PluginRpcRuntime implementation ─────────────────────────────────────────

#[async_trait]
impl PluginRpcRuntime for PluginRegistry {
    async fn dispatch_rpc(
        &self,
        plugin_slug: &str,
        action: &str,
        ctx: &HookContext,
    ) -> Result<serde_json::Value, AppError> {
        let plugin = self
            .plugin_repo
            .find_by_slug(plugin_slug)
            .await?
            .ok_or(AppError::NotFound)?;

        if plugin.status != PluginStatus::Active || plugin.tier != PluginTier::Script {
            return Err(AppError::NotFound);
        }

        #[cfg(feature = "script_plugins")]
        {
            // An action is only invokable if it was both declared by the manifest
            // AND granted by the admin at install time — same trust model as hooks
            // and the HTTP allowlist. Checked here, before ever reaching the plugin's
            // JS thread, so an ungranted action never executes plugin code at all.
            let manifest_actions = ScriptPluginRuntime::rpc_actions_from_manifest(&plugin.manifest);
            let granted_actions =
                ScriptPluginRuntime::rpc_actions_from_capabilities(&plugin.granted_capabilities);
            if !manifest_actions.iter().any(|a| a == action) || !granted_actions.iter().any(|a| a == action) {
                return Err(AppError::forbidden("rpc_action_not_granted"));
            }

            let rt = self
                .script_runtimes
                .get(&plugin.id)
                .map(|r| r.clone())
                .ok_or_else(|| AppError::internal("Plugin runtime not active"))?;

            let ctx_json = serde_json::to_string(ctx).unwrap_or_default();
            match tokio::time::timeout(
                Duration::from_millis(self.hook_timeout_ms),
                rt.execute_rpc(action, ctx_json),
            )
            .await
            {
                Ok(result) => result,
                Err(_timeout) => Err(AppError::internal("Plugin RPC handler timed out")),
            }
        }

        #[cfg(not(feature = "script_plugins"))]
        {
            let _ = (action, ctx);
            Err(AppError::NotFound)
        }
    }
}

// ─── PluginUiRuntime implementation ──────────────────────────────────────────

#[async_trait]
impl PluginUiRuntime for PluginRegistry {
    async fn active_ui_slots(&self) -> Vec<UiSlotEntry> {
        match self.plugin_repo.active_ui_slots().await {
            Ok(slots) => slots
                .into_iter()
                .map(|s| UiSlotEntry {
                    slot_name: s.slot_name,
                    plugin_slug: s.plugin_slug,
                    asset_url: s.asset_url,
                    custom_element_tag: s.custom_element_tag,
                    props: s.props,
                    load_order: s.load_order,
                })
                .collect(),
            Err(e) => {
                tracing::error!("Failed to load active UI slots: {:?}", e);
                vec![]
            }
        }
    }
}

// ─── PluginLifecycle implementation ──────────────────────────────────────────

#[async_trait]
impl PluginLifecycle for PluginRegistry {
    async fn reload_plugin(&self, plugin_id: Uuid) {
        // Step 1: Fetch all needed data from DB before acquiring any lock
        let plugin = match self.plugin_repo.find_by_id(plugin_id).await {
            Ok(Some(p)) => p,
            _ => return,
        };

        let new_hooks = if plugin.status == PluginStatus::Active {
            match self.plugin_repo.hooks_for_plugin(plugin_id).await {
                Ok(hooks) => hooks,
                Err(e) => {
                    tracing::error!(plugin = %plugin.slug, error = ?e, "Failed to load hooks for reload");
                    return;
                }
            }
        } else {
            vec![]
        };

        // Step 2: Update dispatch table (lock held briefly, no await inside)
        {
            let mut table = self.dispatch_table.lock().unwrap();
            for entries in table.values_mut() {
                entries.retain(|e| e.plugin_id != plugin_id);
            }
            table.retain(|_, v| !v.is_empty());

            for hook in new_hooks.iter().filter(|h| h.is_active) {
                table.entry(hook.hook_name.clone()).or_default().push(HookEntry {
                    plugin_id: plugin.id,
                    hook_id: hook.id,
                    plugin_slug: plugin.slug.clone(),
                    tier: plugin.tier.clone(),
                    // Circuit always resets on reload — circuit_open in DB is admin-visibility only.
                    circuit_breaker: Arc::new(CircuitBreaker::new(
                        self.circuit_threshold,
                        3600,
                        300,
                    )),
                });
            }
        }

        // Step 3: Manage Script runtime lifecycle (no lock held)
        #[cfg(feature = "script_plugins")]
        {
            // Always remove the old runtime first (Drop sends Shutdown)
            self.script_runtimes.remove(&plugin_id);

            if plugin.status == PluginStatus::Active
                && plugin.tier == PluginTier::Script
                && Self::manifest_needs_script_runtime(&plugin.manifest)
            {
                match self.init_script_runtime(&plugin) {
                    Ok(rt) => {
                        self.script_runtimes.insert(plugin_id, Arc::new(rt));
                        tracing::info!(plugin = %plugin.slug, "Script runtime restarted");
                    }
                    Err(e) => {
                        tracing::error!(
                            plugin = %plugin.slug,
                            error = ?e,
                            "Failed to restart script runtime"
                        );
                        let _ = self
                            .plugin_repo
                            .update_status(
                                plugin_id,
                                ferum_domain::models::plugin::PluginStatus::Error,
                                Some(e.to_string()),
                            )
                            .await;
                    }
                }
            }
        }

        if plugin.status == PluginStatus::Active {
            tracing::info!(plugin = %plugin.slug, "Plugin dispatch table reloaded");
        } else {
            tracing::info!(plugin = %plugin.slug, "Plugin removed from dispatch table");
        }
    }
}
