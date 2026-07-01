use std::sync::Arc;

use uuid::Uuid;

use crate::permission::PermissionChecker;
use crate::shared::{AppError, OptionExt};
use ferum_domain::models::webhook::Webhook;
use ferum_domain::repositories::webhook_repository::{
    NewWebhook, UpdateWebhook, WebhookRepository,
};
use ferum_domain::AuthUser;

pub struct WebhookUseCase {
    pub webhooks: Arc<dyn WebhookRepository>,
}

impl WebhookUseCase {
    pub fn new(webhooks: Arc<dyn WebhookRepository>) -> Self {
        Self { webhooks }
    }

    #[tracing::instrument(skip(self, actor), fields(user_id = %actor.id))]
    pub async fn list(&self, actor: &AuthUser) -> Result<Vec<Webhook>, AppError> {
        PermissionChecker::can_manage_webhooks(actor)?;
        self.webhooks.list().await
    }

    #[tracing::instrument(skip(self, actor, url, events, secret), fields(user_id = %actor.id))]
    pub async fn create(
        &self,
        actor: &AuthUser,
        url: String,
        events: Vec<String>,
        secret: Option<String>,
    ) -> Result<Webhook, AppError> {
        PermissionChecker::can_manage_webhooks(actor)?;
        validate_webhook_url(&url)?;
        if events.is_empty() {
            return Err(AppError::unprocessable(
                "At least one event type is required",
            ));
        }
        self.webhooks
            .create(NewWebhook {
                url,
                events,
                secret,
                created_by_id: Some(actor.id),
                plugin_id: None,
            })
            .await
    }

    #[tracing::instrument(skip(self, actor, url, events, secret), fields(user_id = %actor.id, webhook_id = %id))]
    pub async fn update(
        &self,
        actor: &AuthUser,
        id: Uuid,
        url: Option<String>,
        events: Option<Vec<String>>,
        secret: Option<String>,
        is_active: Option<bool>,
    ) -> Result<Webhook, AppError> {
        PermissionChecker::can_manage_webhooks(actor)?;
        if let Some(ref u) = url {
            validate_webhook_url(u)?;
        }
        self.webhooks
            .find_by_id(id)
            .await?
            .or_not_found()?;
        self.webhooks
            .update(
                id,
                UpdateWebhook {
                    url,
                    events,
                    secret,
                    is_active,
                },
            )
            .await
    }

    #[tracing::instrument(skip(self, actor), fields(user_id = %actor.id, webhook_id = %id))]
    pub async fn delete(&self, actor: &AuthUser, id: Uuid) -> Result<(), AppError> {
        PermissionChecker::can_manage_webhooks(actor)?;
        self.webhooks
            .find_by_id(id)
            .await?
            .or_not_found()?;
        self.webhooks.delete(id).await
    }
}

/// Validate a webhook URL: must be http/https and must not target private or
/// loopback addresses (SSRF prevention). Also used by plugin-manifest-declared
/// webhooks — any code path that inserts a row into the webhooks table must
/// call this first.
pub(crate) fn validate_webhook_url(url: &str) -> Result<(), crate::shared::AppError> {
    use crate::shared::AppError;

    if url.is_empty() {
        return Err(AppError::unprocessable("Webhook URL is required"));
    }

    // Parse with the http crate's Uri to validate structure.
    let uri: http::Uri = url
        .parse()
        .map_err(|_| AppError::unprocessable("Webhook URL is not a valid URI"))?;

    let scheme = uri.scheme_str().unwrap_or("");
    if scheme != "http" && scheme != "https" {
        return Err(AppError::unprocessable(
            "Webhook URL must use http or https",
        ));
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
        let segments = addr.segments();
        // Loopback (::1), unspecified (::)
        let basic = addr.is_loopback() || addr.is_unspecified();
        // Unique Local Addresses: fc00::/7 (first segment high byte 0xfc or 0xfd)
        let is_ula = (segments[0] & 0xfe00) == 0xfc00;
        // Link-Local: fe80::/10
        let is_link_local = (segments[0] & 0xffc0) == 0xfe80;
        // IPv4-mapped private addresses (::ffff:192.168.x.x etc.)
        let is_v4_mapped_private = addr.to_ipv4_mapped().map_or(false, |v4| {
            v4.is_loopback() || v4.is_private() || v4.is_link_local() || v4.is_unspecified()
        });
        return basic || is_ula || is_link_local || is_v4_mapped_private;
    }
    false
}
