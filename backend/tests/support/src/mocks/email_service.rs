use std::sync::Mutex;

use async_trait::async_trait;

use ferum_application::ports::{EmailService, OutgoingEmail};
use ferum_domain::AppError;

/// One captured message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SentEmail {
    pub to: String,
    pub subject: String,
    pub html_body: String,
    /// Captured too, so a test can assert the plain-text alternative is really
    /// plain — sending the HTML twice would satisfy any check that only looked
    /// at whether a text part existed.
    pub text_body: String,
}

/// Records what was sent instead of sending it.
///
/// Hand-written rather than `mockall`, for the reason `InMemorySiteConfig` gives:
/// `EmailService::send` takes a borrowed struct, which is the case mockall's
/// lifetime handling makes awkward.
///
/// `std::sync::Mutex`, not tokio's: nothing is awaited while the lock is held, so a
/// blocking mutex is both correct and simpler to read.
pub struct RecordingEmailService {
    sent: Mutex<Vec<SentEmail>>,
    /// When set, every send fails with this message — the error path matters as
    /// much as the happy one for anything that reports delivery status.
    fail_with: Option<String>,
}

impl RecordingEmailService {
    pub fn new() -> Self {
        Self {
            sent: Mutex::new(Vec::new()),
            fail_with: None,
        }
    }

    /// A provider that always fails, for exercising failure reporting.
    pub fn failing(message: &str) -> Self {
        Self {
            sent: Mutex::new(Vec::new()),
            fail_with: Some(message.to_string()),
        }
    }

    pub fn sent(&self) -> Vec<SentEmail> {
        self.sent.lock().unwrap().clone()
    }

    pub fn sent_count(&self) -> usize {
        self.sent.lock().unwrap().len()
    }

    pub fn sent_to(&self, addr: &str) -> Vec<SentEmail> {
        self.sent
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.to == addr)
            .cloned()
            .collect()
    }
}

impl Default for RecordingEmailService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EmailService for RecordingEmailService {
    async fn send(&self, message: OutgoingEmail<'_>) -> Result<(), AppError> {
        if let Some(failure) = &self.fail_with {
            return Err(AppError::internal(failure.clone()));
        }
        self.sent.lock().unwrap().push(SentEmail {
            to: message.to.to_string(),
            subject: message.subject.to_string(),
            html_body: message.html.to_string(),
            text_body: message.text.to_string(),
        });
        Ok(())
    }
}
