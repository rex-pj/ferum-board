use axum::extract::{Extension, Multipart, Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use uuid::Uuid;
use validator::Validate;

use crate::app_state::AppState;
use crate::middleware::{AuthUser, AuthUserExt};
use crate::view_models::product::{
    parse_product_sort, parse_product_status, parse_product_type, AutoAssignResponse,
    BrandResponse, CreateBrandRequest, CreateMaterialRequest, CreateProductCategoryRequest,
    CreateProductRequest, MaterialResponse, ProductCategoryResponse, ProductDependentsResponse,
    ProductListQuery, ProductMediaResponse, ProductResponse, SetMaterialsRequest,
    UpdateBrandRequest, UpdateMaterialRequest, UpdateProductCategoryRequest, UpdateProductRequest,
};
use crate::view_models::{DataResponse, HandlerResult, PagedResponse};
use ferum_application::permission::PermissionChecker;
use ferum_application::shared::AppError;
use ferum_domain::models::brand::{NewBrand, UpdateBrand};
use ferum_domain::models::material::{NewMaterial, UpdateMaterial};
use ferum_domain::models::product::NewProduct;
use ferum_domain::models::product_category::{NewProductCategory, UpdateProductCategory};
use ferum_domain::repositories::product_repository::{ProductListFilter, UpdateProduct};

/// GET /api/admin/products — full catalog, any status (requires product.manage).
pub async fn list_products(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Query(q): Query<ProductListQuery>,
) -> HandlerResult<impl IntoResponse> {
    let user = auth_user.require_auth()?;
    PermissionChecker::can_manage_products(user)?;

    let (page, per_page) = crate::utils::paginate(q.page, q.per_page, 20, 100);
    let filter = ProductListFilter {
        product_type: q.product_type.as_deref().and_then(parse_product_type),
        status: q.status.as_deref().and_then(parse_product_status),
        brand_id: q.brand_id,
        category: crate::utils::resolve_product_category_filter(&state, q.category_id.as_deref()).await,
        material_id: q.material_id,
        query: q.q.clone(),
        sort: q.sort.as_deref().map(parse_product_sort).unwrap_or_default(),
        include_own: None,
        // Admin listing must show the whole catalogue, including unreviewed rows.
        min_review_count: None,
    };

    let (products, total) = state.product.list(filter, page, per_page).await?;
    Ok(Json(PagedResponse::new(
        products.into_iter().map(ProductResponse::from).collect(),
        total,
        page,
        per_page,
    )))
}

pub async fn create_product(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Json(body): Json<CreateProductRequest>,
) -> HandlerResult<impl IntoResponse> {
    body.validate()
        .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;
    let user = auth_user.require_auth()?;

    let product_type = parse_product_type(&body.product_type)
        .ok_or_else(|| AppError::invalid("invalid_product_type"))?;

    let input = NewProduct {
        id: Uuid::new_v4(),
        slug: body.slug,
        name: body.name,
        product_type,
        brand_id: body.brand_id,
        category_id: body.category_id,
        style: body.style,
        price_min: body.price_min,
        price_max: body.price_max,
        currency: body.currency.unwrap_or_else(|| "VND".to_string()),
        dimensions: body.dimensions.unwrap_or_else(|| serde_json::json!({})),
        origin: body.origin,
        description_md: body.description_md,
        created_by_id: Some(user.id),
        material_ids: body.material_ids,
    };

    let product = state.product.create(user, input).await?;
    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(ProductResponse::from(product))),
    ))
}

/// GET /api/admin/products/:id — one product in any status. Backs the
/// `/admin/products?edit=<id>` deep link from the public product page, which
/// can't use the public endpoint (that one hides anything unpublished).
pub async fn get_product(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
) -> HandlerResult<impl IntoResponse> {
    let user = auth_user.require_auth()?;
    PermissionChecker::can_manage_products(user)?;
    let product = state.product.get_by_id(id).await?;
    Ok(Json(DataResponse::new(ProductResponse::from(product))))
}

pub async fn update_product(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateProductRequest>,
) -> HandlerResult<impl IntoResponse> {
    let user = auth_user.require_auth()?;

    let status = body
        .status
        .as_deref()
        .map(|s| parse_product_status(s).ok_or_else(|| AppError::invalid("invalid_status")))
        .transpose()?;

    let patch = UpdateProduct {
        name: body.name,
        status,
        brand_id: body.brand_id,
        category_id: body.category_id,
        style: body.style,
        price_min: body.price_min,
        price_max: body.price_max,
        currency: body.currency,
        dimensions: body.dimensions,
        origin: body.origin,
        primary_image_key: body.primary_image_key,
        description_md: body.description_md,
    };

    let product = state.product.update(user, id, patch).await?;
    Ok(Json(DataResponse::new(ProductResponse::from(product))))
}

