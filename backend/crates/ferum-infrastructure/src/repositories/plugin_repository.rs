use async_trait::async_trait;
use chrono::Utc;
use sea_orm::prelude::*;
use sea_orm::*;
use uuid::Uuid;

use crate::crypto::{plugin_config_aad, SecretCipher};
use crate::entities::{plugin_hooks, plugin_logs, plugin_ui_slots, plugins, sea_orm_active_enums};
use ferum_application::shared::AppError;
use ferum_domain::models::plugin::{
    secret_config_keys, ui_slot_element_tag, NewPlugin, NewPluginHook, NewPluginLog,
    NewPluginUiSlot, Plugin, PluginHook, PluginLog, PluginLogQuery, PluginStatus, PluginTier,
    PluginUiSlot,
};
use ferum_domain::repositories::plugin_repository::PluginRepository;
use std::sync::Arc;

pub struct PgPluginRepository {
    db: DatabaseConnection,
    cipher: Option<Arc<SecretCipher>>,
}

impl PgPluginRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db, cipher: None }
    }

    /// Encrypts the `plugins.config` fields a manifest marks `secret = true`.
    ///
    /// **The seam is entity→domain, for the same reason it is in
    /// `PgWebhookRepository`.** Every consumer of a plugin credential — the Tier 1
    /// webhook templater substituting `{{config.webhook_url}}`, the Tier 2 script
    /// runtime exposing `Ferum.config`, the admin detail page — reads it through
    /// `Plugin`, so decrypting here covers all of them at once and leaves no path
    /// that could hand ciphertext to an outbound request. Decrypting further up
    /// would mean auditing each consumer separately, and a missed one fails
    /// silently: a webhook POSTed to the literal string `enc:v1:…` is a 404 at the
    /// far end, recorded as an ordinary delivery failure.
    pub fn with_cipher(mut self, cipher: Arc<SecretCipher>) -> Self {
        self.cipher = Some(cipher);
        self
    }

    /// Opens every sealed secret field in a plugin's config.
    ///
    /// Plaintext passes through untouched, so turning encryption on is a no-op for
    /// plugins configured before it, and each value is sealed on its next save.
    fn open_config(
        &self,
        slug: &str,
        manifest: &serde_json::Value,
        mut config: serde_json::Value,
    ) -> Result<serde_json::Value, AppError> {
        let keys = secret_config_keys(manifest);
        if keys.is_empty() {
            return Ok(config);
        }
        let Some(obj) = config.as_object_mut() else {
            return Ok(config);
        };

        for key in keys {
            let Some(stored) = obj.get(&key).and_then(|v| v.as_str()) else {
                continue;
            };
            match &self.cipher {
                Some(cipher) => {
                    let opened = cipher.open(&plugin_config_aad(slug, &key), stored)?;
                    obj.insert(key, serde_json::Value::String(opened.value));
                }
                // No key configured but the row is sealed. Handing back the
                // ciphertext would send `enc:v1:…` to whatever the plugin points
                // at — a Discord webhook URL, an outbound API — where it fails as
                // an ordinary delivery error with nothing naming the cause.
                // Startup refuses to boot in this state; this is the backstop.
                None if SecretCipher::is_sealed(stored) => {
                    return Err(AppError::internal(format!(
                        "plugin `{slug}` has an encrypted config field `{key}` but \
                         SECRET_ENCRYPTION_KEY is not set"
                    )))
                }
                None => {}
            }
        }
        Ok(config)
    }

    /// Seals every secret field on the way in.
    ///
    /// An empty string is left alone: a blank credential means "not set", and
    /// sealing it would produce a non-blank ciphertext that every presence check
    /// then reads as configured.
    fn seal_config(
        &self,
        slug: &str,
        manifest: &serde_json::Value,
        mut config: serde_json::Value,
    ) -> Result<serde_json::Value, AppError> {
        let Some(cipher) = &self.cipher else {
            return Ok(config);
        };
        let keys = secret_config_keys(manifest);
        if keys.is_empty() {
            return Ok(config);
        }
        let Some(obj) = config.as_object_mut() else {
            return Ok(config);
        };

        for key in keys {
            let Some(plain) = obj.get(&key).and_then(|v| v.as_str()) else {
                continue;
            };
            if plain.is_empty() || SecretCipher::is_sealed(plain) {
                continue;
            }
            let sealed = cipher.seal(&plugin_config_aad(slug, &key), plain)?;
            obj.insert(key, serde_json::Value::String(sealed));
        }
        Ok(config)
    }

    fn plugin_from_entity(&self, m: plugins::Model) -> Result<Plugin, AppError> {
        let config = self.open_config(&m.slug, &m.manifest, m.config.clone())?;
        Ok(plugin_from_entity_with_config(m, config))
    }
}

