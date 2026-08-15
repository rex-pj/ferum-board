use std::sync::Arc;

use uuid::Uuid;

use crate::permission::PermissionChecker;
use crate::ports::{HostResolver, WebhookDeliveryService, WebhookTestResult};
use crate::shared::{AppError, OptionExt};
use ferum_domain::models::webhook::Webhook;
use ferum_domain::repositories::webhook_repository::{
    NewWebhook, UpdateWebhook, WebhookRepository,
};
use ferum_domain::AuthUser;

pub struct WebhookUseCase {
    pub webhooks: Arc<dyn WebhookRepository>,
    pub delivery: Arc<dyn WebhookDeliveryService>,
    pub resolver: Arc<dyn HostResolver>,
}

impl WebhookUseCase {
    pub fn new(
        webhooks: Arc<dyn WebhookRepository>,
        delivery: Arc<dyn WebhookDeliveryService>,
        resolver: Arc<dyn HostResolver>,
    ) -> Self {
        Self {
            webhooks,
            delivery,
            resolver,
        }
    }

    /// Rejects a URL resolving to a private address, on the admin save path
    /// only — so the admin is told now rather than saving a row that fails at
    /// dispatch. **Not the security boundary**: `build_pinned_client` guards
    /// delivery and catches a DNS record flipped afterwards.
    ///
    /// A resolution failure is not fatal — a host reachable only from the
    /// deployment network is a legitimate target.
    async fn assert_hostname_not_private(&self, url: &str) -> Result<(), AppError> {
        let Some(host) = url.parse::<http::Uri>().ok().and_then(|u| {
            u.host()
                .map(|h| ferum_domain::net::normalize_host(h).to_string())
        }) else {
            return Ok(()); // already rejected by validate_webhook_url
        };
        // IP literals were settled synchronously; there is nothing to resolve.
        if host.parse::<std::net::IpAddr>().is_ok() {
            return Ok(());
        }
        match self.resolver.resolve(&host).await {
            Ok(addrs) => {
                if let Some(bad) = addrs
                    .into_iter()
                    .find(|ip| ferum_domain::net::is_private_ip(*ip))
                {
                    return Err(AppError::unprocessable(&format!(
                        "Webhook host {host} resolves to {bad}, a private or reserved address"
                    )));
                }
                Ok(())
            }
            Err(e) => {
                tracing::warn!(%host, error = %e, "webhook host DNS check skipped: resolution failed");
                Ok(())
            }
        }
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
        self.assert_hostname_not_private(&url).await?;
        if events.is_empty() {
            return Err(AppError::invalid("webhook_events_required"));
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
            self.assert_hostname_not_private(u).await?;
        }
        self.webhooks.find_by_id(id).await?.or_not_found()?;
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

    /// Sends a one-off test POST to the webhook's URL — for the admin "Test"
    /// button, so they can confirm the endpoint is reachable and correctly
    /// verifies the signature before relying on it for real events.
    #[tracing::instrument(skip(self, actor), fields(user_id = %actor.id, webhook_id = %id))]
    pub async fn test_delivery(
        &self,
        actor: &AuthUser,
        id: Uuid,
    ) -> Result<WebhookTestResult, AppError> {
        PermissionChecker::can_manage_webhooks(actor)?;
        let webhook = self.webhooks.find_by_id(id).await?.or_not_found()?;
        self.delivery
            .send_test(&webhook.url, webhook.secret.as_deref())
            .await
    }

    #[tracing::instrument(skip(self, actor), fields(user_id = %actor.id, webhook_id = %id))]
    pub async fn delete(&self, actor: &AuthUser, id: Uuid) -> Result<(), AppError> {
        PermissionChecker::can_manage_webhooks(actor)?;
        self.webhooks.find_by_id(id).await?.or_not_found()?;
        self.webhooks.delete(id).await
    }
}

/// Syntactic validation of a webhook URL: must be https, must have a host, and
/// must not be an IP *literal* in a private/loopback/link-local range. Also used
/// by plugin-manifest-declared webhooks — any code path that inserts a row into
/// the webhooks table must call this first.
///
/// Deliberately synchronous, so it cannot see that `evil.example.com` resolves
/// to 127.0.0.1. `WebhookUseCase::assert_hostname_not_private` adds that DNS
/// check on the admin path; the actual SSRF boundary is the DNS pinning in
/// `build_pinned_client` at dispatch time.
///
/// **`http` used to be accepted.** The HMAC signature protects integrity, not
/// confidentiality, so a plaintext delivery puts post bodies, author identity and
/// category on the wire in the clear. The usual argument for allowing it — a
/// receiver on the local network — does not apply here: private addresses are
/// already refused, both as literals below and by DNS pinning at dispatch, so
/// `http` could only ever reach a *public* plaintext endpoint.
///
/// Existing `http` rows are NOT rewritten and keep being delivered; they are
/// warned about at dispatch instead. Failing them silently would look like the
/// receiver breaking.
pub fn validate_webhook_url(url: &str) -> Result<(), crate::shared::AppError> {
    use crate::shared::AppError;

    if url.is_empty() {
        return Err(AppError::invalid("webhook_url_required"));
    }

    // Parse with the http crate's Uri to validate structure.
    let uri: http::Uri = url
        .parse()
        .map_err(|_| AppError::invalid("webhook_url_invalid"))?;

    if uri.scheme_str() != Some("https") {
        return Err(AppError::invalid("webhook_url_scheme"));
    }

    let host = uri
        .host()
        .ok_or_else(|| AppError::invalid("webhook_url_missing_host"))?;

    if ferum_domain::net::is_private_host_literal(host) {
        return Err(AppError::invalid("webhook_url_private_address"));
    }

    Ok(())
}
