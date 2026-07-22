use axum::extract::{Extension, Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::middleware::{AuthUser, AuthUserExt};
use crate::view_models::auth::UserResponse;
use crate::view_models::lookup::{LookupOption, LookupQuery};
use crate::view_models::user::UserSummaryResponse;
use crate::view_models::{DataResponse, HandlerResult, PagedResponse};
use ferum_application::usecases::user_usecase::UpdateProfileCmd;
use ferum_domain::models::user::TrustLevel;

#[derive(Deserialize)]
pub struct UserListQuery {
    pub page: Option<u64>,
    pub per_page: Option<u64>,
    pub q: Option<String>,
}

#[derive(Deserialize)]
pub struct BanUserRequest {
    pub reason: String,
    pub banned_until: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Deserialize)]
pub struct EditUserRequest {
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub website: Option<String>,
}

#[derive(Deserialize)]
pub struct SetTrustLevelRequest {
    pub trust_level: String,
}

pub async fn list_users(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Query(q): Query<UserListQuery>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(20).min(100);

    let (users, total) = state
        .admin
        .list_users(actor, page, per_page, q.q.as_deref(), None, None)
        .await?;

    Ok(Json(PagedResponse::new(
        users.into_iter().map(UserSummaryResponse::from).collect(),
        total,
        page,
        per_page,
    )))
}

pub async fn get_user(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    let user = state.admin.get_user(actor, id).await?;
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

    let mut role_responses = Vec::with_capacity(roles.len());
    for a in roles {
        if let Some(role) = role_map.get(&a.role_id).cloned() {
            let permissions = state
                .permission_resolver
                .permissions_for_role(a.role_id)
                .await;
            role_responses.push(crate::view_models::role::UserRoleResponse {
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

pub async fn ban_user(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
    Json(body): Json<BanUserRequest>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    state.admin.ban(actor, id, body.reason, body.banned_until).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn unban_user(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    state.admin.unban(actor, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn edit_user(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
    Json(body): Json<EditUserRequest>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    let user = state
        .admin
        .edit_user(
            actor,
            id,
            UpdateProfileCmd {
                display_name: body.display_name,
                bio: body.bio,
                website: body.website,
            },
        )
        .await?;
    Ok(Json(DataResponse::new(UserResponse::from(user))))
}

pub async fn set_trust_level(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
    Json(body): Json<SetTrustLevelRequest>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    let level = match body.trust_level.as_str() {
        "new" => TrustLevel::New,
        "basic" => TrustLevel::Basic,
        "member" => TrustLevel::Member,
        "regular" => TrustLevel::Regular,
        "leader" => TrustLevel::Leader,
        _ => {
            return Err(ferum_application::shared::AppError::invalid("invalid_trust_level")
            .into())
        }
    };
    state.admin.set_trust_level(actor, id, level).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn unlock_user(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    state.admin.unlock_user(actor, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn verify_user_email(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    state.admin.verify_user_email(actor, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn lookup_users(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Query(q): Query<LookupQuery>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(20).clamp(1, 50);
    let search = q.q.as_deref().filter(|s| !s.is_empty());
    let (users, total) = state.admin.search_users_lookup(actor, search, page, per_page).await?;
    let data: Vec<LookupOption> = users.into_iter().map(LookupOption::from).collect();
    Ok(Json(PagedResponse::new(data, total, page, per_page)))
}
