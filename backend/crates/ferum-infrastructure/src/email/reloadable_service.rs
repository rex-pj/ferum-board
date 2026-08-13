use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;

use ferum_application::ports::EmailService;
use ferum_application::shared::AppError;

use super::LettreEmailService;

/// Which mail provider is actually in use.
///
/// One value read by four surfaces — the startup log, `/health/ready`, the admin
/// settings page, and the test-send response — so they cannot describe the
/// system differently. Same role as `Credentials::describe()` in the GCS adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MailProvider {
    /// A provider fixed at boot from the environment (currently Resend).
    Fixed,
    /// The reloadable SMTP transport, configured from `site_config`.
    Smtp,
    /// Nothing configured. Mail cannot be sent, and registrations are
    /// auto-verified as a result.
    Disabled,
}

impl MailProvider {
    /// Stable machine-readable label. Consumed by `/health/ready` and by the
    /// settings template's provider banner, so it is part of both contracts —
    /// changing a string here changes an API response and a template branch.
    pub fn label(&self) -> &'static str {
        match self {
            MailProvider::Fixed => "resend",
            MailProvider::Smtp => "smtp",
            MailProvider::Disabled => "disabled",
        }
    }
}

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
    /// A provider chosen once at construction from the environment, which
    /// **wins over SMTP** whenever it is present.
    ///
    /// Its credential is env-only and therefore immutable while the process
    /// runs, so unlike SMTP it needs no reload path at all — which is exactly
    /// why it is a plain field beside `transport` rather than another arm of an
    /// enum threaded through `reload()`. The asymmetry is real: one provider is
    /// runtime-mutable because its settings live in a table an admin can edit,
    /// the other is fixed because its settings live in the environment.
    ///
    /// Typed `dyn` rather than concretely: nothing here calls a provider-specific
    /// method, and `dyn` is what lets the precedence and `auto_verify` tests
    /// substitute a recording stub instead of reaching the network.
    fixed: Option<Arc<dyn EmailService>>,
    transport: RwLock<Option<Arc<LettreEmailService>>>,
    /// True when *no* provider is configured, i.e. new registrations must be
    /// auto-verified because no verification mail can be delivered. Shared by
    /// `Arc` with `AuthUseCase`, so configuring SMTP from the admin UI turns
    /// email verification back on immediately instead of at the next restart.
    ///
    /// Maintained in exactly one place, [`Self::sync_auto_verify`].
    auto_verify: Arc<AtomicBool>,
    from: String,
}

impl ReloadableEmailService {
    /// Starts unconfigured; call [`Self::reload`] once site config is available.
    pub fn new(from: &str) -> Self {
        Self {
            fixed: None,
            transport: RwLock::new(None),
            auto_verify: Arc::new(AtomicBool::new(true)),
            from: from.to_string(),
        }
    }

    /// Installs the boot-time provider that takes precedence over SMTP.
    ///
    /// Builder-style, matching `AuthUseCase::with_auto_verify_flag` and
    /// `JobExecutor::with_translator`, so no existing construction site changes.
    /// Sets `auto_verify` to false immediately: a caller that installs a provider
    /// and never reaches `reload` (SMTP absent) must still not auto-verify.
    pub fn with_fixed_provider(mut self, provider: Arc<dyn EmailService>) -> Self {
        self.fixed = Some(provider);
        self.auto_verify.store(false, Ordering::Relaxed);
        self
    }

    /// The shared "auto-verify new registrations" flag, for `AuthUseCase`.
    pub fn auto_verify_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.auto_verify)
    }

    /// Which provider a send would currently go through.
    pub async fn provider(&self) -> MailProvider {
        if self.fixed.is_some() {
            MailProvider::Fixed
        } else if self.transport.read().await.is_some() {
            MailProvider::Smtp
        } else {
            MailProvider::Disabled
        }
    }

    /// A human-readable, non-secret description of where mail is going, for the
    /// startup log. Includes the SMTP host, port and TLS mode when SMTP is
    /// active, because "email enabled" alone does not tell an operator whether
    /// the relay they configured is the one in use.
    pub async fn describe(&self) -> String {
        if self.fixed.is_some() {
            // The fixed provider logs its own `describe()` when it is built, so
            // there is nothing to add here beyond naming which side won.
            return "fixed provider from the environment".to_string();
        }
        match self.transport.read().await.as_ref() {
            Some(t) => t.describe().to_string(),
            None => "none — email sending disabled".to_string(),
        }
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
            Some(host) => {
                match LettreEmailService::new(host, port, username, password, &self.from) {
                    Ok(t) => Some(Arc::new(t)),
                    // Do not leave `auto_verify` holding a value that predates
                    // this call.
                    //
                    // This arm is currently unreachable in practice, and the
                    // reason is not obvious from the API: `starttls_relay`
                    // returns `Result`, but only stores the domain — rustls does
                    // not parse it as a server name until connect time, so even a
                    // malformed host builds. The one build-time failure is
                    // loading the root certificate store.
                    //
                    // Kept anyway, at three lines, because the flag must be a
                    // function of state rather than of which path last ran. If
                    // this ever does fire, the stale value fails in the worst
                    // direction: the log says "email disabled" while every
                    // registration is silently auto-verified and nobody's address
                    // is checked.
                    Err(e) => {
                        self.sync_auto_verify().await;
                        return Err(e);
                    }
                }
            }
            None => None,
        };

        let configured = next.is_some();
        *self.transport.write().await = next;
        self.sync_auto_verify().await;

        if configured {
            tracing::info!("SMTP transport (re)loaded — email sending enabled");
        } else if self.fixed.is_some() {
            tracing::info!(
                "SMTP is not configured, but a fixed mail provider is active — email sending stays enabled"
            );
        } else {
            tracing::warn!(
                "SMTP host not configured — email sending disabled, new registrations are auto-verified"
            );
        }
        Ok(())
    }

    /// Recomputes `auto_verify` from what is actually in place.
    ///
    /// The invariant, and the only place it is expressed:
    ///
    /// ```text
    /// auto_verify == !(fixed.is_some() || transport.is_some())
    /// ```
    ///
    /// Both terms matter. `reload` is only ever called with SMTP settings, so
    /// deriving the flag from the transport alone would turn auto-verification
    /// back on the moment SMTP was absent or cleared — even with a fixed
    /// provider sending mail perfectly well. That failure is silent in both
    /// directions: mail keeps working, and email addresses stop being verified.
    async fn sync_auto_verify(&self) {
        let configured = self.fixed.is_some() || self.transport.read().await.is_some();
        self.auto_verify.store(!configured, Ordering::Relaxed);
    }
}

#[async_trait]
impl EmailService for ReloadableEmailService {
    async fn send(&self, to: &str, subject: &str, html_body: &str) -> Result<(), AppError> {
        // The boot-time provider wins. It is checked before the lock is taken
        // because when one is installed the SMTP transport is irrelevant to the
        // send, even though `reload` keeps maintaining it so that removing the
        // env var and restarting falls back cleanly.
        if let Some(provider) = &self.fixed {
            return provider.send(to, subject, html_body).await;
        }

        // Clone the Arc out under a short read lock, then send without holding
        // it, so a concurrent reload never waits on an in-flight SMTP round-trip.
        let transport = self.transport.read().await.clone();
        match transport {
            Some(t) => t.send(to, subject, html_body).await,
            None => Err(AppError::internal("smtp_not_configured")),
        }
    }
}
