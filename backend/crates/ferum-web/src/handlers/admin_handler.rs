use axum::extract::{Extension, Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use uuid::Uuid;
use validator::Validate;

use crate::app_state::AppState;
use crate::middleware::AuthUser;
use crate::view_models::category::{
    parse_post_policy, parse_view_policy, AssignModeratorRequest, CategoryModeratorResponse,
    CategoryResponse, CreateCategoryRequest, UpdateCategoryRequest,
};
use crate::view_models::report::{ReportListQuery, ReportResponse};
use crate::view_models::{DataResponse, HandlerResult, PagedResponse};
use ferum_application::shared::AppError;
use ferum_application::usecases::admin_usecase::{CreateCategoryCmd, UpdateCategoryCmd};
use ferum_domain::models::report::ReportStatus;

pub async fn list_categories_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
) -> HandlerResult<impl IntoResponse> {
    let user = auth_user.as_ref().ok_or(AppError::Unauthorized)?;
    let categories = state.admin.list_categories(user).await?;
    let data: Vec<CategoryResponse> = categories.into_iter().map(Into::into).collect();
    Ok(Json(DataResponse::new(data)))
}

pub async fn create_category_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Json(body): Json<CreateCategoryRequest>,
) -> HandlerResult<impl IntoResponse> {
    body.validate()
        .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;
    let user = auth_user.as_ref().ok_or(AppError::Unauthorized)?;

    let view_policy = parse_view_policy(&body.view_policy)
        .ok_or_else(|| AppError::unprocessable("Invalid view_policy value"))?;
    let post_policy = parse_post_policy(&body.post_policy)
        .ok_or_else(|| AppError::unprocessable("Invalid post_policy value"))?;

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

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(CategoryResponse::from(category))),
    ))
}

pub async fn update_category_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateCategoryRequest>,
) -> HandlerResult<impl IntoResponse> {
    body.validate()
        .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;
    let user = auth_user.as_ref().ok_or(AppError::Unauthorized)?;

    let view_policy = body
        .view_policy
        .as_deref()
        .map(|s| parse_view_policy(s).ok_or_else(|| AppError::unprocessable("Invalid view_policy")))
        .transpose()?;
    let post_policy = body
        .post_policy
        .as_deref()
        .map(|s| parse_post_policy(s).ok_or_else(|| AppError::unprocessable("Invalid post_policy")))
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

pub async fn delete_category_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
) -> HandlerResult<impl IntoResponse> {
    let user = auth_user.as_ref().ok_or(AppError::Unauthorized)?;
    state.admin.delete_category(user, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_category_moderators_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(category_id): Path<Uuid>,
) -> HandlerResult<impl IntoResponse> {
    let user = auth_user.as_ref().ok_or(AppError::Unauthorized)?;
    let pairs = state
        .admin
        .list_category_moderators_with_users(user, category_id)
        .await?;
    let data: Vec<CategoryModeratorResponse> = pairs
        .into_iter()
        .map(|(m, u)| CategoryModeratorResponse {
            category_id: m.category_id,
            user_id: m.user_id,
            username: u.username,
            display_name: u.display_name,
            assigned_by: m.assigned_by_id,
            assigned_at: m.assigned_at,
        })
        .collect();
    Ok(Json(DataResponse::new(data)))
}

pub async fn assign_moderator_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(category_id): Path<Uuid>,
    Json(body): Json<AssignModeratorRequest>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(AppError::Unauthorized)?;
    let assignment = state
        .admin
        .assign_moderator(actor, category_id, body.user_id)
        .await?;
    let mod_user = state
        .admin
        .users
        .find_by_id(assignment.user_id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(CategoryModeratorResponse {
            category_id: assignment.category_id,
            user_id: assignment.user_id,
            username: mod_user.username,
            display_name: mod_user.display_name,
            assigned_by: assignment.assigned_by_id,
            assigned_at: assignment.assigned_at,
        })),
    ))
}

pub async fn list_admin_reports_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Query(q): Query<ReportListQuery>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(AppError::Unauthorized)?;
    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(20);

    let status = q.status.as_deref().and_then(|s| match s {
        "pending" => Some(ReportStatus::Pending),
        "resolved" => Some(ReportStatus::Resolved),
        "dismissed" => Some(ReportStatus::Dismissed),
        _ => None,
    });
    let target_type = q.target_type.as_deref();

    let (reports, total) = state
        .moderation
        .list_all_reports(actor, status, target_type, page, per_page)
        .await?;
    Ok(Json(PagedResponse::new(
        reports.into_iter().map(ReportResponse::from).collect(),
        total,
        page,
        per_page,
    )))
}

pub async fn revoke_moderator_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path((category_id, user_id)): Path<(Uuid, Uuid)>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(AppError::Unauthorized)?;
    state
        .admin
        .revoke_moderator(actor, category_id, user_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
