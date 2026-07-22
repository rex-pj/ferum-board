use std::collections::HashSet;

use axum::extract::{Extension, Multipart, Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use uuid::Uuid;
use validator::Validate;

use crate::app_state::AppState;
use crate::middleware::{AuthUser, AuthUserExt};
use crate::view_models::product::{
    parse_product_sort, parse_product_type, BrandResponse, MaterialResponse, ProductDetailResponse,
    ProductListQuery, ProductMediaResponse, ProductResponse, SubmitProductRequest,
};
use crate::view_models::{DataResponse, HandlerResult, PagedResponse};
use ferum_application::shared::AppError;
use ferum_domain::models::product::{NewProduct, ProductStatus};
use ferum_domain::repositories::product_repository::ProductListFilter;

/// GET /api/products — public catalog browse. Published products only, plus the
/// signed-in caller's own pending submissions (so they can still pick a product
/// they just proposed even after a page reload).
pub async fn list_products(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Query(q): Query<ProductListQuery>,
) -> HandlerResult<impl IntoResponse> {
    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(20).clamp(1, 100);

    let filter = ProductListFilter {
        product_type: q.product_type.as_deref().and_then(parse_product_type),
        // Public browse never exposes drafts or archived products…
        status: Some(ProductStatus::Published),
        brand_id: q.brand_id,
        category_id: q.category_id,
        material_id: q.material_id,
        query: q.q.clone(),
        sort: q.sort.as_deref().map(parse_product_sort).unwrap_or_default(),
        // …except the caller's own submissions.
        include_own: auth_user.as_ref().map(|u| u.id),
    };

    let (products, total) = state.product.list(filter, page, per_page).await?;
    Ok(Json(PagedResponse::new(
        products.into_iter().map(ProductResponse::from).collect(),
        total,
        page,
        per_page,
    )))
}

/// POST /api/products — member-submitted product. Created as `draft`, pending
/// admin approval (never appears in the public catalog until published).
pub async fn submit_product(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Json(body): Json<SubmitProductRequest>,
) -> HandlerResult<impl IntoResponse> {
    body.validate()
        .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;
    let user = auth_user.require_auth()?;

    let product_type = parse_product_type(&body.product_type)
        .ok_or_else(|| AppError::invalid("invalid_product_type"))?;

    let input = NewProduct {
        id: Uuid::new_v4(),
        slug: String::new(), // derived server-side from the name
        name: body.name,
        product_type,
        brand_id: body.brand_id,
        category_id: None,
        style: body.style,
        price_min: body.price_min,
        price_max: body.price_max,
        currency: "VND".to_string(),
        dimensions: serde_json::json!({}),
        origin: body.origin,
        description_md: body.description_md,
        created_by_id: Some(user.id),
        material_ids: body.material_ids,
    };

    let product = state.product.submit_product(user, input).await?;
    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(ProductResponse::from(product))),
    ))
}

/// POST /api/products/:id/media — attach a photo to a product.
///
/// A contributor may only upload to their own still-unapproved submission; the
/// use case enforces that. Curators may upload to anything, which keeps this the
/// single upload path for both the member form and the admin panel.
pub async fn upload_media(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
    mut multipart: Multipart,
) -> HandlerResult<impl IntoResponse> {
    let user = auth_user.require_auth()?;
    let (data, content_type) = crate::utils::read_image_field(&mut multipart, "image").await?;
    let media = state.product.upload_media(user, id, data, content_type).await?;
    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(ProductMediaResponse::from(media))),
    ))
}

/// DELETE /api/products/:id/media/:media_id — detach a photo the caller attached.
pub async fn delete_media(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path((_product_id, media_id)): Path<(Uuid, Uuid)>,
) -> HandlerResult<impl IntoResponse> {
    let user = auth_user.require_auth()?;
    state.product.delete_media(user, media_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/products/:slug — full public product view (materials + media + stats).
pub async fn get_product(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> HandlerResult<impl IntoResponse> {
    let product = state.product.get_by_slug(&slug).await?;
    // Unpublished products don't exist as far as the public is concerned.
    if product.status != ProductStatus::Published {
        return Err(AppError::NotFound.into());
    }
    let product_id = product.id;

    let material_ids: HashSet<Uuid> = state
        .product
        .list_material_ids(product_id)
        .await?
        .into_iter()
        .collect();
    let materials: Vec<MaterialResponse> = state
        .product
        .list_materials(None)
        .await?
        .into_iter()
        .filter(|m| material_ids.contains(&m.id))
        .map(MaterialResponse::from)
        .collect();

    let media = state
        .product
        .list_media(product_id)
        .await?
        .into_iter()
        .map(Into::into)
        .collect();

    let rating_stats = state.review.product_stats(product_id).await?.map(Into::into);

    let brand = match product.brand_id {
        Some(bid) => state.product.find_brand(bid).await?.map(BrandResponse::from),
        None => None,
    };

    Ok(Json(DataResponse::new(ProductDetailResponse {
        product: product.into(),
        brand,
        materials,
        media,
        rating_stats,
    })))
}

/// GET /api/brands — public brand directory (used by the catalog filter and the
/// new-thread product context).
pub async fn list_brands(State(state): State<AppState>) -> HandlerResult<impl IntoResponse> {
    let brands = state.product.list_brands().await?;
    Ok(Json(DataResponse::new(
        brands.into_iter().map(BrandResponse::from).collect::<Vec<_>>(),
    )))
}

#[derive(serde::Deserialize)]
pub struct MaterialListQuery {
    pub category: Option<String>,
}

/// GET /api/materials — reference material list (optionally by ?category=).
pub async fn list_materials(
    State(state): State<AppState>,
    Query(q): Query<MaterialListQuery>,
) -> HandlerResult<impl IntoResponse> {
    let materials = state.product.list_materials(q.category.as_deref()).await?;
    Ok(Json(DataResponse::new(
        materials
            .into_iter()
            .map(MaterialResponse::from)
            .collect::<Vec<_>>(),
    )))
}