// ─── Conversion helpers ───────────────────────────────────────────────────────

fn tier_to_entity(t: &PluginTier) -> sea_orm_active_enums::PluginTier {
    match t {
        PluginTier::Manifest => sea_orm_active_enums::PluginTier::Manifest,
        PluginTier::Script => sea_orm_active_enums::PluginTier::Script,
        PluginTier::Service => sea_orm_active_enums::PluginTier::Service,
    }
}

fn tier_from_entity(e: sea_orm_active_enums::PluginTier) -> PluginTier {
    match e {
        sea_orm_active_enums::PluginTier::Manifest => PluginTier::Manifest,
        sea_orm_active_enums::PluginTier::Script => PluginTier::Script,
        sea_orm_active_enums::PluginTier::Service => PluginTier::Service,
    }
}

fn status_to_entity(s: &PluginStatus) -> sea_orm_active_enums::PluginStatus {
    match s {
        PluginStatus::Installing => sea_orm_active_enums::PluginStatus::Installing,
        PluginStatus::Active => sea_orm_active_enums::PluginStatus::Active,
        PluginStatus::Inactive => sea_orm_active_enums::PluginStatus::Inactive,
        PluginStatus::Error => sea_orm_active_enums::PluginStatus::Error,
        PluginStatus::Disabled => sea_orm_active_enums::PluginStatus::Disabled,
        PluginStatus::Uninstalling => sea_orm_active_enums::PluginStatus::Uninstalling,
    }
}

fn status_from_entity(e: sea_orm_active_enums::PluginStatus) -> PluginStatus {
    match e {
        sea_orm_active_enums::PluginStatus::Installing => PluginStatus::Installing,
        sea_orm_active_enums::PluginStatus::Active => PluginStatus::Active,
        sea_orm_active_enums::PluginStatus::Inactive => PluginStatus::Inactive,
        sea_orm_active_enums::PluginStatus::Error => PluginStatus::Error,
        sea_orm_active_enums::PluginStatus::Disabled => PluginStatus::Disabled,
        sea_orm_active_enums::PluginStatus::Uninstalling => PluginStatus::Uninstalling,
    }
}

/// `config` is passed separately because it has already been through the cipher —
/// taking it from `m` here would quietly undo that.
fn plugin_from_entity_with_config(m: plugins::Model, config: serde_json::Value) -> Plugin {
    Plugin {
        id: m.id,
        slug: m.slug,
        name: m.name,
        version: m.version,
        tier: tier_from_entity(m.tier),
        status: status_from_entity(m.status),
        manifest: m.manifest,
        config,
        granted_capabilities: m.granted_capabilities,
        install_path: m.install_path,
        db_schema_name: m.db_schema_name,
        db_schema_version: m.db_schema_version,
        installed_by: m.installed_by,
        installed_at: m.installed_at.with_timezone(&Utc),
        updated_at: m.updated_at.with_timezone(&Utc),
        activated_at: m.activated_at.map(|t| t.with_timezone(&Utc)),
        error_message: m.error_message,
        last_seen_at: m.last_seen_at.map(|t| t.with_timezone(&Utc)),
        restart_count: m.restart_count,
        circuit_open: m.circuit_open,
    }
}

fn hook_from_entity(m: plugin_hooks::Model) -> PluginHook {
    PluginHook {
        id: m.id,
        plugin_id: m.plugin_id,
        hook_name: m.hook_name,
        priority: m.priority,
        is_active: m.is_active,
        avg_ms: m.avg_ms,
    }
}

