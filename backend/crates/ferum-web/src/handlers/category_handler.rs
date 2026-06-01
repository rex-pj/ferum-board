use axum::extract::{Extension, Path, State};
use axum::response::IntoResponse;
use axum::Json;

use crate::app_state::AppState;
use crate::middleware::AuthUser;
use crate::view_models::category::{
    CategoryResponse, ForumIndexGroupResponse, SubcategoryIndexResponse,
};
use crate::view_models::thread::ThreadResponse;
use crate::view_models::{DataResponse, HandlerResult};
use ferum_application::usecases::category_usecase::{ForumIndexItem, SubcategoryCount};

pub async fn list_public_categories_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
) -> HandlerResult<impl IntoResponse> {
    let categories = state.category.list_visible(auth_user.as_ref()).await?;
    let data: Vec<CategoryResponse> = categories.into_iter().map(Into::into).collect();
    Ok(Json(DataResponse::new(data)))
}

pub async fn get_category_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(slug): Path<String>,
) -> HandlerResult<impl IntoResponse> {
    let category = state
        .category
        .get_by_slug(auth_user.as_ref(), &slug)
        .await?;
    Ok(Json(DataResponse::new(CategoryResponse::from(category))))
}

pub async fn get_forum_index_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
) -> HandlerResult<impl IntoResponse> {
    let items = state.category.get_forum_index(auth_user.as_ref()).await?;
    let data: Vec<ForumIndexGroupResponse> = items
        .into_iter()
        .map(forum_index_item_to_response)
        .collect();
    Ok(Json(DataResponse::new(data)))
}

fn forum_index_item_to_response(item: ForumIndexItem) -> ForumIndexGroupResponse {
    ForumIndexGroupResponse {
        id: item.category.id,
        slug: item.category.slug,
        name: item.category.name,
        description: item.category.description,
        color: item.category.color,
        position: item.category.position,
        thread_count: item.thread_count,
        subcategories: item
            .subcategories
            .into_iter()
            .map(sub_to_response)
            .collect(),
        recent_threads: item
            .recent_threads
            .into_iter()
            .map(ThreadResponse::from)
            .collect(),
    }
}

fn sub_to_response(s: SubcategoryCount) -> SubcategoryIndexResponse {
    SubcategoryIndexResponse {
        id: s.category.id,
        slug: s.category.slug,
        name: s.category.name,
        description: s.category.description,
        color: s.category.color,
        thread_count: s.thread_count,
    }
}
