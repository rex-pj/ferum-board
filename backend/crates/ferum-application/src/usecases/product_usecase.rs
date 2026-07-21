use std::sync::Arc;

use uuid::Uuid;

use bytes::Bytes;

use crate::permission::PermissionChecker;
use crate::ports::{ForumJob, JobQueue};
use crate::shared::AppError;
use crate::storage_utils::{cas_key, validate_image_content_type};
use crate::validators::validate_image_magic;
use ferum_domain::models::brand::{Brand, NewBrand, UpdateBrand};
use ferum_domain::models::material::{Material, NewMaterial, UpdateMaterial};
use ferum_domain::models::product::{NewProduct, Product, ProductStatus};
use ferum_domain::models::product_media::{NewProductMedia, ProductMedia};
use ferum_domain::repositories::brand_repository::BrandRepository;
use ferum_domain::repositories::material_repository::MaterialRepository;
use ferum_domain::repositories::product_repository::{
    ProductListFilter, ProductListItem, ProductRepository, UpdateProduct,
};
use ferum_domain::repositories::stored_file_repository::StoredFileRepository;
use ferum_domain::AuthUser;

/// Product photos are capped smaller than thread thumbnails — catalog pages load
/// many at once.
const MAX_PRODUCT_IMAGE_BYTES: usize = 5 * 1024 * 1024;

pub struct ProductUseCase {
    pub products: Arc<dyn ProductRepository>,
    pub materials: Arc<dyn MaterialRepository>,
    pub brands: Arc<dyn BrandRepository>,
    pub stored_files: Arc<dyn StoredFileRepository>,
    pub jobs: Arc<dyn JobQueue>,
}

impl ProductUseCase {
    pub fn new(
        products: Arc<dyn ProductRepository>,
        materials: Arc<dyn MaterialRepository>,
        brands: Arc<dyn BrandRepository>,
        stored_files: Arc<dyn StoredFileRepository>,
        jobs: Arc<dyn JobQueue>,
    ) -> Self {
        Self { products, materials, brands, stored_files, jobs }
    }

    // ─── Materials ────────────────────────────────────────────────────────────

    pub async fn list_materials(&self, category: Option<&str>) -> Result<Vec<Material>, AppError> {
        self.materials.list(category).await
    }

    pub async fn create_material(
        &self,
        actor: &AuthUser,
        material: NewMaterial,
    ) -> Result<Material, AppError> {
        PermissionChecker::can_manage_products(actor)?;
        if material.name.trim().is_empty() {
            return Err(AppError::unprocessable("Material name cannot be empty."));
        }
        if self.materials.find_by_slug(&material.slug).await?.is_some() {
            return Err(AppError::Conflict("slug_taken".into()));
        }
        self.materials.create(material).await
    }

    pub async fn update_material(
        &self,
        actor: &AuthUser,
        id: Uuid,
        patch: UpdateMaterial,
    ) -> Result<Material, AppError> {
        PermissionChecker::can_manage_products(actor)?;
        if let Some(name) = &patch.name {
            if name.trim().is_empty() {
                return Err(AppError::unprocessable("Material name cannot be empty."));
            }
        }
        self.materials.update(id, patch).await
    }

    pub async fn delete_material(&self, actor: &AuthUser, id: Uuid) -> Result<(), AppError> {
        PermissionChecker::can_manage_products(actor)?;
        self.materials.delete(id).await
    }

    // ─── Brands ───────────────────────────────────────────────────────────────

    pub async fn list_brands(&self) -> Result<Vec<Brand>, AppError> {
        self.brands.list().await
    }

    pub async fn get_brand_by_slug(&self, slug: &str) -> Result<Brand, AppError> {
        self.brands.find_by_slug(slug).await?.ok_or(AppError::NotFound)
    }

    pub async fn find_brand(&self, id: Uuid) -> Result<Option<Brand>, AppError> {
        self.brands.find_by_id(id).await
    }

    pub async fn create_brand(&self, actor: &AuthUser, brand: NewBrand) -> Result<Brand, AppError> {
        PermissionChecker::can_manage_products(actor)?;
        if brand.name.trim().is_empty() {
            return Err(AppError::unprocessable("Brand name cannot be empty."));
        }
        if self.brands.find_by_slug(&brand.slug).await?.is_some() {
            return Err(AppError::Conflict("slug_taken".into()));
        }
        self.brands.create(brand).await
    }

