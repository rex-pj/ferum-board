use std::sync::Arc;

use uuid::Uuid;

use crate::application::shared::AppError;
use crate::domain::models::user::UserRole;
use crate::domain::models::webhook::Webhook;
use crate::domain::repositories::webhook_repository::{NewWebhook, UpdateWebhook, WebhookRepository};
use crate::middleware::auth::AuthUser;

pub struct WebhookUseCase {
    pub webhooks: Arc<dyn WebhookRepository>,
}

impl WebhookUseCase {
    pub fn new(webhooks: Arc<dyn WebhookRepository>) -> Self {
        Self { webhooks }
    }

    fn require_admin(actor: &AuthUser) -> Result<(), AppError> {
        if actor.role != UserRole::Admin {
            return Err(AppError::forbidden("admin_required"));
        }
        Ok(())
    }

    pub async fn list(&self, actor: &AuthUser) -> Result<Vec<Webhook>, AppError> {
        Self::require_admin(actor)?;
        self.webhooks.list().await
    }

    pub async fn create(
        &self,
        actor: &AuthUser,
        url: String,
        events: Vec<String>,
        secret: Option<String>,
    ) -> Result<Webhook, AppError> {
        Self::require_admin(actor)?;
        validate_webhook_url(&url)?;
        if events.is_empty() {
            return Err(AppError::unprocessable("At least one event type is required"));
        }
        self.webhooks.create(NewWebhook { url, events, secret, created_by_id: Some(actor.id) }).await
    }

    pub async fn update(
        &self,
        actor: &AuthUser,
        id: Uuid,
        url: Option<String>,
        events: Option<Vec<String>>,
        secret: Option<String>,
        is_active: Option<bool>,
    ) -> Result<Webhook, AppError> {
        Self::require_admin(actor)?;
        if let Some(ref u) = url {
            validate_webhook_url(u)?;
        }
        self.webhooks.find_by_id(id).await?.ok_or(AppError::NotFound)?;
        self.webhooks.update(id, UpdateWebhook { url, events, secret, is_active }).await
    }

    pub async fn delete(&self, actor: &AuthUser, id: Uuid) -> Result<(), AppError> {
        Self::require_admin(actor)?;
        self.webhooks.find_by_id(id).await?.ok_or(AppError::NotFound)?;
        self.webhooks.delete(id).await
    }
}

/// Validate a webhook URL: must be http/https and must not target private or
/// loopback addresses (SSRF prevention).
fn validate_webhook_url(url: &str) -> Result<(), crate::application::shared::AppError> {
    use crate::application::shared::AppError;

    if url.is_empty() {
        return Err(AppError::unprocessable("Webhook URL is required"));
    }

    // Parse with the http crate's Uri to validate structure.
    let uri: axum::http::Uri = url
        .parse()
        .map_err(|_| AppError::unprocessable("Webhook URL is not a valid URI"))?;

    let scheme = uri.scheme_str().unwrap_or("");
    if scheme != "http" && scheme != "https" {
        return Err(AppError::unprocessable("Webhook URL must use http or https"));
    }

    let host = uri
        .host()
        .ok_or_else(|| AppError::unprocessable("Webhook URL must have a host"))?;

    if is_private_host(host) {
        return Err(AppError::unprocessable(
            "Webhook URL must not target private, loopback, or link-local addresses",
        ));
    }

    Ok(())
}

fn is_private_host(host: &str) -> bool {
    // Reject loopback / localhost
    if host == "localhost" || host == "::1" {
        return true;
    }
    // Parse as IPv4
    if let Ok(addr) = host.parse::<std::net::Ipv4Addr>() {
        return addr.is_loopback()
            || addr.is_private()
            || addr.is_link_local()
            || addr.is_broadcast()
            || addr.is_unspecified()
            // 169.254.0.0/16 (cloud metadata) — already covered by is_link_local
            || matches!(addr.octets(), [100, 64..=127, _, _]); // CGNAT
    }
    // Parse as IPv6
    if let Ok(addr) = host.parse::<std::net::Ipv6Addr>() {
        return addr.is_loopback() || addr.is_unspecified();
    }
    false
}
