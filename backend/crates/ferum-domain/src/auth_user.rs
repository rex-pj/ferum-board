use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::models::user::{TrustLevel, UserRole};

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
    pub banned_until: Option<DateTime<Utc>>,
}

impl AuthUser {
    pub fn is_currently_banned(&self) -> bool {
        if !self.is_banned {
            return false;
        }
        self.banned_until.map(|t| t > Utc::now()).unwrap_or(true)
    }
}