fn slot_from_entity(m: plugin_ui_slots::Model, plugin_slug: String) -> PluginUiSlot {
    // Derived, not read from `m.custom_element_tag`. The column is written at
    // activation and nothing rewrites it afterwards, so a row created under an
    // older naming rule would keep naming an element no bundle defines — and the
    // failure is silent, because an unknown custom element renders as an empty
    // inline box. Deriving here leaves exactly one authority for the name and
    // makes such rows self-correcting instead of quietly dead.
    let custom_element_tag = ui_slot_element_tag(&plugin_slug, &m.slot_name);
    PluginUiSlot {
        id: m.id,
        plugin_id: m.plugin_id,
        plugin_slug,
        slot_name: m.slot_name,
        asset_url: m.asset_url,
        custom_element_tag,
        props: m.props,
        load_order: m.load_order,
        is_active: m.is_active,
    }
}

fn log_from_entity(m: plugin_logs::Model) -> PluginLog {
    PluginLog {
        id: m.id,
        plugin_id: m.plugin_id,
        level: m.level,
        hook_name: m.hook_name,
        duration_ms: m.duration_ms,
        message: m.message,
        context: m.context,
        created_at: m.created_at.with_timezone(&Utc),
    }
}

// ─── Repository implementation ────────────────────────────────────────────────

