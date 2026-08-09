use std::sync::Arc;

use bytes::Bytes;
use uuid::Uuid;

use crate::constants::MAX_PLUGIN_MEDIA_BYTES;
use crate::permission::PermissionChecker;
use crate::ports::{ForumJob, JobQueue, PluginLifecycle, StorageService};
use crate::shared::{AppError, OptionExt};
use crate::storage_utils::{cas_key, validate_image_content_type};
use crate::validators::validate_image_magic;
use ferum_domain::models::plugin::{
    ui_slot_element_tag, NewPlugin, NewPluginHook, NewPluginLog, NewPluginUiSlot, Plugin, PluginLog,
    PluginLogQuery, PluginStatus, PluginTier, PluginUiSlot,
};
use ferum_domain::repositories::plugin_db_repository::PluginDbGateway;
use ferum_domain::repositories::plugin_repository::PluginRepository;
use ferum_domain::repositories::stored_file_repository::StoredFileRepository;
use ferum_domain::repositories::webhook_repository::{NewWebhook, WebhookRepository};
use ferum_domain::AuthUser;

pub struct PluginUseCase {
    pub plugins: Arc<dyn PluginRepository>,
    pub webhooks: Arc<dyn WebhookRepository>,
    pub plugin_runtime: Arc<dyn PluginLifecycle>,
    pub db_gateway: Arc<dyn PluginDbGateway>,
    pub stored_files: Arc<dyn StoredFileRepository>,
    /// Blob bytes and the authority on file URL shape.
    pub storage: Arc<dyn StorageService>,
    pub jobs: Arc<dyn JobQueue>,
    plugins_dir: std::path::PathBuf,
}

