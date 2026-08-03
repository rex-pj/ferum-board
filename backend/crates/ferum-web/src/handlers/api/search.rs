use axum::extract::{Extension, Query, State};
use axum::response::IntoResponse;
use axum::Json;

use crate::app_state::AppState;
use crate::middleware::AuthUser;
use crate::view_models::product::parse_product_type;
use crate::view_models::search::{SearchQuery, SearchResponse};
use crate::view_models::HandlerResult;
use ferum_application::ports::{ProductFacets, ProductSearchSort, ThreadSearchSort};
use ferum_application::usecases::search_usecase::{SearchRequest, SearchScope};

pub async fn search(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Query(q): Query<SearchQuery>,
) -> HandlerResult<impl IntoResponse> {
    let query = q.q.unwrap_or_default();
    // Capped below the SSR page's own limit: this endpoint backs typeahead,
    // where a caller asking for 50 rows per keystroke is a mistake, not a need.
    let (page, per_page) = crate::utils::paginate(q.page, q.per_page, 20, 30)?;
    let scope = q.tab.as_deref().map(SearchScope::parse).unwrap_or_default();

    // Category visibility is resolved inside the use case from `auth_user`, so
    // a caller cannot widen it by naming a category it may not see.
    let facets = ProductFacets {
        product_type: q.product_type.as_deref().and_then(parse_product_type),
        brand_id: q.brand_id,
        material_id: q.material_id,
        sort: q.psort.as_deref().map(ProductSearchSort::parse).unwrap_or_default(),
    };

    let outcome = state
        .search
        .search_all(
            auth_user.as_ref(),
            SearchRequest {
                q: query,
                scope,
                category_id: q.category_id,
                product_category_ids: crate::utils::resolve_product_category_ids(
                    &state,
                    q.pcat.as_deref(),
                )
                .await,
                facets,
                thread_sort: q.tsort.as_deref().map(ThreadSearchSort::parse).unwrap_or_default(),
                page,
                per_page,
            },
        )
        .await?;

    Ok(Json(SearchResponse::from(outcome)))
}