    pub async fn update_brand(
        &self,
        actor: &AuthUser,
        id: Uuid,
        patch: UpdateBrand,
    ) -> Result<Brand, AppError> {
        PermissionChecker::can_manage_products(actor)?;
        if let Some(name) = &patch.name {
            if name.trim().is_empty() {
                return Err(AppError::unprocessable("Brand name cannot be empty."));
            }
        }
        self.brands.update(id, patch).await
    }

    pub async fn delete_brand(&self, actor: &AuthUser, id: Uuid) -> Result<(), AppError> {
        PermissionChecker::can_manage_products(actor)?;
        self.brands.delete(id).await
    }

    // ─── Reads (public) ───────────────────────────────────────────────────────

    pub async fn get_by_slug(&self, slug: &str) -> Result<Product, AppError> {
        self.products.find_by_slug(slug).await?.ok_or(AppError::NotFound)
    }

    pub async fn get_by_id(&self, id: Uuid) -> Result<Product, AppError> {
        self.products.find_by_id(id).await?.ok_or(AppError::NotFound)
    }

    pub async fn list(
        &self,
        filter: ProductListFilter,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<ProductListItem>, u64), AppError> {
        self.products.list(filter, page.max(1), per_page).await
    }

    pub async fn list_media(&self, product_id: Uuid) -> Result<Vec<ProductMedia>, AppError> {
        self.products.list_media(product_id).await
    }

    /// Batch: `review_thread_id → product cover image key` (published products
    /// only) — a thumbnail fallback for review cards. No N+1.
    pub async fn review_thumbnails(
        &self,
        thread_ids: &[Uuid],
    ) -> Result<std::collections::HashMap<Uuid, String>, AppError> {
        self.products.primary_image_by_threads(thread_ids).await
    }

    pub async fn list_material_ids(&self, product_id: Uuid) -> Result<Vec<Uuid>, AppError> {
        self.products.list_material_ids(product_id).await
    }

    // ─── Curation (requires product.manage) ───────────────────────────────────

    pub async fn create(&self, actor: &AuthUser, input: NewProduct) -> Result<Product, AppError> {
        PermissionChecker::can_manage_products(actor)?;
        if self.products.find_by_slug(&input.slug).await?.is_some() {
            return Err(AppError::Conflict("slug_taken".into()));
        }
        self.validated_create(input).await
    }

    /// Crowd-sourced submission. Any email-verified member may propose a product;
    /// it is stored as `draft` (the DB default — a submitter has no way to set
    /// `published`) and awaits admin approval. The slug is derived server-side
    /// from the name so contributors never have to craft (or collide on) one.
    pub async fn submit_product(
        &self,
        actor: &AuthUser,
        mut input: NewProduct,
    ) -> Result<Product, AppError> {
        PermissionChecker::can_submit_products(actor)?;
        input.slug = self.unique_slug(slug::slugify(&input.name)).await?;
        let product = self.validated_create(input).await?;
        // Curators (product.manage) skip the moderation queue: their submissions
        // publish immediately, matching the admin "new product" form. Everyone
        // else lands as `draft` for admin approval (the DB default).
        if PermissionChecker::can_manage_products(actor).is_ok() {
            return self
                .products
                .update(
                    product.id,
                    UpdateProduct {
                        status: Some(ProductStatus::Published),
                        ..Default::default()
                    },
                )
                .await;
        }
        Ok(product)
    }

    /// Shared tail of `create` / `submit_product`: field validation + insert.
    /// Permission and slug policy are the caller's responsibility.
    async fn validated_create(&self, input: NewProduct) -> Result<Product, AppError> {
        Self::validate(&input.name, input.price_min, input.price_max)?;
        self.products.create(input).await
    }

    /// Return `base` if free, otherwise `base-xxxxxx` with a short random suffix.
    async fn unique_slug(&self, base: String) -> Result<String, AppError> {
        let base = if base.is_empty() { "san-pham".to_string() } else { base };
        if self.products.find_by_slug(&base).await?.is_none() {
            return Ok(base);
        }
        for _ in 0..5 {
            let cand = format!("{base}-{}", &Uuid::new_v4().simple().to_string()[..6]);
            if self.products.find_by_slug(&cand).await?.is_none() {
                return Ok(cand);
            }
        }
        Err(AppError::Conflict("slug_taken".into()))
    }

