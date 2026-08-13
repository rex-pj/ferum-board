use async_trait::async_trait;
use chrono::Utc;
use sea_orm::prelude::*;
use sea_orm::*;
use std::sync::Arc;
use uuid::Uuid;

use crate::crypto::{SecretCipher, WEBHOOK_SECRET_AAD};
use crate::entities::webhooks;
use ferum_application::constants::{MAX_WEBHOOKS_PER_EVENT, WEBHOOK_MAX_FAILURES};
use ferum_application::shared::AppError;
use ferum_domain::models::webhook::Webhook;
use ferum_domain::repositories::webhook_repository::{
    NewWebhook, UpdateWebhook, WebhookRepository,
};

pub struct PgWebhookRepository {
    db: DatabaseConnection,
    cipher: Option<Arc<SecretCipher>>,
}

impl PgWebhookRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db, cipher: None }
    }

    /// Encrypts `webhooks.secret` at rest.
    ///
    /// **The seam is `entity_to_domain`, and its position is load-bearing.**
    /// Decryption has to happen while an error can still propagate: every arm of
    /// `dispatch_webhook` returns `Ok(())` and records a failure on the row, so a
    /// decrypt error at dispatch time would vanish into `failure_count` with
    /// nothing anywhere explaining why every subscriber started rejecting
    /// signatures. Here it surfaces before `EventBus` ever reads the secret.
    pub fn with_cipher(mut self, cipher: Arc<SecretCipher>) -> Self {
        self.cipher = Some(cipher);
        self
    }

    fn to_domain(&self, m: webhooks::Model) -> Result<Webhook, AppError> {
        let secret = match (m.secret, &self.cipher) {
            (Some(stored), Some(cipher)) => Some(cipher.open(WEBHOOK_SECRET_AAD, &stored)?.value),
            // No key configured but the row is sealed. This secret is an HMAC key:
            // handing back the ciphertext would sign every payload with `enc:v1:…`
            // and every subscriber would reject it, while `record_failure` ticked
            // up silently. Startup refuses to boot in this state; this is the
            // backstop.
            (Some(stored), None) if SecretCipher::is_sealed(&stored) => {
                return Err(AppError::internal(format!(
                    "webhook {} has an encrypted secret but SECRET_ENCRYPTION_KEY is not set",
                    m.id
                )))
            }
            (other, _) => other,
        };

        Ok(Webhook {
            id: m.id,
            url: m.url,
            events: m.events,
            secret,
            is_active: m.is_active,
            created_at: m.created_at.with_timezone(&Utc),
            updated_at: m.updated_at.map(|t| t.with_timezone(&Utc)),
            created_by_id: m.created_by_id,
            last_triggered_at: m.last_triggered_at.map(|t| t.with_timezone(&Utc)),
            failure_count: m.failure_count,
            plugin_id: m.plugin_id,
        })
    }

    /// Seals a secret on the way in. A blank secret is treated as absent rather
    /// than sealed, so `has_secret` in the API stays truthful.
    fn seal_secret(&self, secret: Option<String>) -> Result<Option<String>, AppError> {
        match (secret, &self.cipher) {
            (Some(s), _) if s.is_empty() => Ok(None),
            (Some(s), Some(cipher)) => Ok(Some(cipher.seal(WEBHOOK_SECRET_AAD, &s)?)),
            (other, _) => Ok(other),
        }
    }
}