pub async fn delete_product(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
) -> HandlerResult<impl IntoResponse> {
    let user = auth_user.require_auth()?;
    state.product.delete(user, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn set_materials(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
    Json(body): Json<SetMaterialsRequest>,
) -> HandlerResult<impl IntoResponse> {
    let user = auth_user.require_auth()?;
    state.product.set_materials(user, id, &body.material_ids).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/admin/products/:id/materials — the product's current material ids, so
/// the edit form can pre-fill the picker (and therefore express "no materials").
pub async fn list_product_materials(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
) -> HandlerResult<impl IntoResponse> {
    let user = auth_user.require_auth()?;
    // Not `can_manage_products`: a contributor editing their own unapproved draft
    // has to be able to read its current materials to pre-fill the form.
    state.product.authorize_edit(user, id).await?;
    let ids = state.product.list_material_ids(id).await?;
    Ok(Json(DataResponse::new(ids)))
}

/// GET /api/admin/products/:id/dependents — what a hard delete would affect.
/// The admin UI calls this before showing the delete confirmation.
pub async fn product_dependents(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
) -> HandlerResult<impl IntoResponse> {
    let user = auth_user.require_auth()?;
    let d = state.product.dependents(user, id).await?;
    Ok(Json(DataResponse::new(ProductDependentsResponse {
        reviews: d.reviews,
        media: d.media,
        materials: d.materials,
        can_hard_delete: !d.blocks_hard_delete(),
    })))
}

// ─── Product media ─────────────────────────────────────────────────────────

/// GET /api/admin/products/:id/media — list a product's images (any status).
pub async fn list_media(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
) -> HandlerResult<impl IntoResponse> {
    let user = auth_user.require_auth()?;
    // Same reasoning as `list_product_materials`: the owner of an unapproved
    // draft needs its gallery to manage the photos they are still allowed to change.
    state.product.authorize_edit(user, id).await?;
    let media = state.product.list_media(id).await?;
    Ok(Json(DataResponse::new(
        media.into_iter().map(ProductMediaResponse::from).collect::<Vec<_>>(),
    )))
}

/// POST /api/admin/products/:id/media — multipart image upload into the product's CAS.
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

/// DELETE /api/admin/products/media/:media_id — remove one image (releases its CAS ref).
pub async fn delete_media(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path((_product_id, media_id)): Path<(Uuid, Uuid)>,
) -> HandlerResult<impl IntoResponse> {
    let user = auth_user.require_auth()?;
    state.product.delete_media(user, media_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/admin/brands — create a brand (requires product.manage).
pub async fn create_brand(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Json(body): Json<CreateBrandRequest>,
) -> HandlerResult<impl IntoResponse> {
    body.validate()
        .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;
    let user = auth_user.require_auth()?;

    let brand = state
        .product
        .create_brand(
            user,
            NewBrand {
                id: Uuid::new_v4(),
                slug: body.slug,
                name: body.name,
                description: body.description,
                logo_url: None,
                website: body.website,
                country: body.country,
                is_verified: body.is_verified,
                owner_user_id: None,
            },
        )
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(BrandResponse::from(brand))),
    ))
}

/// POST /api/admin/materials — create a reference material.
pub async fn create_material(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Json(body): Json<CreateMaterialRequest>,
) -> HandlerResult<impl IntoResponse> {
    body.validate()
        .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;
    let user = auth_user.require_auth()?;

    let material = state
        .product
        .create_material(
            user,
            NewMaterial {
                id: Uuid::new_v4(),
                slug: body.slug,
                name: body.name,
                category: body.category,
                description: body.description,
            },
        )
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(MaterialResponse::from(material))),
    ))
}

/// PATCH /api/admin/materials/:id — edit a material (slug is immutable).
pub async fn update_material(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateMaterialRequest>,
) -> HandlerResult<impl IntoResponse> {
    body.validate()
        .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;
    let user = auth_user.require_auth()?;
    let material = state
        .product
        .update_material(
            user,
            id,
            UpdateMaterial {
                name: Some(body.name),
                category: Some(body.category),
                description: Some(body.description.filter(|d| !d.trim().is_empty())),
            },
        )
        .await?;
    Ok(Json(DataResponse::new(MaterialResponse::from(material))))
}

