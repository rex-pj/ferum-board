use async_trait::async_trait;
use chrono::Utc;
use sea_orm::prelude::*;
use sea_orm::*;
use uuid::Uuid;

use crate::entities::plugin_storage;
use ferum_application::shared::AppError;
use ferum_domain::repositories::plugin_storage_repository::PluginStorageRepository;

pub struct PgPluginStorageRepository {
    db: DatabaseConnection,
}

impl PgPluginStorageRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait]
impl PluginStorageRepository for PgPluginStorageRepository {
    async fn get(&self, plugin_id: Uuid, key: &str) -> Result<Option<serde_json::Value>, AppError> {
        Ok(plugin_storage::Entity::find_by_id((plugin_id, key.to_string()))
            .one(&self.db)
            .await?
            .map(|m| m.value))
    }

    async fn set(&self, plugin_id: Uuid, key: &str, value: serde_json::Value) -> Result<(), AppError> {
        let model = plugin_storage::ActiveModel {
            plugin_id: Set(plugin_id),
            key: Set(key.to_string()),
            value: Set(value),
            updated_at: Set(Utc::now().fixed_offset()),
        };
        plugin_storage::Entity::insert(model)
            .on_conflict(
                sea_orm::sea_query::OnConflict::columns([plugin_storage::Column::PluginId, plugin_storage::Column::Key])
                    .update_columns([plugin_storage::Column::Value, plugin_storage::Column::UpdatedAt])
                    .to_owned(),
            )
            .exec(&self.db)
            .await?;
        Ok(())
    }

    async fn delete(&self, plugin_id: Uuid, key: &str) -> Result<(), AppError> {
        plugin_storage::Entity::delete_by_id((plugin_id, key.to_string()))
            .exec(&self.db)
            .await?;
        Ok(())
    }

    async fn list_keys(&self, plugin_id: Uuid, prefix: &str, limit: u64) -> Result<Vec<String>, AppError> {
        let mut q = plugin_storage::Entity::find()
            .filter(plugin_storage::Column::PluginId.eq(plugin_id));
        if !prefix.is_empty() {
            // Escape LIKE metacharacters in the prefix so plugin-supplied text can't
            // widen the scan (e.g. a literal "%" or "_" matching unintended keys).
            let escaped = prefix.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
            q = q.filter(plugin_storage::Column::Key.like(format!("{escaped}%")));
        }
        Ok(q.order_by_asc(plugin_storage::Column::Key)
            .limit(limit)
            .all(&self.db)
            .await?
            .into_iter()
            .map(|m| m.key)
            .collect())
    }
}
