//! Transactional email through Resend's HTTPS API.
//!
//! Not Resend's SMTP bridge: that needs a static credential in `site_config` and
//! gives up the per-message id, the only handle on a delivery after the fact.
//! The API key stays in the environment, so it never needs encrypting.
//!
//! **No cargo feature**, unlike the other adapters — it adds no optional crate,
//! `reqwest` is already unconditional. A gate would buy nothing and introduce
//! the "env var set on a binary that cannot honour it" failure.

use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use std::time::Duration;

use ferum_application::ports::{EmailService, OutgoingEmail};
use ferum_application::shared::AppError;

use crate::network_utils::truncate_for_log;

const RESEND_ENDPOINT: &str = "https://api.resend.com/emails";

/// Matches `build_pinned_client`'s own timeout. A mail send happens on a
/// background job, so it is not blocking a response — but it does hold a job
/// permit, and an unbounded wait would hold one forever.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

pub struct ResendEmailService {
    http: Client,
    api_key: String,
    from: String,
}

impl ResendEmailService {
    pub fn new(api_key: &str, from: &str) -> Result<Self, AppError> {
        let api_key = api_key.trim();
        if api_key.is_empty() {
            // Fails startup rather than degrading. An operator who set the
            // variable meant to use it, and silently falling through to SMTP —
            // or to no mail at all — would be discovered when the first user
            // could not verify an address.
            return Err(AppError::internal("RESEND_API_KEY is set but empty"));
        }

        // Deliberately NOT `network_utils::build_pinned_client`. That guard is
        // the only supported way to reach a *user-, admin- or plugin-supplied*
        // URL, because such a URL can be pointed at a private address between
        // validation and connect. This destination is a compile-time constant:
        // there is nothing an attacker can influence and nothing for the SSRF
        // guard to protect. The same reasoning is written out in
        // `storage/gcs.rs` and `storage/public_read.rs`.
        let http = Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|e| AppError::internal(format!("failed to build Resend HTTP client: {e}")))?;

        Ok(Self {
            http,
            api_key: api_key.to_string(),
            from: from.to_string(),
        })
    }

    /// A short, non-secret label for the startup log — never the key.
    pub fn describe(&self) -> &'static str {
        "Resend HTTPS API (api.resend.com)"
    }
}

/// Builds the request body.
///
/// Factored out, and `pub` so the infra test suite (a separate crate) can assert
/// it without a network: `to` must be an **array** even for a single recipient,
/// which the API rejects otherwise, and that is not visible from any test that
/// stops at the `EmailService` boundary.
pub fn payload(from: &str, message: &OutgoingEmail<'_>) -> serde_json::Value {
    serde_json::json!({
        "from": from,
        "to": [message.to],
        "subject": message.subject,
        "html": message.html,
        // Resend assembles multipart/alternative itself when both are present.
        "text": message.text,
    })
}

/// Turns a non-2xx response into an error.
///
/// Factored out and `pub` for one reason: **the message must never contain the
/// API key**, and that invariant needs a test. It matters more here than in most
/// adapters, because the admin test-send endpoint deliberately surfaces this
/// message to the caller instead of collapsing it to `internal_error`.
///
/// The body is echoed because it is the only thing that says *why* Resend
/// refused — an unverified sending domain and a malformed address are the two
/// common causes and are indistinguishable from the status alone.
pub fn error_from_status(status: StatusCode, body: &str) -> AppError {
    AppError::internal(format!(
        "Resend rejected the send ({status}): {}",
        truncate_for_log(body)
    ))
}

/// Only the field actually used. Resend returns more; ignoring the rest means a
/// new field upstream cannot break deserialization.
#[derive(Deserialize)]
struct SendResponse {
    id: String,
}

#[async_trait]
impl EmailService for ResendEmailService {
    // `skip_all` and then only `subject` back in. `to` is deliberately absent:
    // unlike a webhook URL it is a member's email address, and a log line is not
    // where that belongs.
    #[tracing::instrument(skip_all, fields(subject = %message.subject))]
    async fn send(&self, message: OutgoingEmail<'_>) -> Result<(), AppError> {
        let response = self
            .http
            .post(RESEND_ENDPOINT)
            .bearer_auth(&self.api_key)
            .json(&payload(&self.from, &message))
            .send()
            .await
            .map_err(|e| AppError::internal(format!("Resend request failed: {e}")))?;

        // Status before parsing: an error body has a different shape entirely,
        // so parsing first would report a deserialization failure in place of
        // the actual reason for the rejection.
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(error_from_status(status, &body));
        }

        // No retry. The request carries no idempotency key, so a retry risks a
        // duplicate message and a second charge. (Resend supports an
        // `Idempotency-Key` header; wiring it up would be the prerequisite for
        // retrying, and is out of scope here.) The job queue already logs a
        // failure, and the caller — a verification or password-reset mail — has
        // a user-visible "resend" path.
        let parsed: SendResponse = response
            .json()
            .await
            .map_err(|e| AppError::internal(format!("Resend response unreadable: {e}")))?;

        tracing::debug!(message_id = %parsed.id, "mail accepted by Resend");
        Ok(())
    }
}
