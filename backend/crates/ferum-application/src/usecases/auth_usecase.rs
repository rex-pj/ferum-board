use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use uuid::Uuid;

use crate::constants::{
    DEFAULT_ACCOUNT_LOCKOUT_ATTEMPTS, DEFAULT_ACCOUNT_LOCKOUT_DURATION_MINUTES,
    PASSWORD_RESET_TOKEN_TTL_SECS,
};
use crate::ports::{
    CacheService, ForumJob, HookContext, HookDecision, JobQueue,
    NullPluginRuntime, PasswordHasher, PluginHookRuntime, TokenService,
};
use super::{build_access_token_claims, refresh_token_key};
use crate::shared::{AppError, OptionExt};
use ferum_domain::models::user::{TrustLevel, User};
use ferum_domain::repositories::role_repository::RoleRepository;
use ferum_domain::repositories::site_config_repository::{
    get_config_i32, get_config_u64, SiteConfigRepository,
};
use ferum_domain::repositories::user_repository::{NewUser, UserRepository};
use ferum_domain::repositories::user_role_repository::UserRoleRepository;

pub struct AuthUseCase {
    pub users: Arc<dyn UserRepository>,
    pub roles: Arc<dyn RoleRepository>,
    pub user_roles: Arc<dyn UserRoleRepository>,
    pub hasher: Arc<dyn PasswordHasher>,
    pub tokens: Arc<dyn TokenService>,
    pub cache: Arc<dyn CacheService>,
    pub jobs: Arc<dyn JobQueue>,
    pub plugin_runtime: Arc<dyn PluginHookRuntime>,
    pub site_config: Option<Arc<dyn SiteConfigRepository>>,
    /// When true, newly registered users are immediately verified (no email required),
    /// because no SMTP transport is configured to deliver a verification mail.
    ///
    /// Shared state rather than a captured `bool`: SMTP is editable at runtime from
    /// `/admin/settings`, and a stale value here would let registrations skip email
    /// verification long after mail started working.
    pub auto_verify_email: Arc<AtomicBool>,
}

