use std::sync::Arc;

use async_trait::async_trait;

use ferum_application::ports::{EmailService, ForumJob, JobQueue, StorageService};
use ferum_application::shared::AppError;
use ferum_domain::repositories::stored_file_repository::StoredFileRepository;
use ferum_domain::repositories::webhook_repository::WebhookRepository;

pub struct JobExecutor {
    pub email: Arc<dyn EmailService>,
    pub app_url: String,
    pub storage: Arc<dyn StorageService>,
    pub stored_files: Arc<dyn StoredFileRepository>,
    pub webhooks: Arc<dyn WebhookRepository>,
}

impl JobExecutor {
    pub fn new(
        email: Arc<dyn EmailService>,
        app_url: String,
        storage: Arc<dyn StorageService>,
        stored_files: Arc<dyn StoredFileRepository>,
        webhooks: Arc<dyn WebhookRepository>,
    ) -> Self {
        Self {
            email,
            app_url,
            storage,
            stored_files,
            webhooks,
        }
    }

    pub async fn run(&self, job: ForumJob) -> Result<(), AppError> {
        match job {
            ForumJob::SendEmailVerification { email, token, .. } => {
                let url = format!("{}/verify-email/{}", self.app_url, token);
                let body = format!(
                    "<p>Welcome to Ferum Board! Click the link below to verify your email:</p>\
                     <p><a href=\"{url}\">{url}</a></p>"
                );
                self.email.send(&email, "Verify your email", &body).await
            }
            ForumJob::SendPasswordResetEmail { email, token } => {
                let url = format!("{}/reset-password?token={}", self.app_url, token);
                let body = format!(
                    "<p>You requested a password reset. Click below to reset it (valid 1 hour):</p>\
                     <p><a href=\"{url}\">{url}</a></p>\
                     <p>If you did not request this, ignore this email.</p>"
                );
                self.email.send(&email, "Reset your password", &body).await
            }
            ForumJob::SendNotificationEmail {
                user_id: _,
                subject,
                body: _,
            } => {
                tracing::debug!(
                    "notification email job skipped in inline runner: {}",
                    subject
                );
                Ok(())
            }
            ForumJob::GcStorageKey { key } => self.run_gc_storage_key(&key).await,
            ForumJob::SendWebhook {
                webhook_id,
                url,
                secret,
                event_type,
                payload,
            } => {
                dispatch_webhook(webhook_id, url, secret, event_type, payload, &self.webhooks).await
            }
        }
    }

    pub async fn run_gc_storage_key(&self, key: &str) -> Result<(), AppError> {
        // Decrement once more — might have raced, check count
        let remaining = self.stored_files.decrement_ref(key).await?;
        if remaining > 0 {
            // Another reference was added concurrently; skip deletion
            return Ok(());
        }

        // Delete from underlying storage backend first, then DB row
        if let Err(e) = self.storage.delete(key).await {
            tracing::warn!("gc: storage delete failed for key {}: {:?}", key, e);
        }
        self.stored_files.delete_by_key(key).await?;
        tracing::debug!("gc: deleted orphaned file {}", key);
        Ok(())
    }
}

#[tracing::instrument(skip(secret, payload, webhooks), fields(%webhook_id, %event_type, %url))]
async fn dispatch_webhook(
    webhook_id: uuid::Uuid,
    url: String,
    secret: Option<String>,
    event_type: String,
    payload: serde_json::Value,
    webhooks: &Arc<dyn WebhookRepository>,
) -> Result<(), AppError> {
    // Resolve + validate the IP at dispatch time (an attacker can flip the DNS record
    // to a private IP between webhook creation and dispatch), then pin the actual
    // request to the exact address(es) just validated. Letting reqwest re-resolve the
    // hostname itself would reopen the DNS-rebinding window this check exists to close.
    let host = match url::Url::parse(&url).ok().and_then(|u| u.host_str().map(str::to_string)) {
        Some(h) => h,
        None => {
            tracing::warn!("webhook {} blocked at dispatch: unparseable URL", url);
            webhooks.record_failure(webhook_id).await.ok();
            return Ok(());
        }
    };
    let addrs = match crate::network_utils::resolve_and_validate(&url).await {
        Ok(addrs) => addrs,
        Err(reason) => {
            tracing::warn!("webhook {} blocked at dispatch: {}", url, reason);
            webhooks.record_failure(webhook_id).await.ok();
            return Ok(());
        }
    };
    let pinned_client = match reqwest::Client::builder()
        .resolve_to_addrs(&host, &addrs)
        .timeout(std::time::Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("webhook {} failed to build pinned client: {}", url, e);
            webhooks.record_failure(webhook_id).await.ok();
            return Ok(());
        }
    };

    let body = serde_json::to_string(&payload).map_err(|e| AppError::internal(e.to_string()))?;

    let mut req = pinned_client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("X-Ferum-Event", &event_type)
        .body(body.clone());

    if let Some(ref s) = secret {
        let sig = hmac_sha256(s, &body);
        req = req.header("X-Ferum-Signature", format!("sha256={}", sig));
    }

    match req.send().await {
        Ok(resp) if resp.status().is_success() => {
            if let Err(e) = webhooks.record_success(webhook_id).await {
                tracing::warn!("webhook record_success failed: {:?}", e);
            }
            Ok(())
        }
        Ok(resp) => {
            tracing::warn!("webhook {} returned {}", url, resp.status());
            if let Err(e) = webhooks.record_failure(webhook_id).await {
                tracing::warn!("webhook record_failure failed: {:?}", e);
            }
            Ok(())
        }
        Err(e) => {
            tracing::warn!("webhook {} failed: {}", url, e);
            if let Err(e2) = webhooks.record_failure(webhook_id).await {
                tracing::warn!("webhook record_failure failed: {:?}", e2);
            }
            Ok(())
        }
    }
}


pub fn hmac_sha256(secret: &str, payload: &str) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(payload.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

// ─── InlineJobRunner ──────────────────────────────────────────────────────────

pub struct InlineJobRunner {
    executor: Arc<JobExecutor>,
}

impl InlineJobRunner {
    pub fn new(executor: Arc<JobExecutor>) -> Self {
        Self { executor }
    }
}

#[async_trait]
impl JobQueue for InlineJobRunner {
    async fn enqueue(&self, job: ForumJob) -> Result<(), AppError> {
        let executor = self.executor.clone();
        tokio::spawn(async move {
            if let Err(e) = executor.run(job).await {
                tracing::error!("background job error: {:?}", e);
            }
        });
        Ok(())
    }
}