impl PluginUseCase {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        plugins: Arc<dyn PluginRepository>,
        webhooks: Arc<dyn WebhookRepository>,
        plugin_runtime: Arc<dyn PluginLifecycle>,
        db_gateway: Arc<dyn PluginDbGateway>,
        stored_files: Arc<dyn StoredFileRepository>,
        storage: Arc<dyn StorageService>,
        jobs: Arc<dyn JobQueue>,
        plugins_dir: std::path::PathBuf,
    ) -> Self {
        Self {
            plugins,
            webhooks,
            plugin_runtime,
            db_gateway,
            stored_files,
            storage,
            jobs,
            plugins_dir,
        }
    }

    // ─── List / Get ───────────────────────────────────────────────────────────

    pub async fn list(&self, actor: &AuthUser) -> Result<Vec<Plugin>, AppError> {
        PermissionChecker::can_manage_plugins(actor)?;
        self.plugins.list().await
    }

    pub async fn get_by_slug(&self, actor: &AuthUser, slug: &str) -> Result<Plugin, AppError> {
        PermissionChecker::can_manage_plugins(actor)?;
        self.plugins
            .find_by_slug(slug)
            .await?
            .or_not_found()
    }

    // ─── Register (called by handler after package extraction) ────────────────

    /// Register an extracted plugin in the database.
    ///
    /// The handler is responsible for:
    ///   1. Extracting the .fpkg archive to disk
    ///   2. Parsing the manifest with ManifestLoader
    ///   3. Presenting capabilities to admin for review
    ///   4. Calling this method with the resolved data
    ///
    /// Returns the plugin with status = Inactive (DB default is `installing`,
    /// but this method immediately transitions to `inactive` before returning).
    #[allow(clippy::too_many_arguments)]
    pub async fn register_extracted(
        &self,
        actor: &AuthUser,
        slug: String,
        name: String,
        version: String,
        tier: PluginTier,
        manifest: serde_json::Value,
        install_path: String,
        granted_capabilities: serde_json::Value,
    ) -> Result<Plugin, AppError> {
        PermissionChecker::can_manage_plugins(actor)?;

        if self.plugins.find_by_slug(&slug).await?.is_some() {
            return Err(AppError::Conflict("plugin_already_installed".to_string()));
        }

        let plugin = self
            .plugins
            .create(NewPlugin {
                slug,
                name,
                version,
                tier,
                manifest,
                granted_capabilities,
                install_path,
                installed_by: Some(actor.id),
            })
            .await?;

        // Provision the plugin's own Postgres schema once, at install time — never
        // from a runtime query. Requires the `db` capability to be BOTH requested
        // by the manifest AND explicitly granted by the admin during capability
        // review, same trust model as hooks/rpc/api.
        let schema_tables = manifest_schema_tables(&plugin.manifest);
        if !schema_tables.is_empty()
            && manifest_wants_db(&plugin.manifest)
            && granted_db(&plugin.granted_capabilities)
        {
            if let Err(e) = self.db_gateway.provision_schema(&plugin.slug, &schema_tables).await {
                // Leave the row visible (as Error) rather than silently orphaned —
                // admin can see why and uninstall to retry.
                self.plugins
                    .update_status(plugin.id, PluginStatus::Error, Some(e.to_string()))
                    .await
                    .ok();
                return Err(e);
            }
            self.append_log(
                plugin.id,
                "info",
                None,
                None,
                &format!("Provisioned plugin schema with {} statement(s).", schema_tables.len()),
            )
            .await;
        }

        // Seed config from the manifest's declared defaults. Without this a plugin
        // installs with `config = {}` however carefully its manifest describes
        // what it wants, and the operator's first experience is a plugin that
        // activates cleanly and then does nothing at all — no error, no log line,
        // nothing to search for. `default` in a JSON Schema means "use this when
        // the value is absent", and every manifest in examples/ was already
        // written as if that were honoured here.
        //
        // Skipped when it yields nothing, so a plugin declaring no defaults keeps
        // an untouched `{}` rather than gaining a log line about writing one.
        let defaults = config_defaults_from_schema(&plugin.manifest);
        if defaults.as_object().is_some_and(|o| !o.is_empty()) {
            self.plugins.update_config(plugin.id, defaults).await?;
            self.append_log(
                plugin.id,
                "info",
                None,
                None,
                "Seeded configuration from the manifest's declared defaults.",
            )
            .await;
        }

        self.plugins
            .update_status(plugin.id, PluginStatus::Inactive, None)
            .await?;

        self.append_log(
            plugin.id,
            "info",
            None,
            None,
            "Plugin registered, awaiting configuration and activation.",
        )
        .await;

        self.plugins
            .find_by_id(plugin.id)
            .await?
            .or_not_found()
    }

    // ─── Configure ────────────────────────────────────────────────────────────

    /// Save admin-provided config values. Validates against config_schema from manifest.
    pub async fn configure(
        &self,
        actor: &AuthUser,
        slug: &str,
        config: serde_json::Value,
    ) -> Result<(), AppError> {
        PermissionChecker::can_manage_plugins(actor)?;

        let plugin = self
            .plugins
            .find_by_slug(slug)
            .await?
            .or_not_found()?;

        // Validate config against config_schema if present in manifest
        if let Some(schema) = plugin.manifest.get("config_schema") {
            validate_config_against_schema(&config, schema)?;
        }

        self.plugins.update_config(plugin.id, config).await?;

        // Reload the runtime so Tier 2 Script plugins pick up the new config immediately.
        // For inactive plugins this is a no-op (reload removes nothing from the dispatch table).
        self.plugin_runtime.reload_plugin(plugin.id).await;

        self.append_log(plugin.id, "info", None, None, "Plugin configuration updated.")
            .await;

        Ok(())
    }

    // ─── Activate ─────────────────────────────────────────────────────────────

    /// Activate a plugin:
    ///   - Sets status = Active
    ///   - Registers hooks from manifest into plugin_hooks table
    ///   - Registers UI slots from manifest into plugin_ui_slots table (Tier 3)
    ///   - Registers webhooks from manifest [[webhooks]] sections (Tier 1)
    ///   - Calls plugin_runtime.reload_plugin() to refresh in-memory dispatch table
    pub async fn activate(&self, actor: &AuthUser, slug: &str) -> Result<Plugin, AppError> {
        PermissionChecker::can_manage_plugins(actor)?;

        let plugin = self
            .plugins
            .find_by_slug(slug)
            .await?
            .or_not_found()?;

        if plugin.status == PluginStatus::Active {
            return Ok(plugin);
        }

        // Tier 1 (Manifest) plugins have no before-hooks; registering them wastes DB rows.
        // Only Script and Service plugins declare executable hook handlers.
        if plugin.tier != PluginTier::Manifest {
            self.register_hooks_from_manifest(&plugin).await?;
        }

        // Register UI slots from the manifest's [ui_slots.*] sections. Runs for every
        // tier — in practice Script plugins are the ones that declare them.
        self.register_ui_slots_from_manifest(&plugin).await?;

        // Tier 1: register webhooks from manifest [[webhooks]] sections
        if plugin.tier == PluginTier::Manifest {
            self.register_manifest_webhooks(&plugin).await?;
        }

        self.plugins
            .update_status(plugin.id, PluginStatus::Active, None)
            .await?;
        self.plugins.update_activated_at(plugin.id).await?;

        // Refresh in-memory dispatch table
        self.plugin_runtime.reload_plugin(plugin.id).await;

        self.append_log(plugin.id, "info", None, None, "Plugin activated.")
            .await;

        self.plugins
            .find_by_id(plugin.id)
            .await?
            .or_not_found()
    }

    // ─── Deactivate ───────────────────────────────────────────────────────────

    pub async fn deactivate(&self, actor: &AuthUser, slug: &str) -> Result<(), AppError> {
        PermissionChecker::can_manage_plugins(actor)?;

        let plugin = self
            .plugins
            .find_by_slug(slug)
            .await?
            .or_not_found()?;

        self.plugins
            .update_status(plugin.id, PluginStatus::Inactive, None)
            .await?;

        // Remove webhooks owned by this plugin so they stop firing while deactivated.
        // On uninstall, ON DELETE CASCADE handles cleanup automatically.
        self.webhooks.delete_by_plugin(plugin.id).await?;

        // Remove UI slots so they are not injected into page context while inactive.
        // Re-activation calls register_ui_slots_from_manifest() which re-creates them.
        self.plugins.delete_ui_slots_for_plugin(plugin.id).await?;

        // Hook rows remain in DB with is_active=true; the registry reload empties
        // the in-memory dispatch table for this plugin, which is sufficient to stop dispatch.
        self.plugin_runtime.reload_plugin(plugin.id).await;

        self.append_log(plugin.id, "info", None, None, "Plugin deactivated.")
            .await;

        Ok(())
    }

    // ─── Uninstall ────────────────────────────────────────────────────────────

    /// Uninstall a plugin.
    /// `confirm_slug` must match the plugin's slug — guard against accidental deletions.
    /// Removes DB rows and then cleans up plugin files from disk (non-fatal if dir missing).
    pub async fn uninstall(
        &self,
        actor: &AuthUser,
        slug: &str,
        confirm_slug: &str,
    ) -> Result<(), AppError> {
        PermissionChecker::can_manage_plugins(actor)?;

        if slug != confirm_slug {
            return Err(AppError::invalid("confirmation_slug_mismatch"));
        }

        let plugin = self
            .plugins
            .find_by_slug(slug)
            .await?
            .or_not_found()?;

        // Mark as uninstalling first so hooks stop being dispatched
        self.plugins
            .update_status(plugin.id, PluginStatus::Uninstalling, None)
            .await?;
        self.plugin_runtime.reload_plugin(plugin.id).await;

        // Drop the plugin's own Postgres schema (if it ever had one) — non-fatal,
        // uninstall should still proceed even if this fails so the plugin doesn't
        // become permanently stuck.
        if let Err(e) = self.db_gateway.drop_schema(slug).await {
            tracing::warn!(plugin = %slug, error = ?e, "Failed to drop plugin schema during uninstall");
        }

        // Dereference every file this plugin ever uploaded via /api/plugins/:slug/media
        // (upload_media namespaces CAS keys as "plugin_{slug}/..." — see cas_key call
        // below). Without this, media uploads would be orphaned forever: nothing else
        // ever references or GCs them. Trailing "/" keeps the prefix from also
        // matching a different plugin whose slug happens to start the same way
        // (e.g. "plugin_com.ferum.chat" vs "plugin_com.ferum.chatbox").
        match self.stored_files.list_keys_with_prefix(&format!("plugin_{slug}/")).await {
            Ok(keys) => {
                for key in keys {
                    match self.stored_files.decrement_ref(&key).await {
                        Ok(0) => {
                            let _ = self.jobs.enqueue(ForumJob::GcStorageKey { key }).await;
                        }
                        Ok(_) => {}
                        Err(e) => tracing::warn!(plugin = %slug, key = %key, error = ?e, "Failed to dereference plugin media during uninstall"),
                    }
                }
            }
            Err(e) => tracing::warn!(plugin = %slug, error = ?e, "Failed to list plugin media during uninstall"),
        }

        // Remove DB rows (hooks, slots, logs cascade-deleted via FK)
        let install_path = plugin.install_path.clone();
        self.plugins.delete(plugin.id).await?;

        // Clean up files from disk. Non-fatal: if already removed, that's fine.
        let disk_path = if install_path.is_empty() {
            self.plugins_dir.join(slug)
        } else {
            std::path::PathBuf::from(&install_path)
        };
        if disk_path.exists() {
            if let Err(e) = std::fs::remove_dir_all(&disk_path) {
                tracing::warn!(
                    path = %disk_path.display(),
                    "Failed to remove plugin directory during uninstall: {}",
                    e
                );
            }
        }

        Ok(())
    }

    // ─── Logs ─────────────────────────────────────────────────────────────────

    pub async fn get_logs(
        &self,
        actor: &AuthUser,
        slug: &str,
        query: PluginLogQuery,
    ) -> Result<Vec<PluginLog>, AppError> {
        PermissionChecker::can_manage_plugins(actor)?;
        let plugin = self
            .plugins
            .find_by_slug(slug)
            .await?
            .or_not_found()?;
        self.plugins.get_logs(plugin.id, query).await
    }

    // ─── Media upload (member — for plugin widgets like a story/gallery UI) ───

    /// Upload an image on behalf of a plugin's own feature (not avatar/cover/etc,
    /// those go through UserUseCase). Stored via the same CAS backend, under a
    /// `plugin_{slug}` key namespace so a plugin can never collide with or
    /// overwrite another plugin's — or core's — stored files. Requires the
    /// plugin to be Active and to have been granted the `media` capability
    /// (manifest requests it AND admin approved it at install time).
    pub async fn upload_media(
        &self,
        actor: &AuthUser,
        slug: &str,
        data: Bytes,
        content_type: String,
    ) -> Result<String, AppError> {
        PermissionChecker::require_not_banned(actor)?;

        let plugin = self.plugins.find_by_slug(slug).await?.or_not_found()?;
        if plugin.status != PluginStatus::Active {
            return Err(AppError::NotFound);
        }
        if !manifest_wants_media(&plugin.manifest) || !granted_media(&plugin.granted_capabilities) {
            return Err(AppError::forbidden("media_capability_not_granted"));
        }

        if !validate_image_content_type(&content_type) || !validate_image_magic(&data) {
            return Err(AppError::invalid("media_invalid_type"));
        }
        if data.len() > MAX_PLUGIN_MEDIA_BYTES {
            return Err(AppError::unprocessable(&format!(
                "media exceeds {} MB limit",
                MAX_PLUGIN_MEDIA_BYTES / (1024 * 1024)
            )));
        }

        let size = data.len() as i64;
        let key = cas_key(&format!("plugin_{slug}"), &data, &content_type);
        self.storage.put(&key, data, &content_type).await?;
        self.stored_files
            .upsert_and_ref(&key, &content_type, size, Some(actor.id))
            .await?;

        // Plugin media is referenced from plugin-authored markup that this app
        // does not rewrite — same reasoning as post attachments.
        Ok(crate::ports::file_url(&key))
    }

    // ─── Active UI Slots (public — no auth, for frontend SSR) ─────────────────

    pub async fn active_ui_slots(&self) -> Result<Vec<PluginUiSlot>, AppError> {
        self.plugins.active_ui_slots().await
    }

    // ─── UI Slot placement (admin) ─────────────────────────────────────────────

    pub async fn list_ui_slots(&self, actor: &AuthUser, slug: &str) -> Result<Vec<PluginUiSlot>, AppError> {
        PermissionChecker::can_manage_plugins(actor)?;
        let plugin = self.plugins.find_by_slug(slug).await?.or_not_found()?;
        self.plugins.ui_slots_for_plugin(plugin.id).await
    }

    /// Move a plugin's UI element to a different named slot / load order —
    /// takes effect immediately, no deactivate/reactivate needed, since
    /// active_ui_slots() reads slot_name straight from this row on every render.
    pub async fn update_ui_slot_placement(
        &self,
        actor: &AuthUser,
        slug: &str,
        slot_id: Uuid,
        slot_name: String,
        load_order: i32,
    ) -> Result<PluginUiSlot, AppError> {
        PermissionChecker::can_manage_plugins(actor)?;
        let plugin = self.plugins.find_by_slug(slug).await?.or_not_found()?;

        let owned = self.plugins.ui_slots_for_plugin(plugin.id).await?;
        if !owned.iter().any(|s| s.id == slot_id) {
            return Err(AppError::NotFound);
        }
        if slot_name.trim().is_empty() {
            return Err(AppError::invalid("slot_name_required"));
        }

        self.plugins.update_ui_slot(slot_id, slot_name, load_order).await
    }

    // ─── Internal helpers ─────────────────────────────────────────────────────

    async fn register_hooks_from_manifest(&self, plugin: &Plugin) -> Result<(), AppError> {
        // Delete existing hooks first (idempotent re-activation)
        self.plugins.delete_hooks_for_plugin(plugin.id).await?;

        // A hook only takes effect if it was both requested by the manifest AND
        // granted by the admin at install time — the manifest alone is not trusted,
        // since granted_capabilities is what the admin actually reviewed and approved.
        let granted = granted_hook_names(&plugin.granted_capabilities);
        let hooks = manifest_hooks(&plugin.manifest);
        for (hook_name, priority) in hooks {
            if !granted.contains(&hook_name) {
                self.append_log(
                    plugin.id,
                    "warn",
                    Some(&hook_name),
                    None,
                    &format!(
                        "Hook '{}' requested by manifest but not granted at install — skipped.",
                        hook_name
                    ),
                )
                .await;
                continue;
            }
            self.plugins
                .create_hook(NewPluginHook {
                    plugin_id: plugin.id,
                    hook_name,
                    priority,
                })
                .await?;
        }
        Ok(())
    }

    async fn register_ui_slots_from_manifest(&self, plugin: &Plugin) -> Result<(), AppError> {
        self.plugins.delete_ui_slots_for_plugin(plugin.id).await?;

        let slots = manifest_ui_slots(&plugin.manifest, plugin.id, &plugin.slug);
        for slot_data in slots {
            self.plugins.create_ui_slot(slot_data).await?;
        }
        Ok(())
    }

    /// Parse [[webhooks]] from Tier 1 manifest and register them in the webhooks table.
    /// Substitutes `{{config.field}}` templates with admin-set config values.
    async fn register_manifest_webhooks(&self, plugin: &Plugin) -> Result<(), AppError> {
        let webhook_defs = match plugin.manifest.get("webhooks").and_then(|v| v.as_array()) {
            Some(arr) => arr.clone(),
            None => return Ok(()),
        };

        for webhook_def in webhook_defs {
            let url_template = webhook_def
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let events: Vec<String> = webhook_def
                .get("event")
                .and_then(|v| v.as_str())
                .map(|s| vec![s.to_string()])
                .unwrap_or_default();

            if url_template.is_empty() || events.is_empty() {
                continue;
            }

            let resolved_url = resolve_config_template(url_template, &plugin.config);

            if resolved_url.contains("{{") {
                // Template not fully resolved — config not set yet, skip
                self.append_log(
                    plugin.id,
                    "warn",
                    None,
                    None,
                    &format!(
                        "Webhook URL template not resolved (config missing?): {}",
                        url_template
                    ),
                )
                .await;
                continue;
            }

            if let Err(e) = super::webhook_usecase::validate_webhook_url(&resolved_url) {
                self.append_log(
                    plugin.id,
                    "warn",
                    None,
                    None,
                    &format!("Webhook URL rejected ({}): {}", e, resolved_url),
                )
                .await;
                continue;
            }

            self.webhooks
                .create(NewWebhook {
                    url: resolved_url,
                    events,
                    secret: None,
                    created_by_id: None,
                    plugin_id: Some(plugin.id),
                })
                .await?;
        }

        Ok(())
    }

    async fn append_log(
        &self,
        plugin_id: Uuid,
        level: &str,
        hook_name: Option<&str>,
        duration_ms: Option<i32>,
        message: &str,
    ) {
        let _ = self
            .plugins
            .append_log(NewPluginLog {
                plugin_id,
                level: level.to_string(),
                hook_name: hook_name.map(str::to_string),
                duration_ms,
                message: message.to_string(),
                context: None,
            })
            .await;
    }
}

