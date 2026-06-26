use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::Json;
use uuid::Uuid;

use ferum_application::usecases::role_usecase::{AssignRoleCmd, CreateRoleCmd, UpdateRoleCmd};
use crate::app_state::AppState;
use crate::middleware::AuthUser;
use crate::view_models::role::{
    AssignRoleRequest, CreateRoleRequest, PermissionResponse, RoleResponse, SetPermissionsRequest,
    UpdateRoleRequest, UserRoleResponse,
};
use crate::view_models::{DataResponse, HandlerResult};

pub async fn list_roles(
    State(state): State<AppState>,
) -> HandlerResult<impl IntoResponse> {
    let roles = state.role.list_roles().await?;
    Ok(Json(DataResponse::new(
        roles.into_iter().map(RoleResponse::from).collect::<Vec<_>>(),
    )))
}

pub async fn create_role(
    State(state): State<AppState>,
    axum::Extension(auth_user): axum::Extension<Option<AuthUser>>,
    Json(body): Json<CreateRoleRequest>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(ferum_application::shared::AppError::Unauthorized)?;
    let role = state
        .role
        .create_role(
            actor,
            CreateRoleCmd {
                slug: body.slug,
                name: body.name,
                description: body.description,
                color: body.color,
                position: body.position.unwrap_or(100),
            },
        )
        .await?;
    Ok(Json(DataResponse::new(RoleResponse::from(role))))
}

pub async fn update_role(
    State(state): State<AppState>,
    axum::Extension(auth_user): axum::Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateRoleRequest>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(ferum_application::shared::AppError::Unauthorized)?;
    let role = state
        .role
        .update_role(
            actor,
            id,
            UpdateRoleCmd {
                name: body.name,
                description: body.description,
                color: body.color,
                position: body.position,
            },
        )
        .await?;
    Ok(Json(DataResponse::new(RoleResponse::from(role))))
}

pub async fn delete_role(
    State(state): State<AppState>,
    axum::Extension(auth_user): axum::Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(ferum_application::shared::AppError::Unauthorized)?;
    state.role.delete_role(actor, id).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

pub async fn get_permissions(
    State(state): State<AppState>,
    axum::Extension(auth_user): axum::Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(ferum_application::shared::AppError::Unauthorized)?;
    let perms = state.role.get_role_permissions(actor, id).await?;
    Ok(Json(DataResponse::new(
        perms.into_iter().map(PermissionResponse::from).collect::<Vec<_>>(),
    )))
}

pub async fn list_all_permissions(
    State(state): State<AppState>,
    axum::Extension(auth_user): axum::Extension<Option<AuthUser>>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(ferum_application::shared::AppError::Unauthorized)?;
    let perms = state.role.list_all_permissions(actor).await?;
    Ok(Json(DataResponse::new(
        perms.into_iter().map(PermissionResponse::from).collect::<Vec<_>>(),
    )))
}

pub async fn set_permissions(
    State(state): State<AppState>,
    axum::Extension(auth_user): axum::Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
    Json(body): Json<SetPermissionsRequest>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(ferum_application::shared::AppError::Unauthorized)?;
    state.role.set_role_permissions(actor, id, body.permission_keys).await?;
    // Invalidate the in-memory role permission cache so changes take effect immediately
    state.permission_resolver.reload().await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

pub async fn assign_role(
    State(state): State<AppState>,
    axum::Extension(auth_user): axum::Extension<Option<AuthUser>>,
    Path(user_id): Path<Uuid>,
    Json(body): Json<AssignRoleRequest>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(ferum_application::shared::AppError::Unauthorized)?;
    let assignment = state
        .role
        .assign_role(
            actor,
            AssignRoleCmd {
                user_id,
                role_id: body.role_id,
                category_id: body.category_id,
                expires_at: body.expires_at,
            },
        )
        .await?;

    // Invalidate user roles cache so next request picks up the new assignment
    state.cache.del(&format!("user:roles:{}", user_id)).await.ok();

    let roles = state.role.list_roles().await.unwrap_or_default();
    let role_map: std::collections::HashMap<Uuid, RoleResponse> =
        roles.into_iter().map(|r| (r.id, RoleResponse::from(r))).collect();

    let role_resp = role_map.get(&assignment.role_id).cloned().ok_or(
        ferum_application::shared::AppError::internal("role not found".to_string()),
    )?;
    let permissions = state
        .permission_resolver
        .permissions_for_role(assignment.role_id)
        .await;

    Ok(Json(DataResponse::new(UserRoleResponse {
        id: assignment.id,
        role: role_resp,
        category_id: assignment.category_id,
        expires_at: assignment.expires_at,
        created_at: assignment.created_at,
        permissions,
    })))
}

pub async fn revoke_role(
    State(state): State<AppState>,
    axum::Extension(auth_user): axum::Extension<Option<AuthUser>>,
    Path((user_id, role_id)): Path<(Uuid, Uuid)>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(ferum_application::shared::AppError::Unauthorized)?;
    state.role.revoke_role(actor, user_id, role_id, None).await?;
    state.cache.del(&format!("user:roles:{}", user_id)).await.ok();
    Ok(axum::http::StatusCode::NO_CONTENT)
}
