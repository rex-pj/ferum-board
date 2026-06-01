use async_trait::async_trait;
use bytes::Bytes;
use sea_orm::{ActiveValue::Set, DatabaseConnection, EntityTrait};

use crate::application::ports::StorageService;
use crate::application::shared::AppError;
use crate::entities::stored_files;

pub struct DatabaseStorageService {
    db: DatabaseConnection,
}

impl DatabaseStorageService {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait]
impl StorageService for DatabaseStorageService {
    async fn put(&self, key: &str, data: Bytes, content_type: &str) -> Result<(), AppError> {
        let size = data.len() as i64;
        let model = stored_files::ActiveModel {
            key: Set(key.to_string()),
            content_type: Set(content_type.to_string()),
            data: Set(data.to_vec()),
            size: Set(size),
            ..Default::default()
        };
        // Upsert: insert or update on conflict
        stored_files::Entity::insert(model)
            .on_conflict(
                sea_orm::sea_query::OnConflict::column(stored_files::Column::Key)
                    .update_columns([
                        stored_files::Column::ContentType,
                        stored_files::Column::Data,
                        stored_files::Column::Size,
                    ])
                    .to_owned(),
            )
            .exec(&self.db)
            .await
            .map_err(|e| AppError::internal(format!("db storage put error: {e}")))?;
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), AppError> {
        stored_files::Entity::delete_by_id(key)
            .exec(&self.db)
            .await
            .map_err(|e| AppError::internal(format!("db storage delete error: {e}")))?;
        Ok(())
    }

    fn public_url(&self, key: &str) -> String {
        format!("/files/{key}")
    }
}
