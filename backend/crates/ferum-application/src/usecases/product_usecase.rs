use std::collections::HashMap;
use std::sync::Arc;

use uuid::Uuid;

use bytes::Bytes;

use crate::constants::as_mb;
use crate::permission::PermissionChecker;
use crate::ports::{ForumJob, JobQueue, StorageService};
use crate::shared::AppError;
use crate::storage_utils::{cas_key, validate_image_content_type};
use crate::validators::validate_image_magic;
use ferum_domain::models::brand::{Brand, NewBrand, UpdateBrand};
use ferum_domain::models::material::{Material, NewMaterial, UpdateMaterial};
use ferum_domain::models::product::{NewProduct, Product, ProductStatus};
use ferum_domain::models::product_category::{
    NewProductCategory, ProductCategory, UpdateProductCategory,
};
use ferum_domain::models::product_media::{NewProductMedia, ProductMedia};
use ferum_domain::repositories::brand_repository::BrandRepository;
use ferum_domain::repositories::material_repository::MaterialRepository;
use ferum_domain::repositories::product_repository::{
    AutoAssignReport, ProductCategoryRepository, ProductDependents, ProductListFilter,
    ProductListItem, ProductRepository, ReviewedProduct, UpdateProduct,
};
use ferum_domain::repositories::stored_file_repository::StoredFileRepository;
use ferum_domain::AuthUser;

/// Product photos are capped smaller than thread thumbnails — catalog pages load
/// many at once.
const MAX_PRODUCT_IMAGE_BYTES: usize = 5 * 1024 * 1024;

/// Upper bound on images per product. Mostly an anti-abuse guard on the
/// crowd-sourced submission path, where any Basic member can attach photos.
const MAX_PRODUCT_MEDIA: usize = 12;

/// Fold keywords the way the matcher's SQL does: trimmed, lowercased, blanks
/// dropped, duplicates removed.
///
/// The SQL compares against `lower(f_unaccent(name))`, so a keyword stored as
/// "Ghế" could never match anything — it would sit in the table looking correct
/// and silently do nothing. Normalising on write means what an admin sees is
/// what the matcher uses. Accents are left for Postgres to fold, so the stored
/// value stays readable.
fn normalise_keyword_list(keywords: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(keywords.len());
    for k in keywords {
        let k = k.trim().to_lowercase();
        if !k.is_empty() && !out.contains(&k) {
            out.push(k);
        }
    }
    out
}

fn normalise_keywords(mut category: NewProductCategory) -> NewProductCategory {
    category.match_keywords = normalise_keyword_list(category.match_keywords);
    category
}

pub struct ProductUseCase {
    pub products: Arc<dyn ProductRepository>,
    pub categories: Arc<dyn ProductCategoryRepository>,
    pub materials: Arc<dyn MaterialRepository>,
    pub brands: Arc<dyn BrandRepository>,
    pub stored_files: Arc<dyn StoredFileRepository>,
    /// Blob bytes and the authority on file URL shape.
    pub storage: Arc<dyn StorageService>,
    pub jobs: Arc<dyn JobQueue>,
    /// `None` stores the uploaded bytes untouched — see `UserUseCase::images`.
    pub images: Option<Arc<crate::image_pipeline::ImagePipeline>>,
}

