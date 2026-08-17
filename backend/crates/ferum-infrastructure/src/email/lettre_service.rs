use async_trait::async_trait;
use lettre::message::{Mailbox, MultiPart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use std::net::IpAddr;

use ferum_application::ports::{EmailService, OutgoingEmail};
use ferum_application::shared::AppError;
use ferum_domain::net::normalize_host;

/// Parses a sender address, accepting either a bare address or the RFC 5322
/// `Display Name <addr>` form.
///
/// The single parse of `FROM_EMAIL` in this codebase. `send` uses it, and so does
/// [`validate_from_address`] at startup — deliberately the same call, because a
/// boot check that disagreed with the parser used at send time would be worse than
/// none: it would pass a value that then failed on every message.
fn parse_from(from: &str) -> Result<Mailbox, AppError> {
    from.parse::<Mailbox>().map_err(|e| {
        AppError::internal(format!(
            "FROM_EMAIL is not a valid sender address ({e}). Expected `user@example.com` \
             or `Display Name <user@example.com>`."
        ))
    })
}

/// Rejects a malformed `FROM_EMAIL` at startup.
///
/// A boot check, not a per-send one: parsed only in `send`, a typo gives a
/// process that starts cleanly, reports healthy, and fails every message with a
/// generic error — found when a user cannot register.
///
/// Runs even with no provider configured, and applies lettre's parser to the
/// Resend path too, so `FROM_EMAIL` has one contract however it is delivered.
pub fn validate_from_address(from: &str) -> Result<(), AppError> {
    parse_from(from).map(|_| ())
}

/// The SMTPS port. TLS begins before the first SMTP command here, so STARTTLS —
/// which negotiates *inside* an already-open plaintext session — can never
/// complete on it.
const SMTPS_PORT: u16 = 465;

/// How the connection to an SMTP host is secured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmtpSecurity {
    /// No TLS. Only ever chosen for a loopback address.
    Plaintext,
    /// Connect in the clear, then `STARTTLS` before authenticating. Required,
    /// never opportunistic — a server that does not offer it is refused.
    StartTls,
    /// TLS from the first byte (SMTPS, port 465).
    ImplicitTls,
}

/// Decides how to secure the connection to `host`, inferred not configured.
///
/// No `SMTP_TLS` setting: the address already implies it. **Loopback only,
/// deliberately narrower than `is_private_ip`** — exempting a LAN relay at
/// `10.0.0.5` is what turns a flat internal network into credential harvesting.
///
/// A named function because `AsyncSmtpTransport` exposes no getter for its TLS
/// mode, so the decision would otherwise be untestable.
pub fn security_for(host: &str, port: u16) -> SmtpSecurity {
    if is_loopback_host(host) {
        return SmtpSecurity::Plaintext;
    }
    if port == SMTPS_PORT {
        return SmtpSecurity::ImplicitTls;
    }
    SmtpSecurity::StartTls
}

/// True when `host` names this machine: `localhost`, anything in `127.0.0.0/8`,
/// or IPv6 `::1` with or without the brackets a URL would carry.
fn is_loopback_host(host: &str) -> bool {
    let host = normalize_host(host.trim());
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    match host.parse::<IpAddr>() {
        Ok(ip) => ip.is_loopback(),
        Err(_) => false,
    }
}

pub struct LettreEmailService {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: String,
    description: String,
}

impl LettreEmailService {
    pub fn new(
        host: &str,
        port: u16,
        username: Option<&str>,
        password: Option<&str>,
        from: &str,
    ) -> Result<Self, AppError> {
        let security = security_for(host, port);

        // `relay()` and `starttls_relay()` each set their own default port (465
        // and 587), so `.port(port)` has to come after the choice of builder or
        // the operator's port is silently discarded.
        let mut builder = match security {
            SmtpSecurity::Plaintext => AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host),
            SmtpSecurity::StartTls => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host)
                .map_err(|e| {
                    AppError::internal(format!("SMTP STARTTLS setup failed for {host}: {e}"))
                })?,
            SmtpSecurity::ImplicitTls => AsyncSmtpTransport::<Tokio1Executor>::relay(host)
                .map_err(|e| {
                    AppError::internal(format!("SMTP TLS setup failed for {host}: {e}"))
                })?,
        }
        .port(port);

        if let (Some(user), Some(pass)) = (username, password) {
            builder = builder.credentials(Credentials::new(user.to_string(), pass.to_string()));
        }

        let transport = builder.build();

        Ok(Self {
            transport,
            from: from.to_string(),
            description: format!(
                "SMTP {host}:{port} ({})",
                match security {
                    SmtpSecurity::Plaintext => "no TLS — loopback",
                    SmtpSecurity::StartTls => "STARTTLS",
                    SmtpSecurity::ImplicitTls => "implicit TLS",
                }
            ),
        })
    }

    /// A short, non-secret label for the startup log. Never includes credentials.
    pub fn describe(&self) -> &str {
        &self.description
    }
}

/// Assembles the MIME message.
///
/// Factored out of `send` and `pub` for the reason `resend_service::payload`
/// gives: the message *shape* is the part that can be wrong, and `send` cannot
/// be exercised without an SMTP server. Nothing else about a mail is checkable
/// from outside this adapter.
///
/// # Errors
/// A malformed sender (which startup already validated) or recipient address.
pub fn build_message(from: &str, message: &OutgoingEmail<'_>) -> Result<Message, AppError> {
    Message::builder()
        // The same parse `validate_from_address` ran at startup, so reaching a
        // failure here means the value changed under a running process rather
        // than an operator typo that slipped through.
        .from(parse_from(from)?)
        .to(message
            .to
            .parse()
            .map_err(|_| AppError::internal("invalid to address"))?)
        .subject(message.subject)
        // `alternative_plain_html` emits the plain part first, which is the
        // order RFC 2046 §5.1.4 requires: a client picks the *last* part it can
        // render, so reversing them would serve plain text to everyone.
        .multipart(MultiPart::alternative_plain_html(
            message.text.to_string(),
            message.html.to_string(),
        ))
        .map_err(|e| AppError::internal(format!("email build error: {}", e)))
}

#[async_trait]
impl EmailService for LettreEmailService {
    async fn send(&self, message: OutgoingEmail<'_>) -> Result<(), AppError> {
        let email = build_message(&self.from, &message)?;

        self.transport
            .send(email)
            .await
            .map_err(|e| AppError::internal(format!("email send error: {}", e)))?;

        Ok(())
    }
}
