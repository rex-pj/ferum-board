use async_trait::async_trait;
use lettre::message::header::ContentType;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

use crate::application::ports::EmailService;
use crate::application::shared::AppError;

pub struct LettreEmailService {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: String,
}

impl LettreEmailService {
    pub fn new(
        host: &str,
        port: u16,
        username: Option<&str>,
        password: Option<&str>,
        from: &str,
    ) -> Result<Self, AppError> {
        let mut builder = AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host)
            .port(port);

        if let (Some(user), Some(pass)) = (username, password) {
            builder = builder.credentials(Credentials::new(user.to_string(), pass.to_string()));
        }

        let transport = builder.build();

        Ok(Self {
            transport,
            from: from.to_string(),
        })
    }
}

#[async_trait]
impl EmailService for LettreEmailService {
    async fn send(&self, to: &str, subject: &str, html_body: &str) -> Result<(), AppError> {
        let email = Message::builder()
            .from(self.from.parse().map_err(|_| AppError::internal("invalid from address"))?)
            .to(to.parse().map_err(|_| AppError::internal("invalid to address"))?)
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