#[async_trait]
impl PluginRepository for PgPluginRepository {
    async fn list(&self) -> Result<Vec<Plugin>, AppError> {
        plugins::Entity::find()
            .order_by_asc(plugins::Column::InstalledAt)
            .all(&self.db)
            .await?
            .into_iter()
            .map(|m| self.plugin_from_entity(m))
            .collect()
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<Plugin>, AppError> {
        plugins::Entity::find_by_id(id)
            .one(&self.db)
            .await?
            .map(|m| self.plugin_from_entity(m))
            .transpose()
    }

    async fn find_by_slug(&self, slug: &str) -> Result<Option<Plugin>, AppError> {
        plugins::Entity::find()
            .filter(plugins::Column::Slug.eq(slug))
            .one(&self.db)
            .await?
            .map(|m| self.plugin_from_entity(m))
            .transpose()
    }

    async fn create(&self, data: NewPlugin) -> Result<Plugin, AppError> {
        let model = plugins::ActiveModel {
            id: Set(Uuid::new_v4()),
            slug: Set(data.slug),
            name: Set(data.name),
            version: Set(data.version),
            tier: Set(tier_to_entity(&data.tier)),
            status: Set(sea_orm_active_enums::PluginStatus::Installing),
            manifest: Set(data.manifest),
            config: Set(serde_json::json!({})),
            granted_capabilities: Set(data.granted_capabilities),
            install_path: Set(data.install_path),
            installed_by: Set(data.installed_by),
            ..Default::default()
        };
        self.plugin_from_entity(model.insert(&self.db).await?)
    }

    async fn update_status(
        &self,
        id: Uuid,
        status: PluginStatus,
        error_message: Option<String>,
    ) -> Result<(), AppError> {
        let mut active = plugins::Entity::find_by_id(id)
            .one(&self.db)
            .await?
            .ok_or(AppError::NotFound)?
            .into_active_model();

        active.status = Set(status_to_entity(&status));
        active.error_message = Set(error_message);
        active.update(&self.db).await?;
        Ok(())
    }

    async fn update_config(&self, id: Uuid, config: serde_json::Value) -> Result<(), AppError> {
        let row = plugins::Entity::find_by_id(id)
            .one(&self.db)
            .await?
            .ok_or(AppError::NotFound)?;

        // Sealed against the row's own slug and manifest, not against anything the
        // caller supplies: the manifest names which fields are credentials, and
        // accepting that from the request would let a config save decide its own
        // value was not a secret.
        let config = self.seal_config(&row.slug, &row.manifest, config)?;

        let mut active = row.into_active_model();
        active.config = Set(config);
        active.update(&self.db).await?;
        Ok(())
    }

    async fn update_activated_at(&self, id: Uuid) -> Result<(), AppError> {
        plugins::Entity::update_many()
            .col_expr(
                plugins::Column::ActivatedAt,
                Expr::value(Utc::now().fixed_offset()),
            )
            .filter(plugins::Column::Id.eq(id))
            .exec(&self.db)
            .await?;
        Ok(())
    }

    async fn update_circuit_open(&self, id: Uuid, open: bool) -> Result<(), AppError> {
        plugins::Entity::update_many()
            .col_expr(plugins::Column::CircuitOpen, Expr::value(open))
            .filter(plugins::Column::Id.eq(id))
            .exec(&self.db)
            .await?;
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<(), AppError> {
        plugins::Entity::delete_by_id(id).exec(&self.db).await?;
        Ok(())
    }

    // ── Hooks ─────────────────────────────────────────────────────────────────

    async fn hooks_for_plugin(&self, plugin_id: Uuid) -> Result<Vec<PluginHook>, AppError> {
        Ok(plugin_hooks::Entity::find()
            .filter(plugin_hooks::Column::PluginId.eq(plugin_id))
            .order_by_asc(plugin_hooks::Column::Priority)
            .all(&self.db)
            .await?
            .into_iter()
            .map(hook_from_entity)
            .collect())
    }

    async fn create_hook(&self, data: NewPluginHook) -> Result<PluginHook, AppError> {
        let model = plugin_hooks::ActiveModel {
            id: Set(Uuid::new_v4()),
            plugin_id: Set(data.plugin_id),
            hook_name: Set(data.hook_name),
            priority: Set(data.priority),
            is_active: Set(true),
            ..Default::default()
        };
        Ok(hook_from_entity(model.insert(&self.db).await?))
    }

    async fn delete_hooks_for_plugin(&self, plugin_id: Uuid) -> Result<(), AppError> {
        plugin_hooks::Entity::delete_many()
            .filter(plugin_hooks::Column::PluginId.eq(plugin_id))
            .exec(&self.db)
            .await?;
        Ok(())
    }

    async fn update_hook_avg_ms(&self, hook_id: Uuid, avg_ms: i32) -> Result<(), AppError> {
        plugin_hooks::Entity::update_many()
            .col_expr(plugin_hooks::Column::AvgMs, Expr::value(avg_ms))
            .filter(plugin_hooks::Column::Id.eq(hook_id))
            .exec(&self.db)
            .await?;
        Ok(())
    }

    // ── UI Slots ──────────────────────────────────────────────────────────────

    async fn active_ui_slots(&self) -> Result<Vec<PluginUiSlot>, AppError> {
        let slots = plugin_ui_slots::Entity::find()
            .filter(plugin_ui_slots::Column::IsActive.eq(true))
            .order_by_asc(plugin_ui_slots::Column::LoadOrder)
            .all(&self.db)
            .await?;

        if slots.is_empty() {
            return Ok(vec![]);
        }

        // Fetch slugs for all referenced plugins in a single IN query
        let plugin_ids: Vec<Uuid> = slots.iter().map(|s| s.plugin_id).collect();
        let slug_map: std::collections::HashMap<Uuid, String> = plugins::Entity::find()
            .filter(plugins::Column::Id.is_in(plugin_ids))
            .all(&self.db)
            .await?
            .into_iter()
            .map(|p| (p.id, p.slug))
            .collect();

        let mut slots: Vec<PluginUiSlot> = slots
            .into_iter()
            .map(|m| {
                let slug = slug_map.get(&m.plugin_id).cloned().unwrap_or_default();
                slot_from_entity(m, slug)
            })
            .collect();

        // `load_order` alone is not enough: it is seeded per plugin, so two
        // plugins each declaring one slot both land on 100 and the planner picks
        // their order — widgets swapping places between refreshes.
        //
        // Slug is the tiebreak: stable, predictable, and overridable via
        // PATCH /api/admin/plugins/:slug/ui-slots/:slot_id.
        slots.sort_by(|a, b| {
            a.load_order
                .cmp(&b.load_order)
                .then_with(|| a.plugin_slug.cmp(&b.plugin_slug))
                .then_with(|| a.slot_name.cmp(&b.slot_name))
        });

        Ok(slots)
    }

    async fn ui_slots_for_plugin(&self, plugin_id: Uuid) -> Result<Vec<PluginUiSlot>, AppError> {
        let plugin_slug = plugins::Entity::find_by_id(plugin_id)
            .one(&self.db)
            .await?
            .map(|p| p.slug)
            .unwrap_or_default();

        Ok(plugin_ui_slots::Entity::find()
            .filter(plugin_ui_slots::Column::PluginId.eq(plugin_id))
            .order_by_asc(plugin_ui_slots::Column::LoadOrder)
            .all(&self.db)
            .await?
            .into_iter()
            .map(|m| slot_from_entity(m, plugin_slug.clone()))
            .collect())
    }

    async fn update_ui_slot(&self, id: Uuid, slot_name: String, load_order: i32) -> Result<PluginUiSlot, AppError> {
        let existing = plugin_ui_slots::Entity::find_by_id(id)
            .one(&self.db)
            .await?
            .ok_or(AppError::NotFound)?;
        let plugin_slug = plugins::Entity::find_by_id(existing.plugin_id)
            .one(&self.db)
            .await?
            .map(|p| p.slug)
            .unwrap_or_default();

        let mut active = existing.into_active_model();
        active.slot_name = Set(slot_name);
        active.load_order = Set(load_order);
        let updated = active.update(&self.db).await?;
        Ok(slot_from_entity(updated, plugin_slug))
    }

    async fn create_ui_slot(&self, data: NewPluginUiSlot) -> Result<PluginUiSlot, AppError> {
        let plugin_slug = plugins::Entity::find_by_id(data.plugin_id)
            .one(&self.db)
            .await?
            .map(|p| p.slug)
            .unwrap_or_default();

        let model = plugin_ui_slots::ActiveModel {
            id: Set(Uuid::new_v4()),
            plugin_id: Set(data.plugin_id),
            slot_name: Set(data.slot_name),
            asset_url: Set(data.asset_url),
            custom_element_tag: Set(data.custom_element_tag),
            props: Set(data.props),
            load_order: Set(data.load_order),
            is_active: Set(true),
        };
        Ok(slot_from_entity(model.insert(&self.db).await?, plugin_slug))
    }

    async fn delete_ui_slots_for_plugin(&self, plugin_id: Uuid) -> Result<(), AppError> {
        plugin_ui_slots::Entity::delete_many()
            .filter(plugin_ui_slots::Column::PluginId.eq(plugin_id))
            .exec(&self.db)
            .await?;
        Ok(())
    }

    // ── Logs ──────────────────────────────────────────────────────────────────

    async fn append_log(&self, entry: NewPluginLog) -> Result<(), AppError> {
        let model = plugin_logs::ActiveModel {
            id: Set(Uuid::new_v4()),
            plugin_id: Set(entry.plugin_id),
            level: Set(entry.level),
            hook_name: Set(entry.hook_name),
            duration_ms: Set(entry.duration_ms),
            message: Set(entry.message),
            context: Set(entry.context),
            ..Default::default()
        };
        model.insert(&self.db).await?;
        Ok(())
    }

    async fn append_logs_batch(&self, entries: Vec<NewPluginLog>) -> Result<(), AppError> {
        if entries.is_empty() {
            // `insert_many` with no rows is an error in sea-orm, not a no-op.
            return Ok(());
        }
        let models = entries.into_iter().map(|entry| plugin_logs::ActiveModel {
            id: Set(Uuid::new_v4()),
            plugin_id: Set(entry.plugin_id),
            level: Set(entry.level),
            hook_name: Set(entry.hook_name),
            duration_ms: Set(entry.duration_ms),
            message: Set(entry.message),
            context: Set(entry.context),
            ..Default::default()
        });
        plugin_logs::Entity::insert_many(models).exec(&self.db).await?;
        Ok(())
    }

    async fn get_logs(
        &self,
        plugin_id: Uuid,
        query: PluginLogQuery,
    ) -> Result<Vec<PluginLog>, AppError> {
        let mut q = plugin_logs::Entity::find()
            .filter(plugin_logs::Column::PluginId.eq(plugin_id));

        if let Some(level) = &query.level {
            q = q.filter(plugin_logs::Column::Level.eq(level.as_str()));
        }
        if let Some(hook_name) = &query.hook_name {
            q = q.filter(plugin_logs::Column::HookName.eq(hook_name.as_str()));
        }
        if let Some(since) = query.since {
            q = q.filter(plugin_logs::Column::CreatedAt.gt(since.fixed_offset()));
        }

        let limit = if query.limit == 0 { 100 } else { query.limit };

        Ok(q.order_by_desc(plugin_logs::Column::CreatedAt)
            .offset(query.offset)
            .limit(limit)
            .all(&self.db)
            .await?
            .into_iter()
            .map(log_from_entity)
            .collect())
    }

    async fn delete_old_logs(&self, retention_days: u32) -> Result<u64, AppError> {
        let cutoff = Utc::now() - chrono::Duration::days(retention_days as i64);
        let result = plugin_logs::Entity::delete_many()
            .filter(plugin_logs::Column::CreatedAt.lt(cutoff.fixed_offset()))
            .exec(&self.db)
            .await?;
        Ok(result.rows_affected)
    }
}
