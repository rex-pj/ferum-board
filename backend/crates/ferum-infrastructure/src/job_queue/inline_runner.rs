use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Semaphore;

use crate::network_utils::build_pinned_client;
use ferum_application::ports::{
    EmailService, ForumJob, JobQueue, StorageService, TransArg, Translator,
};
use ferum_application::shared::AppError;
use ferum_domain::Locale;
use ferum_domain::repositories::stored_file_repository::StoredFileRepository;
use ferum_domain::repositories::webhook_repository::WebhookRepository;

pub struct JobExecutor {
    pub email: Arc<dyn EmailService>,
    pub app_url: String,
    pub storage: Arc<dyn StorageService>,
    pub stored_files: Arc<dyn StoredFileRepository>,
    pub webhooks: Arc<dyn WebhookRepository>,
    /// Optional so the executor can still be constructed in tests and during
    /// early startup. When absent, emails fall back to their catalog keys rather
    /// than failing to send.
    pub translator: Option<Arc<dyn Translator>>,
    /// Site name interpolated into email copy.
    pub site_name: String,
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
            translator: None,
            site_name: "Ferum Board".to_string(),
        }
    }

    pub fn with_translator(mut self, translator: Arc<dyn Translator>, site_name: String) -> Self {
        self.translator = Some(translator);
        self.site_name = site_name;
        self
    }

    /// Resolves an email string in the recipient's language.
    ///
    /// Degrades to the raw key when no translator is wired rather than refusing
    /// to send — a verification link the user can still click beats a silent
    /// failure that locks them out of their new account.
    fn t(&self, locale: &Locale, key: &str, args: &[(&str, TransArg)]) -> String {
        match &self.translator {
            Some(t) => t.translate(locale, key, args),
            None => key.to_string(),
        }
    }

    pub async fn run(&self, job: ForumJob) -> Result<(), AppError> {
        match job {
            ForumJob::SendEmailVerification {
                email,
                token,
                locale,
                ..
            } => {
                let url = format!("{}/verify-email/{}", self.app_url, token);
                let args: &[(&str, TransArg)] = &[
                    ("url", TransArg::Str(url)),
                    ("site_name", TransArg::Str(self.site_name.clone())),
                ];
                let subject = self.t(&locale, "email-verify-subject", &[]);
                let body = self.t(&locale, "email-verify-body", args);
                self.email.send(&email, &subject, &body).await
            }
            ForumJob::SendPasswordResetEmail {
                email,
                token,
                locale,
            } => {
                let url = format!("{}/reset-password?token={}", self.app_url, token);
                let args: &[(&str, TransArg)] = &[("url", TransArg::Str(url))];
                let subject = self.t(&locale, "email-reset-subject", &[]);
                let body = self.t(&locale, "email-reset-body", args);
                self.email.send(&email, &subject, &body).await
            }
            ForumJob::SendNotificationEmail {
                user_id: _,
                subject,
                body: _,
                locale: _,
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
        // Deliberately does NOT decrement: every caller decrements first and
        // only enqueues this job once the count already reached 0. Decrementing
        // again used to be how this checked for a concurrent re-reference, but
        // that consumed the very reference it was meant to detect — a user who
        // removed an avatar and re-uploaded the identical file in the same
        // moment had their new one deleted.
        //
        // The row is re-tested inside the DELETE instead, so a revived key is
        // left completely alone — including its blob, which is why the backend
        // delete is now conditional on the row actually going.
        if !self.stored_files.delete_if_unreferenced(key).await? {
            tracing::debug!("gc: key {} was re-referenced or already gone, skipping", key);
            return Ok(());
        }

        if let Err(e) = self.storage.delete(key).await {
            tracing::warn!("gc: storage delete failed for key {}: {:?}", key, e);
        }
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
    let pinned_client = match build_pinned_client(&url).await {
        Ok(c) => c,
        Err(reason) => {
            tracing::warn!("webhook {} blocked at dispatch: {}", url, reason);
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

/// How many *outbound-network* jobs may run at once.
///
/// Webhook delivery is the only job that fans out: `EventBus` enqueues one per
/// subscribed webhook, so a single reply can produce hundreds. Bounding them
/// keeps a burst from opening hundreds of simultaneous sockets — and, since a
/// plugin can register webhooks, from turning one post into an outbound
/// request storm.
///
/// Larger than the local budget below because these jobs are almost entirely
/// I/O wait: each spends up to the 10s HTTP timeout in the network and touches
/// the database only once, briefly, at the very end.
const MAX_CONCURRENT_NETWORK_JOBS: usize = 16;

/// How many jobs that mainly touch *local* resources may run at once.
///
/// Kept separate from the network budget, and that separation is the point.
/// A single shared semaphore looks tidier but couples job classes whose
/// durations differ by three orders of magnitude: 200 webhooks to a dead
/// endpoint would hold every permit for 10s each, so a `SendEmailVerification`
/// enqueued behind them waited minutes — a user sat staring at an inbox waiting
/// for a signup mail because somebody else's webhook host was down. Splitting
/// the budgets means a stalled integration cannot delay a signup.
const MAX_CONCURRENT_LOCAL_JOBS: usize = 8;

pub struct InlineJobRunner {
    executor: Arc<JobExecutor>,
    network_permits: Arc<Semaphore>,
    local_permits: Arc<Semaphore>,
}

impl InlineJobRunner {
    pub fn new(executor: Arc<JobExecutor>) -> Self {
        Self {
            executor,
            network_permits: Arc::new(Semaphore::new(MAX_CONCURRENT_NETWORK_JOBS)),
            local_permits: Arc::new(Semaphore::new(MAX_CONCURRENT_LOCAL_JOBS)),
        }
    }

    /// Which budget a job draws from — by where it actually spends its time,
    /// not by what it is called.
    ///
    /// **A new job that calls out to the network must be added here.** Left out,
    /// it silently draws from the local budget and reintroduces exactly the
    /// head-of-line blocking the split exists to remove; nothing about that
    /// failure is visible except signup mail getting slow. `job_classification`
    /// in the infrastructure tests enumerates the variants to make the omission
    /// fail a build instead.
    ///
    /// Email is deliberately *not* here despite talking to an SMTP server or an
    /// HTTPS mail API: mail is enqueued one job at a time by a user action, never
    /// fanned out, so it cannot produce the burst this bounds — and putting it in
    /// the network budget would let a webhook storm delay it again. The
    /// classification is about **fan-out**, not about whether a socket is opened,
    /// which is why adding an HTTP-based mail provider did not change it.
    pub fn is_network_bound(job: &ForumJob) -> bool {
        matches!(job, ForumJob::SendWebhook { .. })
    }
}

#[async_trait]
impl JobQueue for InlineJobRunner {
    async fn enqueue(&self, job: ForumJob) -> Result<(), AppError> {
        let executor = self.executor.clone();
        let permits = if Self::is_network_bound(&job) {
            self.network_permits.clone()
        } else {
            self.local_permits.clone()
        };
        // The permit is acquired *inside* the task, not before spawning it:
        // `enqueue` is awaited on the request path, so blocking here would make
        // a backed-up job queue slow down the very responses it is meant to stay
        // out of. The task waits instead, and the caller returns immediately.
        tokio::spawn(async move {
            let _permit = match permits.acquire_owned().await {
                Ok(p) => p,
                // Only if the semaphore were closed, which nothing does — but
                // dropping the job silently would be worse than running it.
                Err(_) => return,
            };
            if let Err(e) = executor.run(job).await {
                tracing::error!("background job error: {:?}", e);
            }
        });
        Ok(())
    }
}