#[async_trait]
impl WebhookRepository for PgWebhookRepository {
    async fn list(&self) -> Result<Vec<Webhook>, AppError> {
        webhooks::Entity::find()
            .order_by_asc(webhooks::Column::CreatedAt)
            .all(&self.db)
            .await?
            .into_iter()
            .map(|m| self.to_domain(m))
            .collect()
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<Webhook>, AppError> {
        match webhooks::Entity::find_by_id(id).one(&self.db).await? {
            Some(m) => Ok(Some(self.to_domain(m)?)),
            None => Ok(None),
        }
    }

    async fn find_subscribed(&self, event_type: &str) -> Result<Vec<Webhook>, AppError> {
        // Bounded because the caller fans out one job per row: `EventBus`
        // enqueues a `SendWebhook` for every hook returned, so this row count
        // is a multiplier on the work a single post creates. The limit is a
        // backstop against a misconfigured or hostile install, not a number a
        // real deployment should ever reach — subscribing more than this to one
        // event is already a sign something is wrong.
        webhooks::Entity::find()
            .filter(webhooks::Column::IsActive.eq(true))
            .filter(Expr::cust_with_values(
                "$1 = ANY(events)",
                [event_type.to_string()],
            ))
            .order_by_asc(webhooks::Column::CreatedAt)
            .limit(MAX_WEBHOOKS_PER_EVENT)
            .all(&self.db)
            .await?
            .into_iter()
            .map(|m| self.to_domain(m))
            .collect()
    }

    async fn create(&self, new: NewWebhook) -> Result<Webhook, AppError> {
        let model = webhooks::ActiveModel {
            id: Set(Uuid::new_v4()),
            url: Set(new.url),
            events: Set(new.events),
            secret: Set(self.seal_secret(new.secret)?),
            is_active: Set(true),
            created_by_id: Set(new.created_by_id),
            plugin_id: Set(new.plugin_id),
            ..Default::default()
        };
        self.to_domain(model.insert(&self.db).await?)
    }

    async fn update(&self, id: Uuid, upd: UpdateWebhook) -> Result<Webhook, AppError> {
        let existing = webhooks::Entity::find_by_id(id)
            .one(&self.db)
            .await?
            .ok_or(AppError::NotFound)?;

        let mut active: webhooks::ActiveModel = existing.into();
        if let Some(url) = upd.url {
            active.url = Set(url);
        }
        if let Some(events) = upd.events {
            active.events = Set(events);
        }
        if let Some(secret) = upd.secret {
            active.secret = Set(self.seal_secret(Some(secret))?);
        }
        if let Some(is_active) = upd.is_active {
            active.is_active = Set(is_active);
        }

        self.to_domain(active.update(&self.db).await?)
    }

    async fn delete(&self, id: Uuid) -> Result<(), AppError> {
        webhooks::Entity::delete_by_id(id).exec(&self.db).await?;
        Ok(())
    }

    async fn delete_by_plugin(&self, plugin_id: Uuid) -> Result<(), AppError> {
        webhooks::Entity::delete_many()
            .filter(webhooks::Column::PluginId.eq(plugin_id))
            .exec(&self.db)
            .await?;
        Ok(())
    }

    async fn record_success(&self, id: Uuid) -> Result<(), AppError> {
        webhooks::Entity::update_many()
            .col_expr(
                webhooks::Column::LastTriggeredAt,
                Expr::value(Utc::now().fixed_offset()),
            )
            .col_expr(webhooks::Column::FailureCount, Expr::value(0i32))
            .filter(webhooks::Column::Id.eq(id))
            .exec(&self.db)
            .await?;
        Ok(())
    }

    async fn record_failure(&self, id: Uuid) -> Result<(), AppError> {
        webhooks::Entity::update_many()
            .col_expr(
                webhooks::Column::FailureCount,
                Expr::col(webhooks::Column::FailureCount).add(1i32),
            )
            .filter(webhooks::Column::Id.eq(id))
            .exec(&self.db)
            .await?;

        // Auto-disable if threshold reached
        webhooks::Entity::update_many()
            .col_expr(webhooks::Column::IsActive, Expr::value(false))
            .filter(webhooks::Column::Id.eq(id))
            .filter(webhooks::Column::FailureCount.gte(WEBHOOK_MAX_FAILURES))
            .exec(&self.db)
            .await?;

        Ok(())
    }
}
