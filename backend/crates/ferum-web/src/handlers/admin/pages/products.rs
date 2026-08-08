use axum::extract::State;
use axum::response::IntoResponse;
use axum::Extension;
use tera::Context;

use super::super::{render_admin, require_admin, site_ctx};
use crate::app_state::AppState;
use crate::handlers::pages::{require_page_auth, PageError};
use crate::middleware::AuthUser;
use crate::view_models::page_context::CurrentUserCtx;

/// Catalog admin page. The table and forms are driven client-side by
/// `ferum-admin-products.js`, which reads `/api/admin/products` and `/api/materials`.
pub async fn products(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Extension(req_locale): Extension<crate::middleware::locale::RequestLocale>,
) -> Result<impl IntoResponse, PageError> {
    let auth_user = require_page_auth(auth_user)?;
    require_admin(&auth_user)?;

    // The catalogue taxonomy, for the product form's picker and the list
    // filter. Server-rendered rather than fetched: the list is small, static
    // per page load, and one fewer request before the form is usable.
    //
    // These are *product* categories (Sofa, Ghế, Bàn), not the forum's
    // discussion tree — the two are separate on purpose.
    let categories: Vec<serde_json::Value> = state
        .product
        .list_categories()
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|c| {
            serde_json::json!({
                "id": c.id.to_string(),
                "name": c.name,
                "icon": c.icon,
                "keywords": c.match_keywords.join(", "),
                "position": c.position,
                "is_child": c.parent_id.is_some(),
            })
        })
        .collect();

    let mut ctx = Context::new();
    ctx.insert("site", &site_ctx(&state).await);
    ctx.insert("current_user", &CurrentUserCtx::from(&auth_user));
    ctx.insert("product_categories", &categories);

    render_admin(&state, &req_locale, "admin/products/list.html", ctx).await
}
