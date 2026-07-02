use async_trait::async_trait;
use uuid::Uuid;

use ferum_domain::models::plugin::{
    NewPlugin, NewPluginHook, NewPluginLog, NewPluginUiSlot, Plugin, PluginHook, PluginLog,
    PluginLogQuery, PluginStatus, PluginUiSlot,
};
use ferum_domain::repositories::plugin_repository::PluginRepository;
use ferum_domain::AppError;

mockall::mock! {
    pub PluginRepository {}

    #[async_trait]
    impl PluginRepository for PluginRepository {
        async fn list(&self) -> Result<Vec<Plugin>, AppError>;
        async fn find_by_id(&self, id: Uuid) -> Result<Option<Plugin>, AppError>;
        async fn find_by_slug(&self, slug: &str) -> Result<Option<Plugin>, AppError>;
        async fn create(&self, data: NewPlugin) -> Result<Plugin, AppError>;
        async fn update_status(&self, id: Uuid, status: PluginStatus, error_message: Option<String>) -> Result<(), AppError>;
        async fn update_config(&self, id: Uuid, config: serde_json::Value) -> Result<(), AppError>;
        async fn update_activated_at(&self, id: Uuid) -> Result<(), AppError>;
        async fn update_circuit_open(&self, id: Uuid, open: bool) -> Result<(), AppError>;
        async fn delete(&self, id: Uuid) -> Result<(), AppError>;

        async fn hooks_for_plugin(&self, plugin_id: Uuid) -> Result<Vec<PluginHook>, AppError>;
        async fn active_hooks_for(&self, hook_name: &str) -> Result<Vec<PluginHook>, AppError>;
        async fn create_hook(&self, data: NewPluginHook) -> Result<PluginHook, AppError>;
        async fn delete_hooks_for_plugin(&self, plugin_id: Uuid) -> Result<(), AppError>;
        async fn update_hook_avg_ms(&self, hook_id: Uuid, avg_ms: i32) -> Result<(), AppError>;

        async fn active_ui_slots(&self) -> Result<Vec<PluginUiSlot>, AppError>;
        async fn ui_slots_for_plugin(&self, plugin_id: Uuid) -> Result<Vec<PluginUiSlot>, AppError>;
        async fn create_ui_slot(&self, data: NewPluginUiSlot) -> Result<PluginUiSlot, AppError>;
        async fn update_ui_slot(&self, id: Uuid, slot_name: String, load_order: i32) -> Result<PluginUiSlot, AppError>;
        async fn delete_ui_slots_for_plugin(&self, plugin_id: Uuid) -> Result<(), AppError>;

        async fn append_log(&self, entry: NewPluginLog) -> Result<(), AppError>;
        async fn get_logs(&self, plugin_id: Uuid, query: PluginLogQuery) -> Result<Vec<PluginLog>, AppError>;
        async fn delete_old_logs(&self, retention_days: u32) -> Result<u64, AppError>;
    }
}
