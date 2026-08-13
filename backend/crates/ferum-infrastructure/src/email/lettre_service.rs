use async_trait::async_trait;
use lettre::message::header::ContentType;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use std::net::IpAddr;

use ferum_application::ports::EmailService;
use ferum_application::shared::AppError;
use ferum_domain::net::normalize_host;

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

/// Decides how to secure the connection to `host`, inferred rather than
/// configured.
///
/// **There is no `SMTP_TLS` setting and deliberately so:** every value an
/// operator could set is already implied by the address they typed. A loopback
/// host is a development mail catcher with no certificate to present; anything
/// else is a relay reached over a network, where handing over SMTP AUTH
/// credentials in the clear is not a mode worth offering as a choice.
///
/// This function exists as a named, pure function because `AsyncSmtpTransport`
/// exposes no getter for the TLS mode it was built with. Without it the decision
/// would be unobservable and therefore untestable.
///
/// **Loopback only — deliberately narrower than `is_private_ip`.** Using the
/// domain's private-range predicate here would also exempt a LAN relay at
/// `10.0.0.5`, and "the packet stays inside our network" is exactly the
/// assumption that makes a flat internal network a credential-harvesting
/// opportunity. A relay on another host is a relay, wherever it lives.
///
/// `pub` so the infra test suite — a separate crate — can assert the decision
/// directly; that is the whole reason it is not inlined at the call site.
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

#[async_trait]
impl EmailService for LettreEmailService {
    async fn send(&self, to: &str, subject: &str, html_body: &str) -> Result<(), AppError> {
        let email = Message::builder()
            .from(
                self.from
                    .parse()
                    .map_err(|_| AppError::internal("invalid from address"))?,
            )
            .to(to
                .parse()
                .map_err(|_| AppError::internal("invalid to address"))?)
            .subject(subject)
            .header(ContentType::TEXT_HTML)
            .body(html_body.to_string())
            .map_err(|e| AppError::internal(format!("email build error: {}", e)))?;

        self.transport
            .send(email)
            .await
            .map_err(|e| AppError::internal(format!("email send error: {}", e)))?;

        Ok(())
    }
}
