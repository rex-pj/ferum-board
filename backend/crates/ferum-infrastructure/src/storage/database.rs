use async_trait::async_trait;
use bytes::Bytes;
use sea_orm::{ActiveValue::Set, DatabaseConnection, EntityTrait};

use crate::entities::stored_files;
use ferum_application::ports::{StorageService, FILES_PREFIX};
use ferum_application::shared::AppError;

use super::url_shapes::{strip_files_prefix, under_base, without_query_or_fragment};

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

    /// Answers from the same table the sweep compares against, so this backend
    /// can never report an orphan.
    ///
    /// That is not a reason to skip it: the sweep also finds *rows* stuck at
    /// `ref_count <= 0` after a lost GC job, and those are real here. Returning
    /// `None` would make the tool claim it could not look, which is worse than
    /// looking and finding one class of problem instead of two.
    async fn list_keys(
        &self,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Option<Vec<String>>, AppError> {
        use sea_orm::{ColumnTrait, QueryFilter, QueryOrder, QuerySelect};

        let mut query = stored_files::Entity::find()
            // Never `find()` unfiltered here — the full model includes `data`,
            // and hydrating it would read every blob in the store to learn its
            // name. Same trap as `list_keys_with_prefix`.
            .select_only()
            .column(stored_files::Column::Key)
            .order_by_asc(stored_files::Column::Key)
            .limit(limit as u64);
        if let Some(cursor) = after {
            query = query.filter(stored_files::Column::Key.gt(cursor));
        }

        let keys = query
            .into_tuple::<String>()
            .all(&self.db)
            .await
            .map_err(|e| AppError::internal(format!("db storage list error: {e}")))?;
        Ok(Some(keys))
    }

    fn public_url(&self, key: &str) -> String {
        match &self.cdn_base_url {
            Some(base) => format!("{base}{FILES_PREFIX}{key}"),
            None => format!("{FILES_PREFIX}{key}"),
        }
    }

    /// Same two rules, same order, as the object-store backends — they live in
    /// `url_shapes` so this cannot drift from them again.
    ///
    /// This used to scan for the last `/files/` anywhere in the string, which
    /// resolved both shapes in one line and also claimed
    /// `https://anyone.example.com/files/avatars/a.jpg` as one of ours. Since
    /// `key_from_url` feeds `decrement_ref` and `delete_by_key`, that turned an
    /// externally-hosted URL into a way to release a CAS reference belonging to
    /// something else entirely.
    fn key_from_url(&self, url: &str) -> Option<String> {
        let url = without_query_or_fragment(url);
        // `{cdn}/files/{key}` — this backend's own absolute shape. Reported by
        // `under_base` as a resolver path, which is exactly what it is.
        if let Some(base) = &self.cdn_base_url {
            if let Some(found) = under_base(url, base) {
                return Some(found.into_key().to_string());
            }
        }
        // `/files/{key}` — the same-origin shape, and what `file_url` persists.
        strip_files_prefix(url).map(str::to_string)
    }
}
