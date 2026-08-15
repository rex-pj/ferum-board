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
    /// Resend's HTTPS API. Selected by the operator; its credential still comes
    /// from the environment (see [`MailReload::resend`]).
    Resend,
    /// The SMTP transport, configured from `site_config`.
    Smtp,
    /// Nothing usable is configured. Mail cannot be sent, and registrations are
    /// auto-verified as a result.
    Disabled,
}

impl MailProvider {
    /// Stable machine-readable label. Consumed by `/health/ready` and by the
    /// settings template's provider branches, so it is part of both contracts —
    /// changing a string here changes an API response and a template branch.
    pub fn label(&self) -> &'static str {
        match self {
            MailProvider::Resend => "resend",
            MailProvider::Smtp => "smtp",
            MailProvider::Disabled => "disabled",
        }
    }
}

/// What the operator *chose*, from the `mail_provider` site_config key.
///
/// Deliberately a different type from [`MailProvider`], which is what is
/// *effective*. The two differ whenever a choice cannot be honoured — "Resend
/// selected, RESEND_API_KEY absent" is `Resend` here and `Disabled` there — and
/// collapsing them into one enum is what would let the settings page report a
/// provider that sends nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SelectedProvider {
    /// The default, and the fallback for an unrecognised stored value: SMTP is
    /// the provider that can be configured entirely from the admin UI, so it is
    /// the one an operator can always recover with.
    #[default]
    Smtp,
    Resend,
    /// Mail is deliberately off.
    ///
    /// An explicit value, and that is the point. "Off" used to be encoded as a
    /// blank `smtp_host`, which made a global decision a side effect of one
    /// provider's field — so turning mail off meant erasing a relay you would
    /// have to retype, and the switch that did it could not mean anything at all
    /// while Resend was the selected provider.
    Off,
}

impl SelectedProvider {
    /// Parses the stored `mail_provider` value. Anything unrecognised — including
    /// a blank, unseeded row — is SMTP, never an error: this runs on the startup
    /// path, and refusing to boot over a bad enum in a config table would take a
    /// forum offline over a value the admin UI can fix in one click.
    ///
    /// Note that unrecognised falls to `Smtp` rather than `Off`. Both are safe,
    /// but `Smtp` then reports `Disabled` when no host is stored and sends when
    /// one is — whereas `Off` would silently stop a working forum's mail because
    /// a row was mistyped.
    pub fn from_label(raw: &str) -> Self {
        match raw.trim() {
            "resend" => Self::Resend,
            "off" => Self::Off,
            _ => Self::Smtp,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Smtp => "smtp",
            Self::Resend => "resend",
            Self::Off => "off",
        }
    }
}

/// An SMTP endpoint, as stored in `site_config`.
///
/// Defined here rather than reusing the web layer's `SmtpSettings` because
/// infrastructure cannot depend on `ferum-web` — the dependency arrow points the
/// other way.
#[derive(Debug, Clone)]
pub struct SmtpEndpoint {
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
}

/// Everything a reload needs, as one value.
///
/// One struct rather than a setter per field, because `from` is captured by each
/// provider **at construction**: changing it has to rebuild whichever provider is
/// active. A per-field API would make that ordering the caller's problem, and the
/// failure it invites is a half-applied change — the new sender address stored
/// while the live transport still sends as the old one.
pub struct MailReload {
    pub selected: SelectedProvider,
    pub from: String,
    pub smtp: Option<SmtpEndpoint>,
    /// The Resend provider, already built by the caller.
    ///
    /// Built there and not here because its two inputs come from different
    /// places: the API key is env-only and immutable for the process lifetime,
    /// while `from` above is editable at runtime. Only the caller holds both.
    /// `None` means the env key is absent, so Resend cannot be honoured.
    ///
    /// `dyn` rather than a concrete `ResendEmailService` so the tests can pass a
    /// recording stub and observe a send without reaching the network.
    pub resend: Option<Arc<dyn EmailService>>,
}

/// The whole live state, swapped under one lock.
///
/// One lock over one struct, not a lock per field: a reader taking three locks in
/// sequence can observe a new `provider` beside the previous `active`, which is
/// the settings page reporting one provider while sends go through another.
struct MailState {
    /// The provider a send goes through. `None` means mail is off.
    active: Option<Arc<dyn EmailService>>,
    provider: MailProvider,
    from: String,
    describe: String,
}

/// An `EmailService` whose provider and settings swap at runtime, since both
/// live in `site_config` and are editable from `/admin/settings`. Atomic swap,
/// like `TeraEngine`: a send in flight keeps its provider.
///
/// **Exactly one active provider, resolved at reload time, not send time.**
/// Deciding precedence per message is what once made the settings page show
/// SMTP values that were inert.
pub struct ReloadableEmailService {
    state: RwLock<MailState>,
    /// True when no provider is in place, i.e. new registrations must be
    /// auto-verified because no verification mail can be delivered. Shared by
    /// `Arc` with `AuthUseCase`, so configuring mail from the admin UI turns
    /// email verification back on immediately instead of at the next restart.
    ///
    /// Kept outside the lock because `AuthUseCase` reads it on a hot path and
    /// must never block behind a reload.
    auto_verify: Arc<AtomicBool>,
}

