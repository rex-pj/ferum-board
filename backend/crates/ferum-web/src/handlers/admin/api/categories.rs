use axum::extract::{Extension, Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;
use validator::Validate;

use crate::app_state::AppState;
use crate::middleware::{AuthUser, AuthUserExt};
use crate::view_models::category::{
    parse_post_policy, parse_view_policy, CategoryResponse, CreateCategoryRequest,
    UpdateCategoryRequest,
};
use crate::view_models::role::RoleResponse;
use crate::view_models::{DataResponse, HandlerResult};
use ferum_application::shared::AppError;
use ferum_application::usecases::admin_usecase::{CreateCategoryCmd, UpdateCategoryCmd};

// ─── Category CRUD ────────────────────────────────────────────────────────────

pub async fn list_categories(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
) -> HandlerResult<impl IntoResponse> {
    let user = auth_user.require_auth()?;
    let categories = state.admin.list_categories(user).await?;
    Ok(Json(DataResponse::new(
        categories.into_iter().map(CategoryResponse::from).collect::<Vec<_>>(),
    )))
}

pub async fn create_category(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Json(body): Json<CreateCategoryRequest>,
) -> HandlerResult<impl IntoResponse> {
    body.validate().map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;
    let user = auth_user.require_auth()?;

    let view_policy = parse_view_policy(&body.view_policy)
        .ok_or_else(|| AppError::invalid("invalid_view_policy"))?;
    let post_policy = parse_post_policy(&body.post_policy)
        .ok_or_else(|| AppError::invalid("invalid_post_policy"))?;

    let category = state
        .admin
        .create_category(
            user,
            CreateCategoryCmd {
                name: body.name,
                slug: body.slug,
                description: body.description,
                parent_id: body.parent_id,
                position: body.position,
                view_policy,
                post_policy,
                color: body.color,
            },
        )
        .await?;

    Ok((StatusCode::CREATED, Json(DataResponse::new(CategoryResponse::from(category)))))
}

pub async fn update_category(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateCategoryRequest>,
) -> HandlerResult<impl IntoResponse> {
    body.validate().map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;
    let user = auth_user.require_auth()?;

    let view_policy = body
        .view_policy
        .as_deref()
        .map(|s| parse_view_policy(s).ok_or_else(|| AppError::invalid("invalid_view_policy")))
        .transpose()?;
    let post_policy = body
        .post_policy
        .as_deref()
        .map(|s| parse_post_policy(s).ok_or_else(|| AppError::invalid("invalid_post_policy")))
        .transpose()?;

    let category = state
        .admin
        .update_category(
            user,
            id,
            UpdateCategoryCmd {
                name: body.name,
                slug: body.slug,
                description: body.description,
                parent_id: body.parent_id,
                position: body.position,
                view_policy,
                post_policy,
                color: body.color,
            },
        )
        .await?;

    Ok(Json(DataResponse::new(CategoryResponse::from(category))))
}

pub async fn delete_category(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
) -> HandlerResult<impl IntoResponse> {
    let user = auth_user.require_auth()?;
    state.admin.delete_category(user, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ─── Category moderator assignment ───────────────────────────────────────────

#[derive(Serialize)]
struct CategoryModeratorResponse {
    user_id: Uuid,
    username: String,
    display_name: Option<String>,
    role: RoleResponse,
    category_id: Uuid,
    granted_by: Option<Uuid>,
    created_at: DateTime<Utc>,
}

#[derive(serde::Deserialize)]
pub struct AssignModeratorRequest {
    pub user_id: Uuid,
}

pub async fn list_moderators(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(category_id): Path<Uuid>,
) -> HandlerResult<impl IntoResponse> {
    let user = auth_user.require_auth()?;
    let pairs = state.admin.list_category_moderators(user, category_id).await?;

    let all_roles = state.role.list_roles().await.unwrap_or_default();
    let role_map: std::collections::HashMap<Uuid, RoleResponse> =
        all_roles.into_iter().map(|r| (r.id, RoleResponse::from(r))).collect();

    let data: Vec<CategoryModeratorResponse> = pairs
        .into_iter()
        .filter_map(|(assignment, u)| {
            role_map.get(&assignment.role_id).cloned().map(|role| CategoryModeratorResponse {
                user_id: u.id,
                username: u.username,
                display_name: u.display_name,
                role,
                category_id: assignment.category_id.unwrap_or(category_id),
                granted_by: assignment.granted_by,
                created_at: assignment.created_at,
            })
        })
        .collect();

    Ok(Json(DataResponse::new(data)))
}

pub async fn assign_moderator(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(category_id): Path<Uuid>,
    Json(body): Json<AssignModeratorRequest>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    let assignment = state.admin.assign_moderator(actor, category_id, body.user_id).await?;

    let mod_user =
        state.admin.users.find_by_id(assignment.user_id).await?.ok_or(AppError::NotFound)?;

    let all_roles = state.role.list_roles().await.unwrap_or_default();
    let role = all_roles
        .into_iter()
        .find(|r| r.id == assignment.role_id)
        .map(RoleResponse::from)
        .ok_or_else(|| AppError::internal("role not found".to_string()))?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(CategoryModeratorResponse {
            user_id: mod_user.id,
            username: mod_user.username,
            display_name: mod_user.display_name,
            role,
            category_id: assignment.category_id.unwrap_or(category_id),
            granted_by: assignment.granted_by,
            created_at: assignment.created_at,
        })),
    ))
}

pub async fn revoke_moderator(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path((category_id, user_id)): Path<(Uuid, Uuid)>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    state.admin.revoke_moderator(actor, category_id, user_id).await?;
    Ok(StatusCode::NO_CONTENT)
}