// ─── Manifest parsing helpers ─────────────────────────────────────────────────

/// Extract hook declarations from manifest capabilities.
/// Returns Vec<(hook_name, priority)>.
fn manifest_hooks(manifest: &serde_json::Value) -> Vec<(String, i32)> {
    manifest
        .get("capabilities")
        .and_then(|c| c.get("hooks"))
        .and_then(|h| h.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    let name = item.get("name")?.as_str()?.to_string();
                    let priority = item
                        .get("priority")
                        .and_then(|p| p.as_i64())
                        .unwrap_or(100) as i32;
                    Some((name, priority))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Extract the set of hook names the admin actually granted at install time.
/// `granted_capabilities` mirrors the shape of a manifest's `[capabilities]` table
/// directly (i.e. `{"hooks": [...], "http_allowlist": [...]}`), not nested under
/// a further "capabilities" key.
fn granted_hook_names(granted_capabilities: &serde_json::Value) -> std::collections::HashSet<String> {
    granted_capabilities
        .get("hooks")
        .and_then(|h| h.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| item.get("name")?.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// Whether the manifest's [capabilities] requests the `db` capability.
fn manifest_wants_db(manifest: &serde_json::Value) -> bool {
    manifest
        .get("capabilities")
        .and_then(|c| c.get("db"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Whether the admin actually granted the `db` capability at install time.
fn granted_db(granted_capabilities: &serde_json::Value) -> bool {
    granted_capabilities
        .get("db")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Whether the manifest's [capabilities] requests the `media` capability.
fn manifest_wants_media(manifest: &serde_json::Value) -> bool {
    manifest
        .get("capabilities")
        .and_then(|c| c.get("media"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Whether the admin actually granted the `media` capability at install time.
fn granted_media(granted_capabilities: &serde_json::Value) -> bool {
    granted_capabilities
        .get("media")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Extract one-time schema DDL statements from the manifest's [schema] section.
fn manifest_schema_tables(manifest: &serde_json::Value) -> Vec<String> {
    manifest
        .get("schema")
        .and_then(|s| s.get("tables"))
        .and_then(|a| a.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default()
}

/// Extract UI slot declarations from manifest ui_slots table.
fn manifest_ui_slots(
    manifest: &serde_json::Value,
    plugin_id: Uuid,
    plugin_slug: &str,
) -> Vec<NewPluginUiSlot> {
    let ui_slots_obj = match manifest.get("ui_slots").and_then(|v| v.as_object()) {
        Some(obj) => obj,
        None => return vec![],
    };

    ui_slots_obj
        .iter()
        .enumerate()
        .filter_map(|(i, (slot_name, slot_def))| {
            let component = slot_def.get("component")?.as_str()?;
            // Same function the repository derives with on read, so the stored
            // column and what the page actually renders cannot disagree.
            let custom_element_tag = ui_slot_element_tag(plugin_slug, slot_name);
            let props: Vec<String> = slot_def
                .get("props")
                .and_then(|p| p.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();

            Some(NewPluginUiSlot {
                plugin_id,
                slot_name: slot_name.clone(),
                asset_url: component.to_string(),
                custom_element_tag,
                props,
                load_order: (i as i32) * 10 + 100,
            })
        })
        .collect()
}

/// Build the starting config from `config_schema.properties.*.default`.
///
/// One level deep, deliberately: a `default` on a nested property inside an
/// object-typed field would have to be merged into whatever the parent's own
/// `default` already contains, and two sources writing the same path is how a
/// seeded config ends up disagreeing with itself. A field that wants a
/// structured starting value declares it whole, on that field.
///
/// Only what the manifest actually declares is written — a property with no
/// `default` stays absent rather than becoming `null`, which for a plugin
/// reading `Ferum.config.x` are two different answers.
fn config_defaults_from_schema(manifest: &serde_json::Value) -> serde_json::Value {
    let properties = manifest
        .get("config_schema")
        .and_then(|s| s.get("properties"))
        .and_then(|p| p.as_object());

    let Some(properties) = properties else {
        return serde_json::json!({});
    };

    let seeded: serde_json::Map<String, serde_json::Value> = properties
        .iter()
        .filter_map(|(name, prop)| {
            prop.get("default").map(|d| (name.clone(), d.clone()))
        })
        .collect();

    serde_json::Value::Object(seeded)
}

/// Resolve `{{config.field_name}}` templates against the plugin's config JSON.
fn resolve_config_template(template: &str, config: &serde_json::Value) -> String {
    let mut result = template.to_string();
    if let Some(obj) = config.as_object() {
        for (key, value) in obj {
            let placeholder = format!("{{{{config.{}}}}}", key);
            if let Some(s) = value.as_str() {
                result = result.replace(&placeholder, s);
            }
        }
    }
    result
}

// The tag-naming rule moved to `ferum_domain::models::plugin::ui_slot_element_tag`
// so the write path here and the repository's read path share one definition —
// and so the slug could be folded into the name, which is what lets two plugins
// occupy the same slot.

/// Validate a config JSON value against a JSON Schema object.
/// Returns UnprocessableEntity on validation failure.
fn validate_config_against_schema(
    config: &serde_json::Value,
    schema: &serde_json::Value,
) -> Result<(), AppError> {
    // Basic required-field check only (full jsonschema validation happens in infrastructure layer).
    if let Some(required) = schema.get("required").and_then(|r| r.as_array()) {
        for field in required {
            if let Some(field_name) = field.as_str() {
                if config.get(field_name).is_none() {
                    return Err(AppError::unprocessable(&format!(
                        "Config field '{}' is required",
                        field_name
                    )));
                }
            }
        }
    }
    Ok(())
}
