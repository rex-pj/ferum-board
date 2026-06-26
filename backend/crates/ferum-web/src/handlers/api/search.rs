use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::Json;

use crate::app_state::AppState;
use crate::view_models::search::{SearchHitResponse, SearchQuery};
use crate::view_models::{HandlerResult, PagedResponse};

pub async fn search(
    State(state): State<AppState>,
    Query(q): Query<SearchQuery>,
) -> HandlerResult<impl IntoResponse> {
    let query = q.q.unwrap_or_default();
    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(20).clamp(1, 50);

    let results = state
        .search
        .search(query, q.category_id, page, per_page)
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