    pub async fn update(
        &self,
        actor: &AuthUser,
        id: Uuid,
        patch: UpdateProduct,
    ) -> Result<Product, AppError> {
        PermissionChecker::can_manage_products(actor)?;
        if let Some(name) = &patch.name {
            if name.trim().is_empty() {
                return Err(AppError::unprocessable("Product name cannot be empty."));
            }
        }
        self.products.update(id, patch).await
    }

    pub async fn delete(&self, actor: &AuthUser, id: Uuid) -> Result<(), AppError> {
        PermissionChecker::can_manage_products(actor)?;
        self.products.delete(id).await
    }

    pub async fn set_materials(
        &self,
        actor: &AuthUser,
        product_id: Uuid,
        material_ids: &[Uuid],
    ) -> Result<(), AppError> {
        PermissionChecker::can_manage_products(actor)?;
        self.products.set_materials(product_id, material_ids).await
    }

    /// Store an uploaded image in the CAS blob store and attach it to the product.
    /// The first image on a product with no cover is promoted to `primary_image_key`.
    pub async fn upload_media(
        &self,
        actor: &AuthUser,
        product_id: Uuid,
        data: Bytes,
        content_type: String,
    ) -> Result<ProductMedia, AppError> {
        PermissionChecker::can_manage_products(actor)?;

        if !validate_image_content_type(&content_type) || !validate_image_magic(&data) {
            return Err(AppError::unprocessable(
                "Image must be JPEG, PNG, WebP, or GIF.",
            ));
        }
        if data.len() > MAX_PRODUCT_IMAGE_BYTES {
            return Err(AppError::unprocessable("Image exceeds the 5 MB size limit."));
        }

        let product = self
            .products
            .find_by_id(product_id)
            .await?
            .ok_or(AppError::NotFound)?;

        let key = cas_key("products", &data, &content_type);
        self.stored_files
            .upsert_and_ref(&key, &content_type, &data, data.len() as i64, Some(actor.id))
            .await?;

        let position = self.products.list_media(product_id).await?.len() as i32;
        let media = match self
            .products
            .add_media(NewProductMedia {
                id: Uuid::new_v4(),
                product_id,
                storage_key: key.clone(),
                kind: "photo".to_string(),
                caption: None,
                position,
            })
            .await
        {
            Ok(m) => m,
            Err(e) => {
                // Roll back the ref we just took so the blob isn't orphaned.
                self.release_key(&key).await;
                return Err(e);
            }
        };

        // Give the product a cover automatically if it has none.
        if product.primary_image_key.is_none() {
            let _ = self
                .products
                .update(
                    product_id,
                    UpdateProduct {
                        primary_image_key: Some(Some(key)),
                        ..Default::default()
                    },
                )
                .await;
        }

        Ok(media)
    }

    pub async fn delete_media(&self, actor: &AuthUser, media_id: Uuid) -> Result<(), AppError> {
        PermissionChecker::can_manage_products(actor)?;
        let media = self.products.find_media(media_id).await?;
        self.products.delete_media(media_id).await?;
        if let Some(m) = media {
            self.release_key(&m.storage_key).await;
        }
        Ok(())
    }

    /// Decrement the CAS ref-count for a key and enqueue GC when it hits zero.
    async fn release_key(&self, key: &str) {
        let remaining = self.stored_files.decrement_ref(key).await.unwrap_or(1);
        if remaining == 0 {
            let _ = self
                .jobs
                .enqueue(ForumJob::GcStorageKey { key: key.to_string() })
                .await;
        }
    }

    // ─── Helpers ──────────────────────────────────────────────────────────────

    fn validate(name: &str, price_min: Option<i32>, price_max: Option<i32>) -> Result<(), AppError> {
        if name.trim().is_empty() {
            return Err(AppError::unprocessable("Product name cannot be empty."));
        }
        if let (Some(min), Some(max)) = (price_min, price_max) {
            if max < min {
                return Err(AppError::unprocessable(
                    "Maximum price cannot be lower than the minimum price.",
                ));
            }
        }
        if price_min.map(|v| v < 0).unwrap_or(false) || price_max.map(|v| v < 0).unwrap_or(false) {
            return Err(AppError::unprocessable("Price cannot be negative."));
        }
        Ok(())
    }
}
