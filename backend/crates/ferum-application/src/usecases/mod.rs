pub mod admin_stats_usecase;
pub mod admin_usecase;
pub mod auth_usecase;
pub mod bookmark_usecase;
pub mod email_template_usecase;
pub mod follow_usecase;
pub mod category_usecase;
pub mod moderation_usecase;
pub mod notification_usecase;
pub mod post_usecase;
pub mod product_usecase;
pub mod reaction_usecase;
pub mod review_usecase;
pub mod role_usecase;
pub mod search_usecase;
pub mod setup_usecase;
pub mod storage_audit_usecase;
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
    // 32 hex characters = the first 16 digest bytes.
    format!(
        "{}{}",
        refresh_token_prefix(user_id),
        crate::digest::short_hex(token.as_bytes(), 32)
    )
}

/// The key prefix covering *every* refresh token held for `user_id`.
///
/// One definition because `del_prefix` with a mistyped prefix silently deletes
/// nothing, and the failure is invisible: the caller still returns success and
/// the sessions it meant to end keep working.
pub(crate) fn refresh_token_prefix(user_id: uuid::Uuid) -> String {
    format!("refresh:{user_id}:")
}

/// Builds an `AccessTokenClaims` struct from a `User`. Callers pass the expiry
/// from `TokenService::access_token_ttl_secs()` so the claim `exp` always
/// matches the TTL the token service (and cookie Max-Age) is configured with.
pub(crate) fn build_access_token_claims(
    user: &ferum_domain::models::user::User,
    expiry_secs: u64,
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
        exp: (Utc::now() + chrono::Duration::seconds(expiry_secs as i64)).timestamp(),
        iat: Utc::now().timestamp(),
    }
}

/// Cache key holding the Unix timestamp of a user's most recent session
/// invalidation. An access token whose `iat` predates this value is refused by
/// the auth middleware.
///
/// Deliberately keyed per user and written with a TTL equal to the access-token
/// lifetime: once every token issued before the epoch has expired on its own,
/// the marker has no work left to do and can be evicted.
pub fn session_epoch_key(user_id: uuid::Uuid) -> String {
    format!("user:session_epoch:{user_id}")
}

/// How long a session-epoch marker is retained.
///
/// Only needs to outlive the longest access token that could still be in flight
/// when it is written. Fixed at 30 days rather than derived from
/// `JWT_EXPIRY_SECONDS` so that raising that setting can never silently shorten
/// the window and resurrect tokens the user already revoked. One small key per
/// user who logged out or changed a password is a negligible cost for removing
/// that failure mode.
const SESSION_EPOCH_TTL: std::time::Duration = std::time::Duration::from_secs(30 * 24 * 60 * 60);

/// Revokes every access token issued to `user_id` before the returned epoch,
/// which callers re-issuing a token in the same request must use as its `iat`.
///
/// **Epoch is `now + 1`, not `now`.** `iat` has whole-second resolution, so a
/// token minted in the same second would have `iat == now` and survive the
/// `iat < epoch` test — logging out within a second of logging in would revoke
/// nothing. Best-effort: a cache failure is logged, not returned.
pub async fn invalidate_sessions(
    cache: &dyn crate::ports::CacheService,
    user_id: uuid::Uuid,
) -> i64 {
    let epoch = chrono::Utc::now().timestamp() + 1;
    if let Err(e) = cache
        .set(
            &session_epoch_key(user_id),
            &epoch.to_string(),
            SESSION_EPOCH_TTL,
        )
        .await
    {
        tracing::warn!(
            user_id = %user_id,
            error = %e,
            "failed to write session epoch; previously issued access tokens remain valid until exp"
        );
    }
    epoch
}

/// Ends **every** session for `user_id` — refresh tokens first, then access.
///
/// Use this, not [`invalidate_sessions`], for a credential change: the epoch
/// only withdraws tokens that already exist, and a surviving refresh key mints
/// fresh ones that clear it for the whole refresh lifetime. Refresh keys go
/// first so a racing refresh either fails or yields a token the epoch revokes.
pub async fn revoke_all_sessions(
    cache: &dyn crate::ports::CacheService,
    user_id: uuid::Uuid,
) -> i64 {
    if let Err(e) = cache.del_prefix(&refresh_token_prefix(user_id)).await {
        tracing::warn!(
            user_id = %user_id,
            error = %e,
            "failed to drop refresh tokens; existing sessions can still mint access tokens"
        );
    }
    invalidate_sessions(cache, user_id).await
}
