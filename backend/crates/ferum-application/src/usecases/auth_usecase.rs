use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use uuid::Uuid;

use crate::constants::{
    ACCOUNT_LOCKOUT_ATTEMPTS, ACCOUNT_LOCKOUT_DURATION_MINUTES, PASSWORD_RESET_TOKEN_TTL_SECS,
    REFRESH_TOKEN_TTL_SECS,
};
use crate::ports::{
    AccessTokenClaims, CacheService, ForumJob, HookContext, HookDecision, JobQueue,
    NullPluginRuntime, PasswordHasher, PluginRuntime, TokenService,
};
use crate::shared::AppError;
use ferum_domain::models::user::{TrustLevel, User};
use ferum_domain::repositories::user_repository::{NewUser, UserRepository};

pub struct AuthUseCase {
    pub users: Arc<dyn UserRepository>,
    pub hasher: Arc<dyn PasswordHasher>,
    pub tokens: Arc<dyn TokenService>,
    pub cache: Arc<dyn CacheService>,
    pub jobs: Arc<dyn JobQueue>,
    pub plugin_runtime: Arc<dyn PluginRuntime>,
    /// When true, newly registered users are immediately verified (no email required).
    /// Set to true when SMTP_HOST is not configured.
    pub auto_verify_email: bool,
}

impl AuthUseCase {
    pub fn new(
        users: Arc<dyn UserRepository>,
        hasher: Arc<dyn PasswordHasher>,
        tokens: Arc<dyn TokenService>,
        cache: Arc<dyn CacheService>,
        jobs: Arc<dyn JobQueue>,
    ) -> Self {
        Self {
            users,
            hasher,
            tokens,
            cache,
            jobs,
            plugin_runtime: Arc::new(NullPluginRuntime),
            auto_verify_email: false,
        }
    }

    pub fn with_auto_verify_email(mut self, enabled: bool) -> Self {
        self.auto_verify_email = enabled;
        self
    }

    pub fn with_plugin_runtime(mut self, runtime: Arc<dyn PluginRuntime>) -> Self {
        self.plugin_runtime = runtime;
        self
    }

    // ─── Register ─────────────────────────────────────────────────────────────