impl ProductUseCase {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        products: Arc<dyn ProductRepository>,
        categories: Arc<dyn ProductCategoryRepository>,
        materials: Arc<dyn MaterialRepository>,
        brands: Arc<dyn BrandRepository>,
        stored_files: Arc<dyn StoredFileRepository>,
        storage: Arc<dyn StorageService>,
        jobs: Arc<dyn JobQueue>,
    ) -> Self {
        Self { products, categories, materials, brands, stored_files, storage, jobs, images: None }
    }

    pub fn with_images(mut self, images: Arc<crate::image_pipeline::ImagePipeline>) -> Self {
        self.images = Some(images);
        self
    }

    // ─── Product categories ───────────────────────────────────────────────────
    //
    // The catalogue's own taxonomy (Sofa, Ghế, Bàn …), distinct from the forum's
    // discussion categories. Reads are public — the tree is a browse control on
    // every catalogue page — while every write is a curator action.

    pub async fn list_categories(&self) -> Result<Vec<ProductCategory>, AppError> {
        self.categories.list().await
    }

    /// Categories with how many products sit under each, plus the unfiled count
    /// under `None`. Admin-only: the unfiled number is a work queue, not
    /// something a reader needs.
    pub async fn category_counts(
        &self,
        actor: &AuthUser,
    ) -> Result<(Vec<ProductCategory>, HashMap<Option<Uuid>, u64>), AppError> {
        PermissionChecker::can_manage_products(actor)?;
        let (categories, counts) =
            tokio::try_join!(self.categories.list(), self.categories.product_counts())?;
        Ok((categories, counts))
    }

    pub async fn create_category(
        &self,
        actor: &AuthUser,
        category: NewProductCategory,
    ) -> Result<ProductCategory, AppError> {
        PermissionChecker::can_manage_products(actor)?;
        self.categories.create(normalise_keywords(category)).await
    }

    pub async fn update_category(
        &self,
        actor: &AuthUser,
        id: Uuid,
        mut patch: UpdateProductCategory,
    ) -> Result<ProductCategory, AppError> {
        PermissionChecker::can_manage_products(actor)?;

        // A category cannot be its own parent; the tree is two levels and a
        // cycle would make any recursive walk of it hang.
        if patch.parent_id == Some(Some(id)) {
            return Err(AppError::UnprocessableEntity(
                "category_cannot_be_its_own_parent".into(),
            ));
        }
        if let Some(keywords) = patch.match_keywords.take() {
            patch.match_keywords = Some(normalise_keyword_list(keywords));
        }
        self.categories.update(id, patch).await
    }

    pub async fn delete_category(&self, actor: &AuthUser, id: Uuid) -> Result<(), AppError> {
        PermissionChecker::can_manage_products(actor)?;
        self.categories.delete(id).await
    }

    /// Files every unfiled product the keyword matcher can identify.
    ///
    /// `dry_run` reports the blast radius without writing — the admin UI shows
    /// that first, because a bulk write over the whole catalogue is not
    /// something to trigger blind. Products no keyword matches are left unfiled
    /// rather than swept into a catch-all: an honest blank beats a wrong label,
    /// and the leftover count is the size of the manual queue.
    pub async fn auto_assign_categories(
        &self,
        actor: &AuthUser,
        dry_run: bool,
    ) -> Result<AutoAssignReport, AppError> {
        PermissionChecker::can_manage_products(actor)?;
        self.categories.auto_assign_categories(dry_run).await
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
            return Err(AppError::invalid("material_name_required"));
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
                return Err(AppError::invalid("material_name_required"));
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

    pub async fn find_brand(&self, id: Uuid) -> Result<Option<Brand>, AppError> {
        self.brands.find_by_id(id).await
    }

    pub async fn create_brand(&self, actor: &AuthUser, brand: NewBrand) -> Result<Brand, AppError> {
        PermissionChecker::can_manage_products(actor)?;
        if brand.name.trim().is_empty() {
            return Err(AppError::invalid("brand_name_required"));
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
                return Err(AppError::invalid("brand_name_required"));
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

    /// Batch: `review_thread_id → the product it reviews` (published products
    /// only) — gives review cards a thumbnail fallback and lets them name and
    /// link their subject. No N+1.
    pub async fn reviewed_products(
        &self,
        thread_ids: &[Uuid],
    ) -> Result<std::collections::HashMap<Uuid, ReviewedProduct>, AppError> {
        self.products.reviewed_product_by_threads(thread_ids).await
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
            let suffix: String = Uuid::new_v4().simple().to_string().chars().take(6).collect();
            let cand = format!("{base}-{suffix}");
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
        let product = self.products.find_by_id(id).await?.ok_or(AppError::NotFound)?;
        Self::authorize_product_edit(actor, &product)?;
        Self::reject_curator_only_fields(actor, &patch)?;
        if let Some(name) = &patch.name {
            if name.trim().is_empty() {
                return Err(AppError::invalid("product_name_required"));
            }
        }
        self.products.update(id, patch).await
    }

    /// What a hard delete of this product would affect. Drives the admin
    /// confirmation dialog so "Delete" is never a blind action.
    pub async fn dependents(
        &self,
        actor: &AuthUser,
        id: Uuid,
    ) -> Result<ProductDependents, AppError> {
        let product = self.products.find_by_id(id).await?.ok_or(AppError::NotFound)?;
        Self::authorize_product_edit(actor, &product)?;
        self.products.count_dependents(id).await
    }

    /// Hide a product from the catalog while keeping every review that points at
    /// it. This is the safe counterpart to `delete` and the only way to retire a
    /// product that has been reviewed.
    pub async fn archive(&self, actor: &AuthUser, id: Uuid) -> Result<Product, AppError> {
        self.update(
            actor,
            id,
            UpdateProduct {
                status: Some(ProductStatus::Archived),
                ..Default::default()
            },
        )
        .await
    }

    /// Permanently remove a product.
    ///
    /// Refused while review threads still reference it: `threads.product_id` is
    /// `ON DELETE SET NULL`, so deleting would leave every review in place but
    /// reviewing nothing — silent corruption rather than a visible error. Callers
    /// should `archive` instead. Media rows cascade in the database, so their CAS
    /// refs are released here first; otherwise the blobs leak forever.
    pub async fn delete(&self, actor: &AuthUser, id: Uuid) -> Result<(), AppError> {
        let product = self.products.find_by_id(id).await?.ok_or(AppError::NotFound)?;
        Self::authorize_product_edit(actor, &product)?;

        let dependents = self.products.count_dependents(id).await?;
        if dependents.blocks_hard_delete() {
            return Err(AppError::Conflict("product_has_reviews".into()));
        }

        let media = self.products.list_media(id).await?;
        self.products.delete(id).await?;
        for m in media {
            self.release_key(&m.storage_key).await;
        }
        Ok(())
    }

    pub async fn set_materials(
        &self,
        actor: &AuthUser,
        product_id: Uuid,
        material_ids: &[Uuid],
    ) -> Result<(), AppError> {
        let product = self
            .products
            .find_by_id(product_id)
            .await?
            .ok_or(AppError::NotFound)?;
        Self::authorize_product_edit(actor, &product)?;
        self.products.set_materials(product_id, material_ids).await
    }

    /// Who may edit a product. Curators anything; a contributor only their own
    /// submission, and only while it is still `draft` — once published, other
    /// people's reviews hang off it and it is shared catalogue content.
    ///
    /// One predicate for details, materials and images, so the three cannot
    /// disagree. Decides *whether*; `reject_curator_only_fields` decides *what*.
    fn authorize_product_edit(actor: &AuthUser, product: &Product) -> Result<(), AppError> {
        if PermissionChecker::can_manage_products(actor).is_ok() {
            return Ok(());
        }
        PermissionChecker::can_submit_products(actor)?;
        let own_draft =
            product.created_by_id == Some(actor.id) && product.status == ProductStatus::Draft;
        if own_draft {
            Ok(())
        } else {
            Err(AppError::forbidden("permission_denied"))
        }
    }

    /// Fields only a curator may set, refused for everyone else.
    ///
    /// `status` is the one that matters: without this, a contributor allowed to
    /// edit their own draft could PATCH `status = published` and approve their
    /// own submission, walking straight past the moderation queue. `category_id`
    /// is curation too — where an entry files in the catalogue is not the
    /// submitter's call.
    ///
    /// Refused loudly rather than silently dropped, so a caller that tries finds
    /// out instead of believing it worked.
    fn reject_curator_only_fields(actor: &AuthUser, patch: &UpdateProduct) -> Result<(), AppError> {
        if PermissionChecker::can_manage_products(actor).is_ok() {
            return Ok(());
        }
        if patch.status.is_some() || patch.category_id.is_some() {
            return Err(AppError::forbidden("permission_denied"));
        }
        Ok(())
    }

    /// Gate for reading the curator-only detail behind the edit form (a product's
    /// material ids and its full media list, draft or not). Same rule as editing:
    /// letting someone read the form they may not submit is only a slower refusal.
    pub async fn authorize_edit(&self, actor: &AuthUser, product_id: Uuid) -> Result<(), AppError> {
        let product = self
            .products
            .find_by_id(product_id)
            .await?
            .ok_or(AppError::NotFound)?;
        Self::authorize_product_edit(actor, &product)
    }

    /// Store an uploaded image in the CAS blob store and attach it to the product.
    /// The first image on a product with no cover is promoted to `primary_image_key`.
    pub async fn upload_media(
        &self,
        actor: &AuthUser,
        product_id: Uuid,
        data: Bytes,
        content_type: String,
        crop: Option<crate::ports::CropRect>,
    ) -> Result<ProductMedia, AppError> {
        let product = self
            .products
            .find_by_id(product_id)
            .await?
            .ok_or(AppError::NotFound)?;
        Self::authorize_product_edit(actor, &product)?;

        if !validate_image_content_type(&content_type) || !validate_image_magic(&data) {
            return Err(AppError::invalid("image_invalid_type"));
        }
        if data.len() > MAX_PRODUCT_IMAGE_BYTES {
            return Err(AppError::invalid_with("image_too_large", [("limit_mb", as_mb(MAX_PRODUCT_IMAGE_BYTES).into())]));
        }

        let existing = self.products.list_media(product_id).await?;
        if existing.len() >= MAX_PRODUCT_MEDIA {
            return Err(AppError::invalid_with(
                "product_media_limit",
                [("limit", MAX_PRODUCT_MEDIA.into())],
            ));
        }

        // A curator may crop deliberately; nothing crops on their behalf. The
        // catalogue grid is uniform by CSS, not by discarding pixels that might
        // be the leg of a chair somebody is trying to evaluate.
        let (data, content_type) = crate::image_pipeline::apply(
            self.images.as_ref(),
            data,
            content_type,
            crate::image_pipeline::ImageTarget::ProductMedia,
            crop,
        )
        .await?;

        let size = data.len() as i64;
        let key = cas_key("products", &data, &content_type);
        // Bytes first, then the row — see `UserUseCase::set_avatar`.
        self.storage.put(&key, data, &content_type).await?;
        self.stored_files
            .upsert_and_ref(&key, &content_type, size, Some(actor.id))
            .await?;

        let position = existing.len() as i32;
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
        let media = self
            .products
            .find_media(media_id)
            .await?
            .ok_or(AppError::NotFound)?;
        let product = self
            .products
            .find_by_id(media.product_id)
            .await?
            .ok_or(AppError::NotFound)?;
        Self::authorize_product_edit(actor, &product)?;

        self.products.delete_media(media_id).await?;

        // Removing the cover image would otherwise leave the product pointing at a
        // key whose ref-count just dropped: re-point it at the next image, or clear
        // it when none is left.
        if product.primary_image_key.as_deref() == Some(media.storage_key.as_str()) {
            let next = self
                .products
                .list_media(product.id)
                .await?
                .into_iter()
                .next()
                .map(|m| m.storage_key);
            let _ = self
                .products
                .update(
                    product.id,
                    UpdateProduct {
                        primary_image_key: Some(next),
                        ..Default::default()
                    },
                )
                .await;
        }

        self.release_key(&media.storage_key).await;
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
            return Err(AppError::invalid("product_name_required"));
        }
        if let (Some(min), Some(max)) = (price_min, price_max) {
            if max < min {
                return Err(AppError::invalid("price_range_inverted"));
            }
        }
        if price_min.map(|v| v < 0).unwrap_or(false) || price_max.map(|v| v < 0).unwrap_or(false) {
            return Err(AppError::invalid("price_negative"));
        }
        Ok(())
    }
}
