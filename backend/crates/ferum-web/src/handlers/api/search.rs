use axum::extract::{Extension, Query, State};
use axum::response::IntoResponse;
use axum::Json;

use crate::app_state::AppState;
use crate::middleware::AuthUser;
use crate::view_models::search::{SearchHitResponse, SearchQuery};
use crate::view_models::{HandlerResult, PagedResponse};

pub async fn search(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Query(q): Query<SearchQuery>,
) -> HandlerResult<impl IntoResponse> {
    let query = q.q.unwrap_or_default();
    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(20).clamp(1, 50);

    let category_ids: Vec<uuid::Uuid> = match q.category_id {
        None => vec![],
        Some(selected_id) => {
            let cats = state.category.list_visible(auth_user.as_ref()).await.unwrap_or_default();
            let mut ids = vec![selected_id];
            for cat in &cats {
                if cat.parent_id == Some(selected_id) {
                    ids.push(cat.id);
                }
            }
            ids
        }
    };

    let results = state
        .search
        .search(query, category_ids, page, per_page)
        .await?;

    Ok(Json(PagedResponse::new(
        results
            .hits
            .into_iter()
            .map(SearchHitResponse::from)
            .collect(),
        results.total,
        page,
        per_page,
    )))
}