impl AuthUseCase {
    pub fn new(
        users: Arc<dyn UserRepository>,
        roles: Arc<dyn RoleRepository>,
        user_roles: Arc<dyn UserRoleRepository>,
        hasher: Arc<dyn PasswordHasher>,
        tokens: Arc<dyn TokenService>,
        cache: Arc<dyn CacheService>,
        jobs: Arc<dyn JobQueue>,
    ) -> Self {
        Self {
            users,
            roles,
            user_roles,
            hasher,
            tokens,
            cache,
            jobs,
            plugin_runtime: Arc::new(NullPluginRuntime),
            site_config: None,
            auto_verify_email: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Fixed value, for callers with no reloadable transport (tests, tooling).
    pub fn with_auto_verify_email(self, enabled: bool) -> Self {
        self.auto_verify_email.store(enabled, Ordering::Relaxed);
        self
    }

    /// Shares the mail transport's own flag, so the decision tracks SMTP being
    /// configured or cleared at runtime. Preferred over [`Self::with_auto_verify_email`].
    pub fn with_auto_verify_flag(mut self, flag: Arc<AtomicBool>) -> Self {
        self.auto_verify_email = flag;
        self
    }

    pub fn with_plugin_runtime(mut self, runtime: Arc<dyn PluginHookRuntime>) -> Self {
        self.plugin_runtime = runtime;
        self
    }

    pub fn with_site_config(mut self, site_config: Arc<dyn SiteConfigRepository>) -> Self {
        self.site_config = Some(site_config);
        self
    }

    // ─── Register ─────────────────────────────────────────────────────────────

    #[tracing::instrument(skip_all)]
    pub async fn register(&self, cmd: RegisterCmd) -> Result<User, AppError> {
        if let Some(site_config) = &self.site_config {
            let registration_open = site_config
                .get("registration_open")
                .await?
                .map(|v| v == "true")
                .unwrap_or(true);
            if !registration_open {
                return Err(AppError::forbidden("registration_closed"));
            }
        }

        if !crate::validators::validate_username(&cmd.username) {
            return Err(AppError::invalid("invalid_username_format"));
        }
        if !crate::validators::validate_password(&cmd.password) {
            return Err(AppError::invalid("password_requirements"));
        }

        let email = cmd.email.to_lowercase();

        // Both checks return the same error code so an attacker cannot enumerate
        // which credential (email vs username) is already registered.
        if self.users.find_by_email(&email).await?.is_some()
            || self.users.find_by_username(&cmd.username).await?.is_some()
        {
            return Err(AppError::Conflict("registration_conflict".to_string()));
        }

        // Plugin before-hook — allows Tier 2 plugins (e.g. StopForumSpam) to block registration
        let hook_ctx = HookContext {
            hook_name: "before_user_register".to_string(),
            actor_id: None,
            actor_trust_level: "new".to_string(),
            payload: serde_json::json!({
                "username": cmd.username,
                "email": email,
            }),
        };
        match self
            .plugin_runtime
            .dispatch_before_hook("before_user_register", &hook_ctx)
            .await?
        {
            HookDecision::Deny { reason, error_code } => {
                return Err(AppError::PluginBlocked { reason, error_code });
            }
            HookDecision::Allow => {}
        }

        let hash = self.hasher.hash(&cmd.password).await?;
        let user = self
            .users
            .create(NewUser {
                username: cmd.username,
                email: email.clone(),
                password_hash: Some(hash),
            })
            .await?;

        // Assign all is_default roles so the user has baseline permissions immediately.
        let default_roles = self.roles.list_default().await?;
        for role in default_roles {
            let _ = self
                .user_roles
                .assign(user.id, role.id, None, user.id, None)
                .await;
        }

        if self.auto_verify_email.load(Ordering::Relaxed) {
            self.users.set_email_verified(user.id).await?;
            self.users.set_trust_level(user.id, TrustLevel::Basic).await?;
        } else {
            let token = self.tokens.mint_email_token(user.id, "email_verification")?;
            self.jobs
                .enqueue(ForumJob::SendEmailVerification {
                    user_id: user.id,
                    email,
                    token,
                    locale: cmd.locale.clone(),
                })
                .await?;
        }

        Ok(user)
    }

    /// The language to write an email to `user_id` in.
    ///
    /// Always the *recipient's* stored preference, never the actor's: a member
    /// reading the site in Vietnamese who triggers a notification to an English
    /// member must not send them a Vietnamese email.
    ///
    /// Falls back to the site default when the user has never chosen, or when the
    /// lookup fails — an email in the wrong language still beats no email.
    async fn recipient_locale(&self, user_id: Uuid) -> ferum_domain::Locale {
        match self.users.get_preferences(user_id).await {
            Ok(prefs) => prefs.locale.unwrap_or_default(),
            Err(e) => {
                tracing::warn!(error = %e, %user_id, "could not read recipient locale, using default");
                ferum_domain::Locale::default_locale()
            }
        }
    }

    // ─── Verify email ─────────────────────────────────────────────────────────

    #[tracing::instrument(skip_all)]
    pub async fn verify_email(&self, token: &str) -> Result<(), AppError> {
        let user_id = self
            .tokens
            .verify_email_token(token, "email_verification")
            .map_err(|_| AppError::forbidden("invalid_or_expired_token"))?;

        let user = self.users.find_by_id(user_id).await?.or_not_found()?;

        if !user.is_email_verified {
            self.users.set_email_verified(user_id).await?;
            self.users.set_trust_level(user_id, TrustLevel::Basic).await?;
        }

        Ok(())
    }

    /// Re-sends the verification email. Always returns `Ok(())` regardless of
    /// whether the address exists or is already verified — same enumeration-safe
    /// shape as `forgot_password` — so a caller can't probe which emails are
    /// registered. Rate limiting against spamming a real inbox happens at the
    /// HTTP layer (auth rate limit), not here.
    #[tracing::instrument(skip_all)]
    pub async fn resend_verification_email(&self, email: &str) -> Result<(), AppError> {
        if let Some(user) = self.users.find_by_email(&email.to_lowercase()).await? {
            if !user.is_email_verified {
                let token = self.tokens.mint_email_token(user.id, "email_verification")?;
                let locale = self.recipient_locale(user.id).await;
                self.jobs
                    .enqueue(ForumJob::SendEmailVerification {
                        user_id: user.id,
                        email: user.email,
                        token,
                        locale,
                    })
                    .await?;
            }
        }
        Ok(())
    }

    // ─── Login ────────────────────────────────────────────────────────────────

    #[tracing::instrument(skip_all)]
    pub async fn login(&self, cmd: LoginCmd) -> Result<LoginResult, AppError> {
        let user_opt = self.users.find_by_email(&cmd.email.to_lowercase()).await?;

        if user_opt.is_none() {
            let _ = self
                .hasher
                .verify(&cmd.password, "$2b$12$invalidhashpaddinginvalidhashpa")
                .await;
            tracing::warn!(email = %cmd.email.to_lowercase(), "login failed: unknown email");
            return Err(AppError::Unauthorized);
        }
        let mut user = user_opt.unwrap();

        if user.is_account_locked() {
            let _ = self
                .hasher
                .verify(&cmd.password, "$2b$12$invalidhashpaddinginvalidhashpa")
                .await;
            tracing::warn!(user_id = %user.id, "login blocked: account locked");
            return Err(AppError::forbidden("account_locked"));
        }

        let hash = user.password_hash.as_deref().unwrap_or("$2b$12$invalidhashpaddinginvalidhashpa");
        let valid = self.hasher.verify(&cmd.password, hash).await?;

        if user.password_hash.is_none() || !valid {
            let count = self.users.increment_failed_login(user.id).await?;
            tracing::warn!(user_id = %user.id, failed_attempts = count, "login failed: invalid password");
            let lockout_attempts = match &self.site_config {
                Some(sc) => get_config_i32(sc.as_ref(), "account_lockout_attempts", DEFAULT_ACCOUNT_LOCKOUT_ATTEMPTS).await,
                None => DEFAULT_ACCOUNT_LOCKOUT_ATTEMPTS,
            };
            if count >= lockout_attempts {
                let lockout_minutes = match &self.site_config {
                    Some(sc) => get_config_u64(sc.as_ref(), "account_lockout_duration_minutes", DEFAULT_ACCOUNT_LOCKOUT_DURATION_MINUTES).await,
                    None => DEFAULT_ACCOUNT_LOCKOUT_DURATION_MINUTES,
                };
                let until = Utc::now()
                    + chrono::Duration::minutes(lockout_minutes as i64);
                self.users.lock_until(user.id, until).await?;
                tracing::warn!(user_id = %user.id, failed_attempts = count, locked_until = %until, "account locked due to repeated failed logins");
                return Err(AppError::forbidden("account_locked"));
            }
            return Err(AppError::Unauthorized);
        }

        if !user.is_email_verified {
            return Err(AppError::forbidden("email_not_verified"));
        }

        if user.is_currently_banned() {
            return Err(AppError::forbidden("account_suspended"));
        }

        // Reset only after all checks pass — prevents banned/unverified users
        // from resetting their lockout counter on each correct-password attempt.
        self.users.reset_failed_login(user.id).await?;

        // ── Activity tracking (fire-and-forget, never blocks login) ──────────
        // update_last_seen atomically increments days_visited when the UTC date
        // has changed since the user's last visit.
        {
            let users = self.users.clone();
            let uid = user.id;
            tokio::spawn(async move {
                let _ = users.update_last_seen(uid).await;
            });
        }

        // ── Lazy trust level promotion ────────────────────────────────────────
        if let Some(new_level) = evaluate_trust_promotion(&user) {
            self.users.set_trust_level(user.id, new_level).await?;
            user.trust_level = new_level;
        }

        let claims = build_access_token_claims(&user, self.tokens.access_token_ttl_secs());
        let access_token = self.tokens.mint_access_token(&claims)?;
        let refresh_token = self.tokens.mint_refresh_token(user.id)?;

        self.cache
            .set(
                &refresh_token_key(user.id, &refresh_token),
                "1",
                Duration::from_secs(self.tokens.refresh_token_ttl_secs()),
            )
            .await?;

        Ok(LoginResult { user, access_token, refresh_token })
    }

    // ─── Refresh ──────────────────────────────────────────────────────────────

    #[tracing::instrument(skip_all)]
    pub async fn refresh_access_token(
        &self,
        refresh_token: &str,
    ) -> Result<RefreshResult, AppError> {
        let user_id = self.tokens.verify_refresh_token(refresh_token)?;

        let key = refresh_token_key(user_id, refresh_token);
        if !self.cache.exists(&key).await {
            return Err(AppError::Unauthorized);
        }

        let user = self.users.find_by_id(user_id).await?.ok_or(AppError::Unauthorized)?;

        if user.is_currently_banned() {
            return Err(AppError::forbidden("account_suspended"));
        }

        // Fire-and-forget: update last_seen_at and days_visited on token refresh (≤1/hour).
        // This catches active users who never explicitly log out and back in.
        {
            let users = self.users.clone();
            tokio::spawn(async move {
                let _ = users.update_last_seen(user_id).await;
            });
        }

        let claims = build_access_token_claims(&user, self.tokens.access_token_ttl_secs());
        let access_token = self.tokens.mint_access_token(&claims)?;

        Ok(RefreshResult { access_token })
    }

    // ─── Logout ───────────────────────────────────────────────────────────────

    #[tracing::instrument(skip_all, fields(user_id = %user_id))]
    pub async fn logout(&self, user_id: Uuid, refresh_token: &str) -> Result<(), AppError> {
        self.cache.del(&refresh_token_key(user_id, refresh_token)).await?;
        // Dropping the refresh token alone left the *access* token usable until
        // its own expiry — logging out did not actually end the session for
        // anyone holding a copy of the cookie.
        crate::usecases::invalidate_sessions(self.cache.as_ref(), user_id).await;
        Ok(())
    }

    /// Logout when the refresh token is not available (expired, or a client that
    /// only ever held the access token). Revoking the access token still has to
    /// happen — otherwise "log out" is purely cosmetic for that caller.
    #[tracing::instrument(skip(self), fields(user_id = %user_id))]
    pub async fn logout_all(&self, user_id: Uuid) -> Result<(), AppError> {
        self.cache
            .del_prefix(&crate::usecases::refresh_token_prefix(user_id))
            .await
            .ok();
        crate::usecases::invalidate_sessions(self.cache.as_ref(), user_id).await;
        Ok(())
    }

    // ─── Forgot password ──────────────────────────────────────────────────────

    #[tracing::instrument(skip_all)]
    pub async fn forgot_password(&self, email: &str) -> Result<(), AppError> {
        if let Some(user) = self.users.find_by_email(&email.to_lowercase()).await? {
            let token = self.tokens.mint_email_token(user.id, "password_reset")?;
            let locale = self.recipient_locale(user.id).await;
            self.jobs
                .enqueue(ForumJob::SendPasswordResetEmail {
                    email: user.email,
                    token,
                    locale,
                })
                .await?;
        }
        Ok(())
    }

    // ─── Reset password ───────────────────────────────────────────────────────

    #[tracing::instrument(skip_all)]
    pub async fn reset_password(&self, cmd: ResetPasswordCmd) -> Result<(), AppError> {
        if !crate::validators::validate_password(&cmd.new_password) {
            return Err(AppError::invalid("password_requirements"));
        }

        let user_id = self
            .tokens
            .verify_email_token(&cmd.token, "password_reset")
            .map_err(|_| AppError::forbidden("invalid_or_expired_token"))?;

        let used_key = format!("used_token:{}", cmd.token);
        let claimed = self
            .cache
            .set_nx(&used_key, "1", Duration::from_secs(PASSWORD_RESET_TOKEN_TTL_SECS + 60))
            .await?;
        if !claimed {
            return Err(AppError::forbidden("token_already_used"));
        }

        let hash = self.hasher.hash(&cmd.new_password).await?;
        self.users.set_password_hash(user_id, hash).await?;

        // A password reset is usually a response to compromise, so it has to end
        // every session, not just the access tokens: a surviving refresh token
        // keeps minting fresh ones straight past the session epoch.
        crate::usecases::revoke_all_sessions(self.cache.as_ref(), user_id).await;

        Ok(())
    }
}

// ─── Lazy trust level promotion ───────────────────────────────────────────────
// Runs synchronously during login. No background job needed.

fn evaluate_trust_promotion(user: &User) -> Option<TrustLevel> {
    match user.trust_level {
        TrustLevel::Basic => {
            // Promote to Member: ≥30 posts AND ≥15 days visited
            if user.post_count >= 30 && user.days_visited >= 15 {
                Some(TrustLevel::Member)
            } else {
                None
            }
        }
        TrustLevel::Member => {
            // Promote to Regular: ≥500 posts AND ≥100 days AND trust_score ≥80
            if user.post_count >= 500 && user.days_visited >= 100 && user.trust_score >= 80 {
                Some(TrustLevel::Regular)
            } else {
                None
            }
        }
        // Leader is manually assigned; New/Regular/Leader don't auto-promote here
        _ => None,
    }
}

// ─── Commands ─────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct RegisterCmd {
    pub username: String,
    pub email: String,
    pub password: String,
    /// Language the visitor was reading the site in when they signed up.
    ///
    /// A brand-new account has no stored preference yet, so the request locale is
    /// the only evidence of what language this person reads — and the
    /// verification email is the very first thing they receive.
    pub locale: ferum_domain::Locale,
}

#[derive(Debug)]
pub struct LoginCmd {
    pub email: String,
    pub password: String,
}

#[derive(Debug)]
pub struct ResetPasswordCmd {
    pub token: String,
    pub new_password: String,
}

// ─── Results ──────────────────────────────────────────────────────────────────

pub struct LoginResult {
    pub user: User,
    pub access_token: String,
    pub refresh_token: String,
}

pub struct RefreshResult {
    pub access_token: String,
}

