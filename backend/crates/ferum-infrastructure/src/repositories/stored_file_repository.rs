use async_trait::async_trait;
use sea_orm::prelude::*;
use sea_orm::sea_query::OnConflict;
use sea_orm::*;
use uuid::Uuid;

use crate::entities::stored_files;
use ferum_application::shared::AppError;
use ferum_domain::repositories::stored_file_repository::StoredFileRepository;

pub struct PgStoredFileRepository {
    db: DatabaseConnection,
}

impl PgStoredFileRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait]
impl StoredFileRepository for PgStoredFileRepository {
    async fn exists(&self, key: &str) -> Result<bool, AppError> {
        let count = stored_files::Entity::find_by_id(key)
            .count(&self.db)
            .await?;
        Ok(count > 0)
    }

    async fn upsert(
        &self,
        key: &str,
        content_type: &str,
        data: &[u8],
        size: i64,
        uploaded_by_id: Option<Uuid>,
    ) -> Result<(), AppError> {
        let model = stored_files::ActiveModel {
            key: Set(key.to_string()),
            content_type: Set(content_type.to_string()),
            data: Set(data.to_vec()),
            size: Set(size),
            ref_count: Set(1),
            uploaded_by_id: Set(uploaded_by_id),
            ..Default::default()
        };
        stored_files::Entity::insert(model)
            .on_conflict(
                OnConflict::column(stored_files::Column::Key)
                    .do_nothing()
                    .to_owned(),
            )
            .exec(&self.db)
            .await
            .map(|_| ())
            .or_else(|e| {
                // RecordNotInserted = key already exists, that's fine
                if matches!(e, DbErr::RecordNotInserted) {
                    Ok(())
                } else {
                    Err(AppError::internal(e.to_string()))
                }
            })
    }

    async fn increment_ref(&self, key: &str) -> Result<i32, AppError> {
        self.db
            .execute(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "UPDATE stored_files SET ref_count = ref_count + 1 WHERE key = $1",
                [key.into()],
            ))
            .await?;

        let row = stored_files::Entity::find_by_id(key)
            .one(&self.db)
            .await?
            .ok_or_else(|| AppError::NotFound)?;
        Ok(row.ref_count)
    }

    async fn decrement_ref(&self, key: &str) -> Result<i32, AppError> {
        self.db
            .execute(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "UPDATE stored_files SET ref_count = GREATEST(ref_count - 1, 0) WHERE key = $1",
                [key.into()],
            ))
            .await?;

        let row = stored_files::Entity::find_by_id(key).one(&self.db).await?;
        Ok(row.map(|r| r.ref_count).unwrap_or(0))
    }

    async fn delete_by_key(&self, key: &str) -> Result<(), AppError> {
        stored_files::Entity::delete_by_id(key)
            .exec(&self.db)
            .await?;
        Ok(())
    }
}
