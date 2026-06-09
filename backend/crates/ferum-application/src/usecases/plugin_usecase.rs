use std::sync::Arc;

use uuid::Uuid;

use crate::permission::PermissionChecker;
use crate::ports::PluginRuntime;
use crate::shared::AppError;
use ferum_domain::models::plugin::{
    NewPlugin, NewPluginHook, NewPluginLog, NewPluginUiSlot, Plugin, PluginLog, PluginLogQuery,
    PluginStatus, PluginTier, PluginUiSlot,
};
use ferum_domain::repositories::plugin_repository::PluginRepository;
use ferum_domain::repositories::webhook_repository::{NewWebhook, WebhookRepository};
use ferum_domain::AuthUser;

pub struct PluginUseCase {
    pub plugins: Arc<dyn PluginRepository>,
    pub webhooks: Arc<dyn WebhookRepository>,
    pub plugin_runtime: Arc<dyn PluginRuntime>,
}

impl PluginUseCase {
    pub fn new(
        plugins: Arc<dyn PluginRepository>,
        webhooks: Arc<dyn WebhookRepository>,
        plugin_runtime: Arc<dyn PluginRuntime>,
    ) -> Self {
        Self {
            plugins,
            webhooks,
            plugin_runtime,
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
            .ok_or(AppError::NotFound)
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
            .ok_or(AppError::NotFound)
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
            .ok_or(AppError::NotFound)?;

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
            .ok_or(AppError::NotFound)?;

        if plugin.status == PluginStatus::Active {
            return Ok(plugin);
        }

        // Tier 1 (Manifest) plugins have no before-hooks; registering them wastes DB rows.
        // Only Script and Service plugins declare executable hook handlers.
        if plugin.tier != PluginTier::Manifest {
            self.register_hooks_from_manifest(&plugin).await?;
        }

        // Register UI slots (Tier 3 only — manifest declares ui_slots)
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
            .ok_or(AppError::NotFound)
    }

    // ─── Deactivate ───────────────────────────────────────────────────────────

    pub async fn deactivate(&self, actor: &AuthUser, slug: &str) -> Result<(), AppError> {
        PermissionChecker::can_manage_plugins(actor)?;

        let plugin = self
            .plugins
            .find_by_slug(slug)
            .await?
            .ok_or(AppError::NotFound)?;

        self.plugins
            .update_status(plugin.id, PluginStatus::Inactive, None)
            .await?;

        // Remove webhooks owned by this plugin so they stop firing while deactivated.
        // On uninstall, ON DELETE CASCADE handles cleanup automatically.
        self.webhooks.delete_by_plugin(plugin.id).await?;

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
    /// Caller (handler) is responsible for deleting files from disk after this returns Ok.
    pub async fn uninstall(
        &self,
        actor: &AuthUser,
        slug: &str,
        confirm_slug: &str,
    ) -> Result<String, AppError> {
        PermissionChecker::can_manage_plugins(actor)?;

        if slug != confirm_slug {
            return Err(AppError::unprocessable(
                "Confirmation slug does not match plugin slug",
            ));
        }

        let plugin = self
            .plugins
            .find_by_slug(slug)
            .await?
            .ok_or(AppError::NotFound)?;

        // Mark as uninstalling first so hooks stop being dispatched
        self.plugins
            .update_status(plugin.id, PluginStatus::Uninstalling, None)
            .await?;
        self.plugin_runtime.reload_plugin(plugin.id).await;

        // Remove DB rows (hooks, slots, logs cascade-deleted via FK)
        let install_path = plugin.install_path.clone();
        self.plugins.delete(plugin.id).await?;

        Ok(install_path)
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
            .ok_or(AppError::NotFound)?;
        self.plugins.get_logs(plugin.id, query).await
    }

    // ─── Active UI Slots (public — no auth, for frontend SSR) ─────────────────

    pub async fn active_ui_slots(&self) -> Result<Vec<PluginUiSlot>, AppError> {
        self.plugins.active_ui_slots().await
    }

    // ─── Internal helpers ─────────────────────────────────────────────────────

    async fn register_hooks_from_manifest(&self, plugin: &Plugin) -> Result<(), AppError> {
        // Delete existing hooks first (idempotent re-activation)
        self.plugins.delete_hooks_for_plugin(plugin.id).await?;

        let hooks = manifest_hooks(&plugin.manifest);
        for (hook_name, priority) in hooks {
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

        let slots = manifest_ui_slots(&plugin.manifest, plugin.id);
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

/// Extract UI slot declarations from manifest ui_slots table.
fn manifest_ui_slots(manifest: &serde_json::Value, plugin_id: Uuid) -> Vec<NewPluginUiSlot> {
    let ui_slots_obj = match manifest.get("ui_slots").and_then(|v| v.as_object()) {
        Some(obj) => obj,
        None => return vec![],
    };

    ui_slots_obj
        .iter()
        .enumerate()
        .filter_map(|(i, (slot_name, slot_def))| {
            let component = slot_def.get("component")?.as_str()?;
            let custom_element_tag = slot_name_to_element_tag(slot_name);
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

/// Derive a custom element tag name from a slot name.
/// e.g. "thread.below_posts" → "ferum-slot-thread-below-posts"
fn slot_name_to_element_tag(slot_name: &str) -> String {
    let sanitized = slot_name.replace('.', "-").replace('_', "-");
    format!("ferum-slot-{}", sanitized)
}

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