/// DELETE /api/admin/materials/:id — remove a material (product links cascade).
pub async fn delete_material(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
) -> HandlerResult<impl IntoResponse> {
    let user = auth_user.require_auth()?;
    state.product.delete_material(user, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// PATCH /api/admin/brands/:id — edit a brand (slug is immutable).
pub async fn update_brand(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateBrandRequest>,
) -> HandlerResult<impl IntoResponse> {
    body.validate()
        .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;
    let user = auth_user.require_auth()?;
    let brand = state
        .product
        .update_brand(
            user,
            id,
            UpdateBrand {
                name: Some(body.name),
                description: Some(body.description.filter(|d| !d.trim().is_empty())),
                website: Some(body.website.filter(|d| !d.trim().is_empty())),
                country: Some(body.country.filter(|d| !d.trim().is_empty())),
                is_verified: body.is_verified,
            },
        )
        .await?;
    Ok(Json(DataResponse::new(BrandResponse::from(brand))))
}

/// DELETE /api/admin/brands/:id — remove a brand (products' brand set to NULL).
pub async fn delete_brand(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
) -> HandlerResult<impl IntoResponse> {
    let user = auth_user.require_auth()?;
    state.product.delete_brand(user, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ─── Product categories ────────────────────────────────────────────────────────
//
// The catalogue's own taxonomy (Sofa, Ghế, Bàn …). Distinct from
// `/api/admin/categories`, which manages the forum's discussion tree — the two
// answer different questions and a sofa belongs in exactly one of them.

/// GET /api/admin/product-categories — the taxonomy with per-category product
/// counts, plus the unfiled count. Unfiled is the curator's work queue, so it
/// rides along rather than needing a second call.
pub async fn list_product_categories(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
) -> HandlerResult<impl IntoResponse> {
    let user = auth_user.require_auth()?;
    let (categories, counts) = state.product.category_counts(user).await?;

    let items: Vec<ProductCategoryResponse> = categories
        .into_iter()
        .map(|c| {
            let count = counts.get(&Some(c.id)).copied().unwrap_or(0);
            ProductCategoryResponse { product_count: count, ..c.into() }
        })
        .collect();

    Ok(Json(serde_json::json!({
        "data": items,
        "meta": { "unfiled": counts.get(&None).copied().unwrap_or(0) },
    })))
}

/// POST /api/admin/product-categories
pub async fn create_product_category(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Json(body): Json<CreateProductCategoryRequest>,
) -> HandlerResult<impl IntoResponse> {
    body.validate()
        .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;
    let user = auth_user.require_auth()?;

    let created = state
        .product
        .create_category(
            user,
            NewProductCategory {
                id: Uuid::new_v4(),
                slug: body.slug,
                name: body.name,
                parent_id: body.parent_id,
                position: body.position,
                icon: body.icon,
                match_keywords: body.match_keywords,
            },
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(ProductCategoryResponse::from(created))),
    ))
}

/// PATCH /api/admin/product-categories/:id — slug is immutable (stable key).
pub async fn update_product_category(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateProductCategoryRequest>,
) -> HandlerResult<impl IntoResponse> {
    let user = auth_user.require_auth()?;
    let updated = state
        .product
        .update_category(
            user,
            id,
            UpdateProductCategory {
                name: body.name,
                parent_id: body.parent_id,
                position: body.position,
                icon: body.icon,
                match_keywords: body.match_keywords,
            },
        )
        .await?;
    Ok(Json(DataResponse::new(ProductCategoryResponse::from(updated))))
}

/// DELETE /api/admin/product-categories/:id — products fall back to unfiled.
pub async fn delete_product_category(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
) -> HandlerResult<impl IntoResponse> {
    let user = auth_user.require_auth()?;
    state.product.delete_category(user, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/admin/product-categories/auto-assign — file every unfiled product
/// the keyword matcher can identify.
///
/// Defaults to a dry run. A bulk write across the whole catalogue is not
/// something to trigger by accident, so applying it takes an explicit
/// `?apply=1`; without it the caller gets the numbers and nothing changes.
pub async fn auto_assign_product_categories(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Query(q): Query<AutoAssignQuery>,
) -> HandlerResult<impl IntoResponse> {
    let user = auth_user.require_auth()?;
    let dry_run = q.apply.as_deref() != Some("1");
    let report = state.product.auto_assign_categories(user, dry_run).await?;

    Ok(Json(DataResponse::new(AutoAssignResponse {
        assigned: report.assigned,
        unmatched: report.unmatched,
        dry_run,
    })))
}

#[derive(serde::Deserialize)]
pub struct AutoAssignQuery {
    /// `"1"` applies the assignment; anything else (including absent) previews.
    pub apply: Option<String>,
}
