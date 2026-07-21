use axum::extract::{Extension, Multipart, Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use uuid::Uuid;
use validator::Validate;

use crate::app_state::AppState;
use crate::middleware::{AuthUser, AuthUserExt};
use crate::view_models::product::{
    parse_product_sort, parse_product_status, parse_product_type, BrandResponse,
    CreateBrandRequest, CreateMaterialRequest, CreateProductRequest, MaterialResponse,
    ProductListQuery, ProductMediaResponse, ProductResponse, SetMaterialsRequest,
    UpdateBrandRequest, UpdateMaterialRequest, UpdateProductRequest,
};
use crate::view_models::{DataResponse, HandlerResult, PagedResponse};
use ferum_application::permission::PermissionChecker;
use ferum_application::shared::AppError;
use ferum_domain::models::brand::{NewBrand, UpdateBrand};
use ferum_domain::models::material::{NewMaterial, UpdateMaterial};
use ferum_domain::models::product::NewProduct;
use ferum_domain::repositories::product_repository::{ProductListFilter, UpdateProduct};

/// GET /api/admin/products — full catalog, any status (requires product.manage).
pub async fn list_products(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Query(q): Query<ProductListQuery>,
) -> HandlerResult<impl IntoResponse> {
    let user = auth_user.require_auth()?;
    PermissionChecker::can_manage_products(user)?;

    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(20).clamp(1, 100);
    let filter = ProductListFilter {
        product_type: q.product_type.as_deref().and_then(parse_product_type),
        status: q.status.as_deref().and_then(parse_product_status),
        brand_id: q.brand_id,
        category_id: q.category_id,
        material_id: q.material_id,
        query: q.q.clone(),
        sort: q.sort.as_deref().map(parse_product_sort).unwrap_or_default(),
        include_own: None,
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
        .ok_or_else(|| AppError::unprocessable("Invalid product type."))?;

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
        .map(|s| parse_product_status(s).ok_or_else(|| AppError::unprocessable("Invalid status.")))
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

// ─── Product media ─────────────────────────────────────────────────────────

/// GET /api/admin/products/:id/media — list a product's images (any status).
pub async fn list_media(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(id): Path<Uuid>,
) -> HandlerResult<impl IntoResponse> {
    let user = auth_user.require_auth()?;
    ferum_application::permission::PermissionChecker::can_manage_products(user)?;
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

    let mut file: Option<(bytes::Bytes, String)> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?
    {
        if field.name() == Some("image") {
            let ct = field
                .content_type()
                .unwrap_or("application/octet-stream")
                .to_string();
            let data = field
                .bytes()
                .await
                .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;
            file = Some((data, ct));
        }
    }

    let (data, content_type) =
        file.ok_or_else(|| AppError::unprocessable("No image field in the upload."))?;
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