    pub async fn register(&self, cmd: RegisterCmd) -> Result<User, AppError> {
        if !crate::validators::validate_username(&cmd.username) {
            return Err(AppError::unprocessable(
                "Username must be 3–30 chars, alphanumeric/underscore/hyphen",
            ));
        }
        if !crate::validators::validate_password(&cmd.password) {
            return Err(AppError::unprocessable(crate::validators::PASSWORD_REQUIREMENTS));
        }

        let email = cmd.email.to_lowercase();

        if self.users.find_by_email(&email).await?.is_some() {
            return Err(AppError::Conflict("email_taken".to_string()));
        }
        if self.users.find_by_username(&cmd.username).await?.is_some() {
            return Err(AppError::Conflict("username_taken".to_string()));
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

        if self.auto_verify_email {
            self.users.set_email_verified(user.id).await?;
            self.users.set_trust_level(user.id, TrustLevel::Basic).await?;
        } else {
            let token = self.tokens.mint_email_token(user.id, "email_verification")?;
            self.jobs
                .enqueue(ForumJob::SendEmailVerification {
                    user_id: user.id,
                    email,
                    token,
                })
                .await?;
        }

        Ok(user)
    }

    // ─── Verify email ─────────────────────────────────────────────────────────

    pub async fn verify_email(&self, token: &str) -> Result<(), AppError> {
        let user_id = self
            .tokens
            .verify_email_token(token, "email_verification")
            .map_err(|_| AppError::forbidden("invalid_or_expired_token"))?;

        let user = self.users.find_by_id(user_id).await?.ok_or(AppError::NotFound)?;

        if !user.is_email_verified {
            self.users.set_email_verified(user_id).await?;
            self.users.set_trust_level(user_id, TrustLevel::Basic).await?;
        }

        Ok(())
    }

    // ─── Login ────────────────────────────────────────────────────────────────

    pub async fn login(&self, cmd: LoginCmd) -> Result<LoginResult, AppError> {
        let user_opt = self.users.find_by_email(&cmd.email.to_lowercase()).await?;

        if user_opt.is_none() {
            let _ = self
                .hasher
                .verify(&cmd.password, "$2b$12$invalidhashpaddinginvalidhashpa")
                .await;
            return Err(AppError::Unauthorized);
        }
        let mut user = user_opt.unwrap();

        if user.is_account_locked() {
            let _ = self
                .hasher
                .verify(&cmd.password, "$2b$12$invalidhashpaddinginvalidhashpa")
                .await;
            return Err(AppError::forbidden("account_locked"));
        }

        let hash = user.password_hash.as_deref().unwrap_or("$2b$12$invalidhashpaddinginvalidhashpa");
        let valid = self.hasher.verify(&cmd.password, hash).await?;

        if user.password_hash.is_none() || !valid {
            let count = self.users.increment_failed_login(user.id).await?;
            if count >= ACCOUNT_LOCKOUT_ATTEMPTS {
                let until = Utc::now()
                    + chrono::Duration::minutes(ACCOUNT_LOCKOUT_DURATION_MINUTES as i64);
                self.users.lock_until(user.id, until).await?;
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

        let trust_str = format!("{:?}", user.trust_level).to_lowercase();
        let claims = AccessTokenClaims {
            sub: user.id,
            username: user.username.clone(),
            trust_level: trust_str,
            is_banned: user.is_banned,
            banned_until: user.banned_until.map(|t| t.timestamp()),
            exp: (Utc::now()
                + chrono::Duration::seconds(crate::constants::JWT_EXPIRY_SECS as i64))
            .timestamp(),
        };
        let access_token = self.tokens.mint_access_token(&claims)?;
        let refresh_token = self.tokens.mint_refresh_token(user.id)?;

        self.cache
            .set(
                &refresh_token_key(user.id, &refresh_token),
                "1",
                Duration::from_secs(REFRESH_TOKEN_TTL_SECS),
            )
            .await?;

        Ok(LoginResult { user, access_token, refresh_token })
    }

    // ─── Refresh ──────────────────────────────────────────────────────────────

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

        let trust_str = format!("{:?}", user.trust_level).to_lowercase();
        let claims = AccessTokenClaims {
            sub: user.id,
            username: user.username.clone(),
            trust_level: trust_str,
            is_banned: user.is_banned,
            banned_until: user.banned_until.map(|t| t.timestamp()),
            exp: (Utc::now()
                + chrono::Duration::seconds(crate::constants::JWT_EXPIRY_SECS as i64))
            .timestamp(),
        };
        let access_token = self.tokens.mint_access_token(&claims)?;

        Ok(RefreshResult { access_token })
    }

    // ─── Logout ───────────────────────────────────────────────────────────────

    pub async fn logout(&self, user_id: Uuid, refresh_token: &str) -> Result<(), AppError> {
        self.cache.del(&refresh_token_key(user_id, refresh_token)).await?;
        Ok(())
    }

    // ─── Forgot password ──────────────────────────────────────────────────────

    pub async fn forgot_password(&self, email: &str) -> Result<(), AppError> {
        if let Some(user) = self.users.find_by_email(&email.to_lowercase()).await? {
            let token = self.tokens.mint_email_token(user.id, "password_reset")?;
            self.jobs
                .enqueue(ForumJob::SendPasswordResetEmail { email: user.email, token })
                .await?;
        }
        Ok(())
    }

    // ─── Reset password ───────────────────────────────────────────────────────

    pub async fn reset_password(&self, cmd: ResetPasswordCmd) -> Result<(), AppError> {
        if !crate::validators::validate_password(&cmd.new_password) {
            return Err(AppError::unprocessable(crate::validators::PASSWORD_REQUIREMENTS));
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

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn refresh_token_key(user_id: Uuid, token: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(token.as_bytes());
    let digest = h.finalize();
    format!("refresh:{}:{}", user_id, hex::encode(&digest[..16]))
}
