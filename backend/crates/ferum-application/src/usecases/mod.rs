pub mod admin_stats_usecase;
pub mod admin_usecase;
pub mod auth_usecase;
pub mod bookmark_usecase;
pub mod follow_usecase;
pub mod category_usecase;
pub mod moderation_usecase;
pub mod notification_usecase;
pub mod post_usecase;
pub mod reaction_usecase;
pub mod role_usecase;
pub mod search_usecase;
pub mod setup_usecase;
pub mod tag_usecase;
pub mod thread_usecase;
pub mod user_usecase;
pub mod plugin_usecase;
pub mod webhook_usecase;
pub mod theme_usecase;

// ─── Shared usecase helpers ───────────────────────────────────────────────────

/// Derives the Redis key for a refresh token by hashing the raw token value.
/// Stored as `refresh:<user_id>:<first-16-bytes-hex>` to avoid putting raw
/// tokens in cache keys while keeping the mapping collision-resistant.
pub(crate) fn refresh_token_key(user_id: uuid::Uuid, token: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(token.as_bytes());
    let digest = h.finalize();
    format!("refresh:{}:{}", user_id, hex::encode(&digest[..16]))
}

/// Builds an `AccessTokenClaims` struct from a `User`, using the configured
/// JWT expiry. Centralised here to keep every token mint consistent.
pub(crate) fn build_access_token_claims(
    user: &ferum_domain::models::user::User,
) -> crate::ports::AccessTokenClaims {
    use chrono::Utc;
    crate::ports::AccessTokenClaims {
        sub: user.id,
        username: user.username.clone(),
        display_name: user.display_name.clone(),
        avatar_url: user.avatar_url.clone(),
        trust_level: format!("{:?}", user.trust_level).to_lowercase(),
        is_banned: user.is_banned,
        banned_until: user.banned_until.map(|t| t.timestamp()),
        exp: (Utc::now()
            + chrono::Duration::seconds(crate::constants::JWT_EXPIRY_SECS as i64))
        .timestamp(),
    }
}
