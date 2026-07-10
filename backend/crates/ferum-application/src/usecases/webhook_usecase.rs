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
        Self { webhooks, delivery, resolver }
    }

    /// Rejects a URL whose hostname *resolves* to a private address. Runs only
    /// on the admin-facing create/update path, where the point is to tell the
    /// admin their URL is unusable rather than to let them save a row that will
    /// be silently refused at dispatch. Delivery is still guarded independently
    /// by `build_pinned_client`, so a DNS record flipped after this check is
    /// caught there — this cannot and does not try to be the security boundary.
    ///
    /// A resolution failure is *not* fatal: a host that is merely unreachable
    /// right now (or resolves only from the network the app deploys into) is a
    /// legitimate webhook target, and failing closed here would make webhooks
    /// unconfigurable in those environments.
    async fn assert_hostname_not_private(&self, url: &str) -> Result<(), AppError> {
        let Some(host) = url
            .parse::<http::Uri>()
            .ok()
            .and_then(|u| u.host().map(|h| ferum_domain::net::normalize_host(h).to_string()))
        else {
            return Ok(()); // already rejected by validate_webhook_url
        };
        // IP literals were settled synchronously; there is nothing to resolve.
        if host.parse::<std::net::IpAddr>().is_ok() {
            return Ok(());
        }
        match self.resolver.resolve(&host).await {
            Ok(addrs) => {
                if let Some(bad) = addrs.into_iter().find(|ip| ferum_domain::net::is_private_ip(*ip)) {
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
            self.assert_hostname_not_private(u).await?;
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

    /// Sends a one-off test POST to the webhook's URL — for the admin "Test"
    /// button, so they can confirm the endpoint is reachable and correctly
    /// verifies the signature before relying on it for real events.
    #[tracing::instrument(skip(self, actor), fields(user_id = %actor.id, webhook_id = %id))]
    pub async fn test_delivery(&self, actor: &AuthUser, id: Uuid) -> Result<WebhookTestResult, AppError> {
        PermissionChecker::can_manage_webhooks(actor)?;
        let webhook = self.webhooks.find_by_id(id).await?.or_not_found()?;
        self.delivery.send_test(&webhook.url, webhook.secret.as_deref()).await
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

/// Syntactic validation of a webhook URL: must be http/https, must have a host,
/// and must not be an IP *literal* in a private/loopback/link-local range. Also
/// used by plugin-manifest-declared webhooks — any code path that inserts a row
/// into the webhooks table must call this first.
///
/// Deliberately synchronous, so it cannot see that `evil.example.com` resolves
/// to 127.0.0.1. `WebhookUseCase::assert_hostname_not_private` adds that DNS
/// check on the admin path; the actual SSRF boundary is the DNS pinning in
/// `build_pinned_client` at dispatch time.
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

    if ferum_domain::net::is_private_host_literal(host) {
        return Err(AppError::unprocessable(
            "Webhook URL must not target private, loopback, or link-local addresses",
        ));
    }

    Ok(())
}

#[cfg(test)]
mod validate_webhook_url_tests {
    use super::validate_webhook_url;

    #[test]
    fn rejects_ipv4_private_and_loopback_literals() {
        for u in [
            "http://127.0.0.1/hook",
            "http://10.0.0.5/hook",
            "http://192.168.1.1/hook",
            "http://169.254.169.254/latest/meta-data",
            "http://localhost:8080/hook",
        ] {
            assert!(validate_webhook_url(u).is_err(), "{u} must be rejected");
        }
    }

    /// `http::Uri::host()` keeps the brackets on IPv6 literals, so a check that
    /// compares the raw host against "::1" never fires. Regression guard.
    #[test]
    fn rejects_bracketed_ipv6_private_and_loopback_literals() {
        for u in [
            "http://[::1]/hook",
            "http://[::1]:9000/hook",
            "http://[fd00::1]/hook",
            "http://[fe80::1]/hook",
            "http://[::ffff:127.0.0.1]/hook",
        ] {
            assert!(validate_webhook_url(u).is_err(), "{u} must be rejected");
        }
    }

    #[test]
    fn rejects_non_http_schemes_and_empty() {
        assert!(validate_webhook_url("").is_err());
        assert!(validate_webhook_url("ftp://example.com/x").is_err());
        assert!(validate_webhook_url("file:///etc/passwd").is_err());
    }

    #[test]
    fn accepts_public_hosts() {
        for u in [
            "https://hooks.slack.com/services/abc",
            "http://example.com:8080/hook",
            "https://8.8.8.8/hook",
            "https://[2606:4700::1111]/hook",
        ] {
            assert!(validate_webhook_url(u).is_ok(), "{u} must be accepted");
        }
    }
}
