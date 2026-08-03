use async_trait::async_trait;
use uuid::Uuid;

use crate::models::plugin::{
    NewPlugin, NewPluginHook, NewPluginLog, NewPluginUiSlot, Plugin, PluginHook, PluginLog,
    PluginLogQuery, PluginStatus, PluginUiSlot,
};
use crate::AppError;

#[async_trait]
pub trait PluginRepository: Send + Sync {
    async fn list(&self) -> Result<Vec<Plugin>, AppError>;
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Plugin>, AppError>;
    async fn find_by_slug(&self, slug: &str) -> Result<Option<Plugin>, AppError>;
    async fn create(&self, data: NewPlugin) -> Result<Plugin, AppError>;
    async fn update_status(
        &self,
        id: Uuid,
        status: PluginStatus,
        error_message: Option<String>,
    ) -> Result<(), AppError>;
    async fn update_config(&self, id: Uuid, config: serde_json::Value) -> Result<(), AppError>;
    async fn update_activated_at(&self, id: Uuid) -> Result<(), AppError>;
    async fn update_circuit_open(&self, id: Uuid, open: bool) -> Result<(), AppError>;
    async fn delete(&self, id: Uuid) -> Result<(), AppError>;

    // ── Hook management ───────────────────────────────────────────────────────
    async fn hooks_for_plugin(&self, plugin_id: Uuid) -> Result<Vec<PluginHook>, AppError>;
    /// Returns active hooks for a given hook name, sorted by priority ASC.
    async fn active_hooks_for(&self, hook_name: &str) -> Result<Vec<PluginHook>, AppError>;
    async fn create_hook(&self, data: NewPluginHook) -> Result<PluginHook, AppError>;
    async fn delete_hooks_for_plugin(&self, plugin_id: Uuid) -> Result<(), AppError>;
    async fn update_hook_avg_ms(&self, hook_id: Uuid, avg_ms: i32) -> Result<(), AppError>;

    // ── UI Slot management ────────────────────────────────────────────────────
    /// Returns active UI slots, sorted by load_order ASC, for frontend hydration.
    async fn active_ui_slots(&self) -> Result<Vec<PluginUiSlot>, AppError>;
    /// Returns all UI slots owned by a plugin (active or not), for the admin
    /// placement editor.
    async fn ui_slots_for_plugin(&self, plugin_id: Uuid) -> Result<Vec<PluginUiSlot>, AppError>;
    async fn create_ui_slot(&self, data: NewPluginUiSlot) -> Result<PluginUiSlot, AppError>;
    /// Admin override: move a slot to a different named position and/or load order,
    /// without requiring the plugin to be deactivated/reactivated.
    async fn update_ui_slot(&self, id: Uuid, slot_name: String, load_order: i32) -> Result<PluginUiSlot, AppError>;
    async fn delete_ui_slots_for_plugin(&self, plugin_id: Uuid) -> Result<(), AppError>;

    // ── Log management ────────────────────────────────────────────────────────
    async fn append_log(&self, entry: NewPluginLog) -> Result<(), AppError>;
    /// Insert many log entries in one statement.
    ///
    /// Plugin logging is caller-driven — a loop in plugin script can emit
    /// thousands of entries — so the write path has to cost one round trip per
    /// batch rather than one per line, or logging becomes a way to exhaust the
    /// connection pool.
    async fn append_logs_batch(&self, entries: Vec<NewPluginLog>) -> Result<(), AppError>;
    async fn get_logs(
        &self,
        plugin_id: Uuid,
        query: PluginLogQuery,
    ) -> Result<Vec<PluginLog>, AppError>;
    /// Delete logs older than the given number of days.
    async fn delete_old_logs(&self, retention_days: u32) -> Result<u64, AppError>;
}
