#![allow(dead_code)]

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::domain::models::user::{TrustLevel, UserRole};

/// Extracted from JWT by the auth middleware.
/// Carried in request extensions as `Option<AuthUser>`.
/// None = unauthenticated guest.
#[derive(Clone, Debug)]
pub struct AuthUser {
    pub id: Uuid,
    pub username: String,
    pub role: UserRole,
    pub trust_level: TrustLevel,
    pub is_global_mod: bool,
    pub is_banned: bool,
    pub banned_until: Option<chrono::DateTime<chrono::Utc>>,
}

impl AuthUser {
    pub fn is_currently_banned(&self) -> bool {
        if !self.is_banned {
            return false;
        }
        self.banned_until
            .map(|t| t > chrono::Utc::now())
            .unwrap_or(true)
    }
}

pub async fn auth_middleware(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Response {
    let token = extract_bearer(&req).or_else(|| extract_cookie(&req));

    if let Some(token) = token {
        if let Ok(claims) = state.token_service.verify_access_token(&token) {
            let role = match claims.role.as_str() {
                "admin" => UserRole::Admin,
                "moderator" => UserRole::Moderator,
                _ => UserRole::Member,
            };
            let trust_level = match claims.trust_level.as_str() {
                "leader" => TrustLevel::Leader,
                "regular" => TrustLevel::Regular,
                "member" => TrustLevel::Member,
                "basic" => TrustLevel::Basic,
                _ => TrustLevel::New,
            };
            // Check for an immediate ban override stored in cache (set on ban actions).
            let cache_banned = state.cache.exists(&format!("user:banned:{}", claims.sub)).await;
            let is_banned = claims.is_banned || cache_banned;
            let banned_until = if cache_banned { None } else { claims.banned_until.map(|ts| {
                chrono::DateTime::from_timestamp(ts, 0).unwrap_or(chrono::DateTime::<chrono::Utc>::MAX_UTC)
            })};
            let user = AuthUser {
                id: claims.sub,
                username: claims.username,
                role,
                trust_level,
                is_global_mod: claims.is_global_mod,
                is_banned,
                banned_until,
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