impl ReloadableEmailService {
    /// Starts unconfigured; call [`Self::reload`] once site config is available.
    pub fn new(from: &str) -> Self {
        Self {
            state: RwLock::new(MailState {
                active: None,
                provider: MailProvider::Disabled,
                from: from.to_string(),
                describe: "none — email sending disabled".to_string(),
            }),
            auto_verify: Arc::new(AtomicBool::new(true)),
        }
    }

    /// The shared "auto-verify new registrations" flag, for `AuthUseCase`.
    pub fn auto_verify_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.auto_verify)
    }

    /// Which provider a send would currently go through.
    pub async fn provider(&self) -> MailProvider {
        self.state.read().await.provider
    }

    /// The `From` address the active provider sends as.
    ///
    /// Exposed for the admin settings page, and read from the live service rather
    /// than re-read from `Config` for the same reason [`Self::provider`] is: this
    /// is the address mail is actually sent as. It is also the single largest
    /// deliverability factor — SPF and DKIM align against this domain, not against
    /// the relay — so an operator changing relays needs to see it.
    pub async fn from(&self) -> String {
        self.state.read().await.from.clone()
    }

    /// A human-readable, non-secret description of where mail is going, for the
    /// startup log. Includes the SMTP host, port and TLS mode when SMTP is
    /// active, because "email enabled" alone does not tell an operator whether
    /// the relay they configured is the one in use.
    pub async fn describe(&self) -> String {
        self.state.read().await.describe.clone()
    }

    /// Applies a whole new mail configuration.
    ///
    /// The new provider is built **before** the lock is taken, so a failure
    /// leaves the previous one serving and nothing is half-applied. Callers
    /// persisting new settings should only commit them once this returns `Ok`.
    pub async fn reload(&self, next: MailReload) -> Result<(), AppError> {
        // Resolve the choice into something that can actually send. Note that an
        // unhonourable choice is `Disabled`, not an error: "Resend selected but
        // RESEND_API_KEY is unset" is a state the admin UI has to be able to
        // render and fix, and returning `Err` here would instead fail the save
        // that was trying to fix it.
        let (active, provider, describe): (Option<Arc<dyn EmailService>>, _, String) =
            match next.selected {
                SelectedProvider::Off => (
                    None,
                    MailProvider::Disabled,
                    "none — email sending turned off by an administrator".into(),
                ),
                SelectedProvider::Resend => match next.resend {
                    Some(p) => (Some(p), MailProvider::Resend, "Resend (HTTPS API)".into()),
                    None => (
                        None,
                        MailProvider::Disabled,
                        "none — Resend is selected but RESEND_API_KEY is not set".into(),
                    ),
                },
                SelectedProvider::Smtp => {
                    match next.smtp.filter(|e| !e.host.trim().is_empty()) {
                        Some(e) => {
                            // `?` here is safe without touching `auto_verify`: no
                            // state has been written yet, so the flag still
                            // describes what is genuinely still in place. The
                            // previous implementation had to resync on this path
                            // because it wrote in several steps.
                            let t = LettreEmailService::new(
                                e.host.trim(),
                                e.port,
                                e.username.as_deref(),
                                e.password.as_deref(),
                                &next.from,
                            )?;
                            let describe = t.describe().to_string();
                            (Some(Arc::new(t) as Arc<dyn EmailService>), MailProvider::Smtp, describe)
                        }
                        None => (
                            None,
                            MailProvider::Disabled,
                            "none — email sending disabled".into(),
                        ),
                    }
                }
            };

        let configured = active.is_some();

        // One write, so no reader can see a torn combination.
        {
            let mut state = self.state.write().await;
            state.active = active;
            state.provider = provider;
            state.from = next.from;
            state.describe = describe;
        }
        // The invariant, expressed in exactly one place:
        //   auto_verify == no provider is in place
        self.auto_verify.store(!configured, Ordering::Relaxed);

        match provider {
            MailProvider::Resend => {
                tracing::info!("Mail provider: Resend (HTTPS API) — email sending enabled")
            }
            MailProvider::Smtp => {
                tracing::info!("Mail provider: SMTP transport (re)loaded — email sending enabled")
            }
            MailProvider::Disabled => tracing::warn!(
                "Mail is not configured — email sending disabled, new registrations are auto-verified"
            ),
        }
        Ok(())
    }
}

#[async_trait]
impl EmailService for ReloadableEmailService {
    async fn send(&self, to: &str, subject: &str, html_body: &str) -> Result<(), AppError> {
        // Clone the Arc out under a short read lock, then send without holding
        // it, so a concurrent reload never waits on an in-flight round-trip.
        let active = self.state.read().await.active.clone();
        match active {
            Some(p) => p.send(to, subject, html_body).await,
            None => Err(AppError::internal("mail_not_configured")),
        }
    }
}
