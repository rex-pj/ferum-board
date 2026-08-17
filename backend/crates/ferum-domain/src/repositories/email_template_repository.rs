use async_trait::async_trait;

use crate::models::email_template::EmailTemplate;
use crate::AppError;

#[async_trait]
pub trait EmailTemplateRepository: Send + Sync {
    /// Every stored row, for the admin editor.
    async fn list(&self) -> Result<Vec<EmailTemplate>, AppError>;

    /// One exact row. `None` means this locale has no row of its own — which is
    /// not the same as "no copy exists", because [`Self::resolve`] would still
    /// find an inherited one.
    async fn find(&self, key: &str, locale: &str) -> Result<Option<EmailTemplate>, AppError>;

    /// The row to actually send, given a recipient's locale fallback chain.
    ///
    /// **Takes the whole chain and answers in one round trip.** `chain` is
    /// `Locale::fallback_chain()` output, most specific first, and the
    /// implementation must return the earliest match *in that order* — not
    /// whatever the planner happened to sort first. Issuing one query per link
    /// instead would put up to three round trips on the path of every email.
    ///
    /// `None` means no locale in the chain has a row, and the caller falls back
    /// to the compiled-in default rather than failing: a verification link the
    /// user can still click beats locking them out of a new account.
    async fn resolve(
        &self,
        key: &str,
        chain: &[String],
    ) -> Result<Option<EmailTemplate>, AppError>;

    /// Creates or replaces one locale's copy.
    async fn upsert(
        &self,
        key: &str,
        locale: &str,
        subject: &str,
        body_html: &str,
    ) -> Result<(), AppError>;

    /// Inserts only when absent, reporting whether it wrote.
    ///
    /// What the startup seeder uses. Never an upsert: re-asserting defaults on
    /// every boot would silently discard an admin's edits, the same rule that
    /// governs default permission grants.
    async fn insert_if_absent(
        &self,
        key: &str,
        locale: &str,
        subject: &str,
        body_html: &str,
    ) -> Result<bool, AppError>;

    /// Drops one locale's copy, which is what "restore default" does — the
    /// compiled-in catalogue is then what resolves, so nothing has to store a
    /// second pristine copy to restore from.
    async fn delete(&self, key: &str, locale: &str) -> Result<(), AppError>;
}
