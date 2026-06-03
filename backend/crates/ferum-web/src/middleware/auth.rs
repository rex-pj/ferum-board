#![allow(dead_code)]

use std::time::Duration;

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;

use ferum_domain::models::user::TrustLevel;
use ferum_domain::AuthUser;

use crate::app_state::AppState;

/// Cache TTL for user role assignments (per-user, stored as JSON in Redis).
const USER_ROLES_CACHE_TTL: Duration = Duration::from_secs(300); // 5 min

pub async fn auth_middleware(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Response {
    let token = extract_bearer(&req).or_else(|| extract_cookie(&req));

    if let Some(token) = token {
        if let Ok(claims) = state.token_service.verify_access_token(&token) {
            let trust_level = match claims.trust_level.as_str() {
                "leader" => TrustLevel::Leader,
                "regular" => TrustLevel::Regular,
                "member" => TrustLevel::Member,
                "basic" => TrustLevel::Basic,
                _ => TrustLevel::New,
            };

            let cache_banned = state
                .cache
                .exists(&format!("user:banned:{}", claims.sub))
                .await;
            let is_banned = claims.is_banned || cache_banned;
            let banned_until = if cache_banned {
                None
            } else {
                claims.banned_until.map(|ts| {
                    chrono::DateTime::from_timestamp(ts, 0)
                        .unwrap_or(chrono::DateTime::<chrono::Utc>::MAX_UTC)
                })
            };

            // Resolve role assignments → permissions (cached in Redis, TTL 5min)
            let (permissions, category_permissions) =
                resolve_permissions(&state, claims.sub).await;

            let user = AuthUser {
                id: claims.sub,
                username: claims.username,
                trust_level,
                is_banned,
                banned_until,
                permissions,
                category_permissions,
            };
            req.extensions_mut().insert(Some(user));
        } else {
            req.extensions_mut().insert(Option::<AuthUser>::None);
        }
    } else {
        req.extensions_mut().insert(Option::<AuthUser>::None);
    }

    next.run(req).await
}

/// Resolve effective permissions for a user. Attempts Redis cache first;
/// falls back to DB query + RolePermissionCache on miss.
async fn resolve_permissions(
    state: &AppState,
    user_id: uuid::Uuid,
) -> (
    std::collections::HashSet<String>,
    std::collections::HashMap<uuid::Uuid, std::collections::HashSet<String>>,
) {
    let cache_key = format!("user:roles:{user_id}");

    // Try cache first
    if let Some(cached) = state.cache.get(&cache_key).await {
        if let Ok(assignments) = serde_json::from_str::<Vec<ferum_domain::models::role::UserRoleAssignment>>(&cached) {
            let global = state
                .role_permission_cache
                .resolve_global(&assignments)
                .await;
            let category = state
                .role_permission_cache
                .resolve_category(&assignments)
                .await;
            return (global, category);
        }
    }

    // Cache miss — fetch from DB
    let assignments = state
        .user_role_repo
        .list_for_user(user_id)
        .await
        .unwrap_or_default();

    // Store in cache
    if let Ok(json) = serde_json::to_string(&assignments) {
        let _ = state
            .cache
            .set(&cache_key, &json, USER_ROLES_CACHE_TTL)
            .await;
    }

    let global = state
        .role_permission_cache
        .resolve_global(&assignments)
        .await;
    let category = state
        .role_permission_cache
        .resolve_category(&assignments)
        .await;

    (global, category)
}

fn extract_bearer(req: &Request) -> Option<String> {
    req.headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

fn extract_cookie(req: &Request) -> Option<String> {
    req.headers()
        .get(axum::http::header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|cookies| {
            cookies.split(';').find_map(|part| {
                let part = part.trim();
                part.strip_prefix("token=").map(|t| t.to_string())
            })
        })
}
