use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;

use ferum_application::ports::EmailService;
use ferum_application::shared::AppError;

use super::LettreEmailService;

/// An `EmailService` whose SMTP transport can be replaced at runtime.
///
/// SMTP settings live in `site_config` and are editable from `/admin/settings`
/// (see the seeding block in `startup.rs`), so the transport cannot be a value
/// captured once at boot. This mirrors the atomic-`Arc`-swap idiom `TeraEngine`
/// uses for theme reload: a send in flight keeps the transport it started with,
/// and the next send picks up the new one — no restart, no dropped mail.
///
/// `None` inside means SMTP is not configured. Sends then fail fast with
/// `smtp_not_configured` rather than timing out against a placeholder host.
pub struct ReloadableEmailService {
    transport: RwLock<Option<Arc<LettreEmailService>>>,
    /// True when SMTP is *not* configured, i.e. new registrations must be
    /// auto-verified because no verification mail can be delivered. Shared by
    /// `Arc` with `AuthUseCase`, so configuring SMTP from the admin UI turns
    /// email verification back on immediately instead of at the next restart.
    auto_verify: Arc<AtomicBool>,
    from: String,
}

impl ReloadableEmailService {
    /// Starts unconfigured; call [`Self::reload`] once site config is available.
    pub fn new(from: &str) -> Self {
        Self {
            transport: RwLock::new(None),
            auto_verify: Arc::new(AtomicBool::new(true)),
            from: from.to_string(),
        }
    }

    /// The shared "auto-verify new registrations" flag, for `AuthUseCase`.
    pub fn auto_verify_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.auto_verify)
    }

    pub async fn is_configured(&self) -> bool {
        self.transport.read().await.is_some()
    }

    /// Swaps in a transport built from `host`/`port`/credentials. A `None` or
    /// blank host clears it (email disabled).
    ///
    /// The new transport is built *before* the lock is taken, so a failure
    /// leaves the previous one serving. Callers persisting new settings should
    /// only commit them once this returns `Ok`.
    pub async fn reload(
        &self,
        host: Option<&str>,
        port: u16,
        username: Option<&str>,
        password: Option<&str>,
    ) -> Result<(), AppError> {
        let host = host.map(str::trim).filter(|h| !h.is_empty());

        let next = match host {
            Some(host) => Some(Arc::new(LettreEmailService::new(
                host, port, username, password, &self.from,
            )?)),
            None => None,
        };

        let configured = next.is_some();
        *self.transport.write().await = next;
        self.auto_verify.store(!configured, Ordering::Relaxed);

        if configured {
            tracing::info!("SMTP transport (re)loaded — email sending enabled");
        } else {
            tracing::warn!(
                "SMTP host not configured — email sending disabled, new registrations are auto-verified"
            );
        }
        Ok(())
    }
}

#[async_trait]
impl EmailService for ReloadableEmailService {
    async fn send(&self, to: &str, subject: &str, html_body: &str) -> Result<(), AppError> {
        // Clone the Arc out under a short read lock, then send without holding
        // it, so a concurrent reload never waits on an in-flight SMTP round-trip.
        let transport = self.transport.read().await.clone();
        match transport {
            Some(t) => t.send(to, subject, html_body).await,
            None => Err(AppError::internal("smtp_not_configured")),
        }
    }
}
