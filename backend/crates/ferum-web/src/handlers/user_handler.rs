use axum::extract::{Extension, Path, Query, State};
use axum::response::IntoResponse;
use axum::Json;

use crate::app_state::AppState;
use crate::middleware::AuthUser;
use crate::view_models::auth::UserResponse;
use crate::view_models::thread::{ThreadListQuery, ThreadResponse};
use crate::view_models::user::UserSummaryResponse;
use crate::view_models::{DataResponse, HandlerResult, PagedResponse};
use ferum_application::shared::AppError;

pub async fn get_public_profile_handler(
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> HandlerResult<impl IntoResponse> {
    let user = state
        .auth
        .users
        .find_by_username(&username)
        .await?
        .ok_or(AppError::NotFound)?;

    Ok(Json(DataResponse::new(UserSummaryResponse::from(user))))
}

pub async fn list_user_threads_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(username): Path<String>,
    Query(q): Query<ThreadListQuery>,
) -> HandlerResult<impl IntoResponse> {
    let user = state
        .auth
        .users
        .find_by_username(&username)
        .await?
        .ok_or(AppError::NotFound)?;

    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(20);

    let (threads, total) = state
        .thread
        .list_by_author(auth_user.as_ref(), user.id, page, per_page)
        .await?;

    Ok(Json(PagedResponse::new(
        threads.into_iter().map(ThreadResponse::from).collect(),
        total,
        page,
        per_page,
    )))
}

pub async fn get_me_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
) -> HandlerResult<impl IntoResponse> {
    use crate::view_models::role::{RoleResponse, UserRoleResponse};
    use std::collections::HashMap;
    use uuid::Uuid;

    let auth = auth_user.as_ref().ok_or(AppError::Unauthorized)?;
    let user = state
        .auth
        .users
        .find_by_id(auth.id)
        .await?
        .ok_or(AppError::NotFound)?;

    // Include full role assignments so frontend can compute permissions and category scope
    let assignments = state.user_role_repo.list_for_user(auth.id).await.unwrap_or_default();
    let all_roles = state.role.list_roles().await.unwrap_or_default();
    let role_map: HashMap<Uuid, RoleResponse> =
        all_roles.into_iter().map(|r| (r.id, RoleResponse::from(r))).collect();

    let mut role_responses = Vec::with_capacity(assignments.len());
    for a in assignments {
        if let Some(role) = role_map.get(&a.role_id).cloned() {
            let permissions = state
                .role_permission_cache
                .permissions_for_role(a.role_id)
                .await;
            role_responses.push(UserRoleResponse {
                id: a.id,
                role,
                category_id: a.category_id,
                expires_at: a.expires_at,
                created_at: a.created_at,
                permissions,
            });
        }
    }

    let mut resp = UserResponse::from(user);
    resp.roles = Some(role_responses);

    Ok(Json(DataResponse::new(resp)))
}
