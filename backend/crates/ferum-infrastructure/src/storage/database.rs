use async_trait::async_trait;
use bytes::Bytes;
use sea_orm::{ActiveValue::Set, DatabaseConnection, EntityTrait};

use crate::entities::stored_files;
use ferum_application::ports::{StorageService, LEGACY_FILES_PREFIX};
use ferum_application::shared::AppError;

/// Stores blob bytes in `stored_files.data`.
///
/// The database backend is unusual among `StorageService` implementations in
/// that its bytes share a row with the metadata the repository owns. That makes
/// the division of labour worth stating explicitly, because getting it wrong is
/// silent: **this type owns `data`, `content_type` and `size`; it never touches
/// `ref_count`.** Reference counting belongs to `StoredFileRepository`, which
/// runs immediately after every `put`.
pub struct DatabaseStorageService {
    db: DatabaseConnection,
    /// Optional origin to prefix onto file URLs.
    ///
    /// `CDN_BASE_URL` used to be read only by the S3 adapter, so setting it with
    /// database storage did nothing at all — no error, no warning, just
    /// same-origin URLs and an operator wondering why their edge cache saw no
    /// traffic. A pull-CDN pointed at this app serves `/files/` perfectly well,
    /// so honouring it here costs nothing and makes the variable mean the same
    /// thing under both backends.
    cdn_base_url: Option<String>,
}

impl DatabaseStorageService {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db, cdn_base_url: None }
    }

    pub fn with_cdn_base_url(mut self, cdn_base_url: Option<&str>) -> Self {
        self.cdn_base_url = cdn_base_url
            .map(|u| u.trim_end_matches('/').to_string())
            .filter(|u| !u.is_empty());
        self
    }
}

#[async_trait]
impl StorageService for DatabaseStorageService {
    async fn put(&self, key: &str, data: Bytes, content_type: &str) -> Result<(), AppError> {
        let size = data.len() as i64;
        let model = stored_files::ActiveModel {
            key: Set(key.to_string()),
            content_type: Set(content_type.to_string()),
            data: Set(Some(data.to_vec())),
            size: Set(size),
            // Explicit, and load-bearing: the column DEFAULTs to 1, so letting
            // this fall through would leave a fresh row already referenced once.
            // The repository call that follows every `put` is what takes the
            // first reference — counting it here too would double it, and a file
            // whose count never reaches 0 is never collected.
            ref_count: Set(0),
            ..Default::default()
        };
        stored_files::Entity::insert(model)
            .on_conflict(
                sea_orm::sea_query::OnConflict::column(stored_files::Column::Key)
                    // `ref_count` is deliberately absent: re-uploading identical
                    // bytes (which CAS makes routine) must not reset how many
                    // posts already reference them.
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
        match &self.cdn_base_url {
            Some(base) => format!("{base}{LEGACY_FILES_PREFIX}{key}"),
            None => format!("{LEGACY_FILES_PREFIX}{key}"),
        }
    }

    fn key_from_url(&self, url: &str) -> Option<String> {
        // Both shapes this backend can have emitted end in the same
        // `/files/{key}` suffix, so one rule covers the CDN-prefixed form, the
        // same-origin form, and anything written before `CDN_BASE_URL` was set.
        // `rfind` rather than `find` so a key that somehow contains the literal
        // "/files/" still resolves to the last segment.
        url.rfind(LEGACY_FILES_PREFIX)
            .map(|i| url[i + LEGACY_FILES_PREFIX.len()..].to_string())
            .filter(|k| !k.is_empty())
    }
}
