use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

use chrono::Utc;
use uuid::Uuid;

use crate::application::ports::{
    AccessTokenClaims, BulkSeedService, CacheService, PasswordHasher, TokenService,
};
use crate::application::shared::AppError;
use crate::constants::{JWT_EXPIRY_SECS, REFRESH_TOKEN_TTL_SECS};
use crate::domain::models::user::{TrustLevel, User, UserRole};
use crate::domain::repositories::{UserRepository, NewUser};
use crate::infrastructure::repositories::PgSiteConfigRepository;

pub struct SetupUseCase {
    users: Arc<dyn UserRepository>,
    hasher: Arc<dyn PasswordHasher>,
    tokens: Arc<dyn TokenService>,
    cache: Arc<dyn CacheService>,
    site_config: Arc<PgSiteConfigRepository>,
    bulk_seed: Arc<dyn BulkSeedService>,
    setup_lock: Arc<Mutex<()>>,
}

impl SetupUseCase {
    pub fn new(
        users: Arc<dyn UserRepository>,
        hasher: Arc<dyn PasswordHasher>,
        tokens: Arc<dyn TokenService>,
        cache: Arc<dyn CacheService>,
        site_config: Arc<PgSiteConfigRepository>,
        bulk_seed: Arc<dyn BulkSeedService>,
    ) -> Self {
        Self { users, hasher, tokens, cache, site_config, bulk_seed, setup_lock: Arc::new(Mutex::new(())) }
    }

    pub async fn needs_setup(&self) -> Result<bool, AppError> {
        Ok(self.users.count_admins().await? == 0)
    }

    pub async fn run_setup(&self, cmd: RunSetupCmd) -> Result<SetupResult, AppError> {
        let _lock = self.setup_lock.lock().await;

        if !self.needs_setup().await? {
            // Return 404 so the endpoint reveals nothing about the system state
            // to an attacker who didn't know setup was already done.
            return Err(AppError::NotFound);
        }

        if !crate::validators::validate_username(&cmd.admin_username) {
            return Err(AppError::unprocessable("Username must be 3–30 chars, alphanumeric/underscore/hyphen"));
        }
        if !crate::validators::validate_password(&cmd.admin_password) {
            return Err(AppError::unprocessable(crate::validators::PASSWORD_REQUIREMENTS));
        }
        if self.users.find_by_email(&cmd.admin_email).await?.is_some() {
            return Err(AppError::Conflict("email_taken".to_string()));
        }
        if self.users.find_by_username(&cmd.admin_username).await?.is_some() {
            return Err(AppError::Conflict("username_taken".to_string()));
        }

        let hash = self.hasher.hash(&cmd.admin_password).await?;
        let user = self.users.create(NewUser {
            username: cmd.admin_username,
            email: cmd.admin_email,
            password_hash: Some(hash),
            role: UserRole::Admin,
        }).await?;

        self.users.set_email_verified(user.id).await?;
        self.users.set_trust_level(user.id, TrustLevel::Member).await?;

        let user = self.users.find_by_id(user.id).await?.unwrap_or(user);

        if let Some(cfg) = cmd.config {
            let mut map = std::collections::HashMap::new();
            if let Some(v) = cfg.site_name { map.insert("site_name".to_string(), v); }
            if let Some(v) = cfg.site_tagline { map.insert("site_tagline".to_string(), v); }
            if let Some(v) = cfg.primary_color { map.insert("primary_color".to_string(), v); }
            if let Some(v) = cfg.registration_open {
                map.insert("registration_open".to_string(), v.to_string());
            }
            if let Some(v) = cfg.smtp_host { map.insert("smtp_host".to_string(), v); }
            if let Some(v) = cfg.smtp_port { map.insert("smtp_port".to_string(), v.to_string()); }
            if let Some(v) = cfg.smtp_user { map.insert("smtp_user".to_string(), v); }
            if let Some(v) = cfg.smtp_pass { map.insert("smtp_pass".to_string(), v); }
            if !map.is_empty() {
                self.site_config.set_many(&map).await?;
            }
        }

        if cmd.seed_example_data {
            self.bulk_seed.seed_bulk(user.id).await?;
        }

        let claims = AccessTokenClaims {
            sub: user.id,
            username: user.username.clone(),
            role: format!("{:?}", user.role).to_lowercase(),
            trust_level: format!("{:?}", user.trust_level).to_lowercase(),
            is_global_mod: user.is_global_mod,
            is_banned: user.is_banned,
            banned_until: user.banned_until.map(|t| t.timestamp()),
            exp: (Utc::now() + chrono::Duration::seconds(JWT_EXPIRY_SECS as i64)).timestamp(),
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

        Ok(SetupResult { user, access_token, refresh_token })
    }
}

// ─── Commands / Results ───────────────────────────────────────────────────────

pub struct RunSetupCmd {
    pub admin_username: String,
    pub admin_email: String,
    pub admin_password: String,
    pub config: Option<SetupConfigCmd>,
    pub seed_example_data: bool,
}

pub struct SetupConfigCmd {
    pub site_name: Option<String>,
    pub site_tagline: Option<String>,
    pub primary_color: Option<String>,
    pub registration_open: Option<bool>,
    pub smtp_host: Option<String>,
    pub smtp_port: Option<u16>,
    pub smtp_user: Option<String>,
    pub smtp_pass: Option<String>,
}

pub struct SetupResult {
    pub user: User,
    pub access_token: String,
    pub refresh_token: String,
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn refresh_token_key(user_id: Uuid, token: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(token.as_bytes());
    let digest = h.finalize();
    format!("refresh:{}:{}", user_id, hex::encode(&digest[..16]))
}
