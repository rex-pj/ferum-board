pub mod api;
pub mod pages;

use serde::Deserialize;

use crate::handlers::pages::PageError;
use crate::middleware::AuthUser;

/// Query params shared by all moderation page handlers.
#[derive(Deserialize)]
pub struct ModPageQuery {
    pub page: Option<u64>,
    pub status: Option<String>,
    pub q: Option<String>,
    pub category_id: Option<String>,
    pub actor_id: Option<String>,
    pub target_type: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
}

/// Guard: caller must hold moderation.view_reports (globally or in any category) OR admin.users.
/// Using has_perm_any_category so category-scoped moderators can access the mod panel.
pub fn require_moderator(auth_user: &AuthUser) -> Result<(), PageError> {
    if !auth_user.has_perm_any_category("moderation.view_reports")
        && !auth_user.has_perm("admin.users")
    {
        return Err(PageError::Unauthorized);
    }
    Ok(())
}
