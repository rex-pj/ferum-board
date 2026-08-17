use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Semaphore;

use crate::network_utils::build_pinned_client;
use ferum_application::constants::UNSUBSCRIBE_TOKEN_TTL_SECS;
use ferum_application::ports::{
    EmailService, ForumJob, JobQueue, StorageService, TokenService, TransArg, Translator,
    UNSUBSCRIBE_PURPOSE,
};
use ferum_application::shared::AppError;
use ferum_domain::Locale;
use ferum_domain::repositories::stored_file_repository::StoredFileRepository;
use ferum_domain::repositories::webhook_repository::WebhookRepository;

/// Escapes a value for interpolation into an email's HTML body.
///
/// Fluent does not escape, so without this an author-controlled string reaches
/// the recipient's inbox as markup. `&#39;` rather than XML's `&apos;`, which
/// predates HTML5 and is not defined in HTML 4.
///
/// **Body only.** A subject line is plain text, so escaping it would show a
/// literal `&amp;` in the inbox list.
fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

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
    /// Mints the unsubscribe token carried by every notification email.
    ///
    /// Minted here rather than at enqueue time on purpose: a token created when
    /// the event fired would start ageing while the job sat in the queue, and the
    /// job payload would then be carrying a credential around. Optional for the
    /// same reason as `translator` — the executor is constructible without one,
    /// and a notification email simply is not sent when it is absent, because a
    /// notification email with no working unsubscribe link is the thing that
    /// earns a spam complaint.
    pub tokens: Option<Arc<dyn TokenService>>,
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
            tokens: None,
        }
    }

    pub fn with_translator(mut self, translator: Arc<dyn Translator>, site_name: String) -> Self {
        self.translator = Some(translator);
        self.site_name = site_name;
        self
    }

    /// Supplies the token service used to build unsubscribe links.
    pub fn with_tokens(mut self, tokens: Arc<dyn TokenService>) -> Self {
        self.tokens = Some(tokens);
        self
    }

    /// Resolves an email string in the recipient's language.
    ///
    /// **Interpolates raw — every `-body` caller must pass values through
    /// [`escape_html`] first.** Fluent performs no escaping of its own, so an
    /// argument reaches the HTML body exactly as given.
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
                    ("site_name", TransArg::Str(escape_html(&self.site_name))),
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
                user_id,
                email,
                kind,
                thread_slug,
                thread_title,
                actor_username,
                locale,
            } => {
                // No token service means no unsubscribe link, and a notification
                // email without one is exactly the message people report rather
                // than mute. Dropping it is the safer failure.
                let Some(tokens) = &self.tokens else {
                    tracing::warn!(
                        %user_id,
                        "notification email skipped: no token service, so no unsubscribe link"
                    );
                    return Ok(());
                };
                let token =
                    tokens.mint_email_token(user_id, UNSUBSCRIBE_PURPOSE, UNSUBSCRIBE_TOKEN_TTL_SECS)?;

                let thread_url = format!("{}/forum/t/{}", self.app_url, thread_slug);
                let unsubscribe_url = format!("{}/unsubscribe/{}", self.app_url, token);
                let settings_url = format!("{}/account", self.app_url);

                let stem = kind.key_stem();

                // Two arg sets because the subject is plain text and the body is
                // HTML. `thread_title` and `actor` are author-controlled —
                // `validate_thread_title` checks length only and a title never
                // passes through ammonia — so unescaped they put arbitrary markup,
                // including an `<a href>`, in someone else's inbox over the forum's
                // own verified domain. The URLs are built here from an app_url, a
                // slugified slug and a minted token, so they carry no user input.
                let subject_args: &[(&str, TransArg)] = &[
                    ("site_name", TransArg::Str(self.site_name.clone())),
                    ("actor", TransArg::Str(actor_username.clone())),
                    ("thread_title", TransArg::Str(thread_title.clone())),
                    ("url", TransArg::Str(thread_url.clone())),
                    ("unsubscribe_url", TransArg::Str(unsubscribe_url.clone())),
                    ("settings_url", TransArg::Str(settings_url.clone())),
                ];
                let body_args: &[(&str, TransArg)] = &[
                    ("site_name", TransArg::Str(escape_html(&self.site_name))),
                    ("actor", TransArg::Str(escape_html(&actor_username))),
                    ("thread_title", TransArg::Str(escape_html(&thread_title))),
                    ("url", TransArg::Str(thread_url)),
                    ("unsubscribe_url", TransArg::Str(unsubscribe_url)),
                    ("settings_url", TransArg::Str(settings_url)),
                ];
                let subject = self.t(&locale, &format!("{stem}-subject"), subject_args);
                let body = self.t(&locale, &format!("{stem}-body"), body_args);
                self.email.send(&email, &subject, &body).await
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
    // `validate_webhook_url` now refuses `http` on save, but rows predating that
    // still deliver. Warned rather than blocked: the HMAC still protects
    // integrity, and failing them here would look like the receiver breaking.
    if url.starts_with("http://") {
        tracing::warn!(
            webhook_id = %webhook_id,
            "delivering over plaintext http — payload is readable in transit. \
             Re-save this webhook with an https URL."
        );
    }

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

/// How many *outbound-network* jobs may run at once.
///
/// Webhooks are the only fan-out job — one per subscriber, and a plugin can
/// register them — so a single reply could otherwise open hundreds of sockets.
///
/// Larger than the local budget because these are almost pure I/O wait.
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

    /// Which budget a job draws from, by where it spends time.
    ///
    /// **Exhaustive on purpose — no catch-all arm.** A new `ForumJob` variant
    /// must fail to compile here rather than defaulting to the local budget,
    /// which would restore the head-of-line blocking this split removes and show
    /// up only as slow signup mail.
    ///
    /// Email is excluded despite opening a socket: it is enqueued one at a time,
    /// never fanned out. The axis is fan-out, not I/O.
    pub fn is_network_bound(job: &ForumJob) -> bool {
        match job {
            ForumJob::SendWebhook { .. } => true,
            ForumJob::SendEmailVerification { .. }
            | ForumJob::SendPasswordResetEmail { .. }
            | ForumJob::SendNotificationEmail { .. }
            | ForumJob::GcStorageKey { .. } => false,
        }
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