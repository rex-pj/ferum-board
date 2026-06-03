use axum::extract::{Extension, Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::middleware::AuthUser;
use crate::view_models::auth::UserResponse;
use crate::view_models::user::UserSummaryResponse;
use crate::view_models::{DataResponse, HandlerResult, PagedResponse};
use ferum_application::shared::AppError;

#[derive(Deserialize)]
pub struct UserListQuery {
    pub page: Option<u64>,
    pub per_page: Option<u64>,
    pub q: Option<String>,
}

#[derive(Deserialize)]
pub struct BanUserRequest {
    pub reason: String,
}

pub async fn admin_stats_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(AppError::Unauthorized)?;
    let stats = state.admin_stats.dashboard(actor).await?;
    Ok(Json(DataResponse::new(stats)))
}

pub async fn admin_list_users_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Query(q): Query<UserListQuery>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(AppError::Unauthorized)?;
    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(20).min(100);

    let (users, total) = state
        .admin
        .list_users(actor, page, per_page, q.q.as_deref())
        .await?;

    Ok(Json(PagedResponse::new(
        users.into_iter().map(UserSummaryResponse::from).collect(),
        total,
        page,
        per_page,
    )))
}

pub async fn admin_get_user_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(AppError::Unauthorized)?;
    let user = state.admin.get_user(actor, id).await?;
    // Fetch roles for full user detail view
    let roles = state.role.list_user_roles(actor, id).await.unwrap_or_default();
    let role_map: std::collections::HashMap<uuid::Uuid, crate::view_models::role::RoleResponse> =
        state
            .role
            .list_roles()
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|r| (r.id, crate::view_models::role::RoleResponse::from(r)))
            .collect();

    let mut resp = UserResponse::from(user);
    resp.roles = Some(
        roles
            .into_iter()
            .filter_map(|a| {
                role_map.get(&a.role_id).cloned().map(|role| {
                    crate::view_models::role::UserRoleResponse {
                        id: a.id,
                        role,
                        category_id: a.category_id,
                        expires_at: a.expires_at,
                        created_at: a.created_at,
                    }
                })
            })
            .collect(),
    );
    Ok(Json(DataResponse::new(resp)))
}

pub async fn admin_ban_user_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
    Json(body): Json<BanUserRequest>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(AppError::Unauthorized)?;
    state.admin.permanent_ban(actor, id, body.reason).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn admin_unban_user_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(AppError::Unauthorized)?;
    state.admin.unban(actor, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
