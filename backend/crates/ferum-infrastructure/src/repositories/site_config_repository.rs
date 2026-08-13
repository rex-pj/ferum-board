
use sea_orm::prelude::*;
use sea_orm::*;
use std::collections::HashMap;
use std::sync::Arc;

use crate::crypto::{site_config_aad, SecretCipher, ENCRYPTED_CONFIG_KEYS};
use crate::entities::site_config;
use async_trait::async_trait;
use ferum_application::shared::AppError;
use ferum_domain::repositories::SiteConfigRepository;

pub struct PgSiteConfigRepository {
    db: DatabaseConnection,
    /// `None` means encryption at rest is not configured, and every value is
    /// read and written verbatim — the behaviour of every release before this
    /// one, which is what keeps enabling it optional.
    cipher: Option<Arc<SecretCipher>>,
}

impl PgSiteConfigRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db, cipher: None }
    }

    /// Encrypts [`ENCRYPTED_KEYS`] at rest.
    ///
    /// Builder-style so no existing construction site changes signature, matching
    /// `AuthUseCase::with_auto_verify_flag` and `JobExecutor::with_translator`.
    ///
    /// **This is the seam, and it is here rather than in a use case on purpose.**
    /// `site_config_cache` is filled straight from `get_all()` at startup and read
    /// by the rate-limit middleware, the admin page context, the settings handler
    /// and `site_ctx`. Decrypting anywhere above this point would leave every one
    /// of those holding ciphertext.
    pub fn with_cipher(mut self, cipher: Arc<SecretCipher>) -> Self {
        self.cipher = Some(cipher);
        self
    }

    /// Decrypts one value if its key is encrypted and a cipher is present.
    ///
    /// A value that is not sealed passes through unchanged, which is what makes
    /// turning encryption on a no-op for an existing database.
    fn decode(&self, key: &str, value: String) -> Result<String, AppError> {
        if !ENCRYPTED_CONFIG_KEYS.contains(&key) {
            return Ok(value);
        }
        match &self.cipher {
            Some(cipher) => Ok(cipher.open(&site_config_aad(key), &value)?.value),
            // No key configured. A sealed value cannot be read, and returning the
            // ciphertext as if it were the password would mean silently
            // authenticating to an SMTP server with `enc:v1:…`. Startup refuses to
            // boot in this state; this is the backstop if it is ever reached.
            None if SecretCipher::is_sealed(&value) => Err(AppError::internal(format!(
                "site_config `{key}` is encrypted but SECRET_ENCRYPTION_KEY is not set"
            ))),
            None => Ok(value),
        }
    }

    /// Encrypts one value if its key is encrypted and a cipher is present.
    fn encode(&self, key: &str, value: &str) -> Result<String, AppError> {
        match (&self.cipher, ENCRYPTED_CONFIG_KEYS.contains(&key)) {
            // Never seal an empty string. A blank secret means "not set", and the
            // presence checks that drive the admin UI test for non-blank — sealing
            // `""` would produce a non-blank ciphertext and report a password that
            // does not exist.
            (Some(cipher), true) if !value.is_empty() => cipher.seal(&site_config_aad(key), value),
            _ => Ok(value.to_string()),
        }
    }
}

#[async_trait]
impl SiteConfigRepository for PgSiteConfigRepository {
    async fn get_all(&self) -> Result<HashMap<String, String>, AppError> {
        let rows = site_config::Entity::find().all(&self.db).await?;
        rows.into_iter()
            .map(|r| {
                let value = self.decode(&r.key, r.value)?;
                Ok((r.key, value))
            })
            .collect()
    }

    async fn get(&self, key: &str) -> Result<Option<String>, AppError> {
        match site_config::Entity::find_by_id(key).one(&self.db).await? {
            Some(row) => Ok(Some(self.decode(key, row.value)?)),
            None => Ok(None),
        }
    }

    async fn set(&self, key: &str, value: &str) -> Result<(), AppError> {
        let model = site_config::ActiveModel {
            key: Set(key.to_string()),
            value: Set(self.encode(key, value)?),
            updated_at: Set(chrono::Utc::now().fixed_offset()),
            updated_by_id: NotSet,
        };
        site_config::Entity::insert(model)
            .on_conflict(
                sea_orm::sea_query::OnConflict::column(site_config::Column::Key)
                    .update_columns([site_config::Column::Value, site_config::Column::UpdatedAt])
                    .to_owned(),
            )
            .exec(&self.db)
            .await?;
        Ok(())
    }

    /// One multi-row upsert, not one round trip per key.
    ///
    /// `PUT /api/admin/config` writes the whole settings form at once — around
    /// eighteen keys — so the previous `for (k, v) in entries { self.set(..) }`
    /// cost eighteen sequential statements to save one page. It was also not
    /// atomic: a failure partway left some keys written and the rest not, with
    /// no indication of where it stopped. A single INSERT … ON CONFLICT applies
    /// all of them or none.
    async fn set_many(&self, entries: &HashMap<String, String>) -> Result<(), AppError> {
        let now = chrono::Utc::now().fixed_offset();
        let rows: Vec<site_config::ActiveModel> = entries
            .iter()
            .map(|(key, value)| {
                Ok(site_config::ActiveModel {
                    key: Set(key.clone()),
                    value: Set(self.encode(key, value)?),
                    updated_at: Set(now),
                    updated_by_id: NotSet,
                })
            })
            .collect::<Result<_, AppError>>()?;

        // sea-orm 2.0 returns Ok (rather than `DbErr::RecordNotInserted`) for an
        // empty iterator, so an empty map needs no guard of its own.
        site_config::Entity::insert_many(rows)
            .on_conflict(
                sea_orm::sea_query::OnConflict::column(site_config::Column::Key)
                    .update_columns([site_config::Column::Value, site_config::Column::UpdatedAt])
                    .to_owned(),
            )
            .exec(&self.db)
            .await?;
        Ok(())
    }
}
