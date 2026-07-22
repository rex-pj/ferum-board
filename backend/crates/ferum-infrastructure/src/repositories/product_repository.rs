use std::collections::HashMap;

use async_trait::async_trait;
use chrono::Utc;
use rust_decimal::Decimal;
use sea_orm::sea_query::extension::postgres::PgExpr;
use sea_orm::sea_query::{Expr, NullOrdering, Query};
use sea_orm::*;
use uuid::Uuid;

use crate::entities::{product_materials, product_media, product_rating_stats, products, threads};
use ferum_application::shared::AppError;
use ferum_domain::models::product::{NewProduct, Product, ProductStatus, ProductType};
use ferum_domain::models::product_media::{NewProductMedia, ProductMedia};
use ferum_domain::repositories::product_repository::{
    ProductDependents, ProductListFilter, ProductListItem, ProductRepository, ProductSort,
    UpdateProduct,
};

pub struct PgProductRepository {
    db: DatabaseConnection,
}

impl PgProductRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

// ── enum mapping (domain ↔ entity) ─────────────────────────────────────────────

fn type_to_entity(t: ProductType) -> products::ProductType {
    match t {
        ProductType::Furniture => products::ProductType::Furniture,
        ProductType::Material => products::ProductType::Material,
        ProductType::Room => products::ProductType::Room,
    }
}

fn type_to_domain(t: products::ProductType) -> ProductType {
    match t {
        products::ProductType::Furniture => ProductType::Furniture,
        products::ProductType::Material => ProductType::Material,
        products::ProductType::Room => ProductType::Room,
    }
}

fn status_to_entity(s: ProductStatus) -> products::ProductStatus {
    match s {
        ProductStatus::Draft => products::ProductStatus::Draft,
        ProductStatus::Published => products::ProductStatus::Published,
        ProductStatus::Archived => products::ProductStatus::Archived,
    }
}

fn status_to_domain(s: products::ProductStatus) -> ProductStatus {
    match s {
        products::ProductStatus::Draft => ProductStatus::Draft,
        products::ProductStatus::Published => ProductStatus::Published,
        products::ProductStatus::Archived => ProductStatus::Archived,
    }
}

fn to_domain(m: products::Model) -> Product {
    Product {
        id: m.id,
        slug: m.slug,
        name: m.name,
        product_type: type_to_domain(m.product_type),
        status: status_to_domain(m.status),
        brand_id: m.brand_id,
        category_id: m.category_id,
        style: m.style,
        price_min: m.price_min,
        price_max: m.price_max,
        currency: m.currency,
        dimensions: m.dimensions,
        origin: m.origin,
        primary_image_key: m.primary_image_key,
        description_md: m.description_md,
        created_by_id: m.created_by_id,
        created_at: m.created_at.with_timezone(&Utc),
    }
}

fn media_to_domain(m: product_media::Model) -> ProductMedia {
    ProductMedia {
        id: m.id,
        product_id: m.product_id,
        storage_key: m.storage_key,
        kind: m.kind,
        caption: m.caption,
        position: m.position,
        created_at: m.created_at.with_timezone(&Utc),
    }
}

#[async_trait]
impl ProductRepository for PgProductRepository {
    async fn create(&self, product: NewProduct) -> Result<Product, AppError> {
        let model = products::ActiveModel {
            id: Set(product.id),
            slug: Set(product.slug),
            name: Set(product.name),
            product_type: Set(type_to_entity(product.product_type)),
            brand_id: Set(product.brand_id),
            category_id: Set(product.category_id),
            style: Set(product.style),
            price_min: Set(product.price_min),
            price_max: Set(product.price_max),
            currency: Set(product.currency),
            dimensions: Set(product.dimensions),
            origin: Set(product.origin),
            description_md: Set(product.description_md),
            created_by_id: Set(product.created_by_id),
            ..Default::default()
        };
        let created = to_domain(model.insert(&self.db).await?);
        if !product.material_ids.is_empty() {
            self.set_materials(created.id, &product.material_ids).await?;
        }
        Ok(created)
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<Product>, AppError> {
        Ok(products::Entity::find_by_id(id)
            .one(&self.db)
            .await?
            .map(to_domain))
    }

    async fn find_by_slug<'a>(&self, slug: &'a str) -> Result<Option<Product>, AppError> {
        Ok(products::Entity::find()
            .filter(products::Column::Slug.eq(slug))
            .one(&self.db)
            .await?
            .map(to_domain))
    }

    async fn list(
        &self,
        filter: ProductListFilter,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<ProductListItem>, u64), AppError> {
        let mut q = products::Entity::find();

        if let Some(t) = filter.product_type {
            q = q.filter(products::Column::ProductType.eq(type_to_entity(t)));
        }
        match (filter.status, filter.include_own) {
            // Public list for a signed-in user: everything published OR their own
            // (draft/archived) submissions.
            (Some(s), Some(uid)) => {
                q = q.filter(
                    Condition::any()
                        .add(products::Column::Status.eq(status_to_entity(s)))
                        .add(products::Column::CreatedById.eq(uid)),
                );
            }
            (Some(s), None) => {
                q = q.filter(products::Column::Status.eq(status_to_entity(s)));
            }
            (None, _) => {}
        }
        if let Some(brand_id) = filter.brand_id {
            q = q.filter(products::Column::BrandId.eq(brand_id));
        }
        if let Some(category_id) = filter.category_id {
            q = q.filter(products::Column::CategoryId.eq(category_id));
        }
        if let Some(material_id) = filter.material_id {
            q = q.filter(
                products::Column::Id.in_subquery(
                    Query::select()
                        .column(product_materials::Column::ProductId)
                        .from(product_materials::Entity)
                        .and_where(product_materials::Column::MaterialId.eq(material_id))
                        .to_owned(),
                ),
            );
        }
        if let Some(text) = filter.query.filter(|s| !s.trim().is_empty()) {
            // Case-insensitive substring match (ILIKE) — matches the user-search
            // idiom; plain LIKE would be case-sensitive on Postgres.
            let pattern = format!("%{}%", text.trim());
            q = q.filter(Expr::col(products::Column::Name).ilike(pattern));
        }

        let per_page = per_page.clamp(1, 100);
        // Count the filtered set before joining the stats table (a LEFT JOIN on a
        // 1:1 rollup can't multiply rows, but counting pre-join keeps it cheap).
        let total = q.clone().count(&self.db).await?;

        // Rating-based sorts LEFT JOIN the aggregate rollup and order by it with
        // unrated products (NULL) pushed last, then newest as a stable tiebreak.
        let stats_rel = product_rating_stats::Relation::Product.def().rev();
        let q = match filter.sort {
            ProductSort::Newest => q.order_by_desc(products::Column::CreatedAt),
            ProductSort::TopRated => q
                .join(JoinType::LeftJoin, stats_rel)
                .order_by_with_nulls(
                    product_rating_stats::Column::AvgOverall,
                    Order::Desc,
                    NullOrdering::Last,
                )
                .order_by_desc(products::Column::CreatedAt),
            ProductSort::MostReviewed => q
                .join(JoinType::LeftJoin, stats_rel)
                .order_by_with_nulls(
                    product_rating_stats::Column::ReviewCount,
                    Order::Desc,
                    NullOrdering::Last,
                )
                .order_by_desc(products::Column::CreatedAt),
        };

        let models = q
            .paginate(&self.db, per_page)
            .fetch_page(page.saturating_sub(1))
            .await?;

        // Fetch aggregate ratings for just this page in one query (no N+1).
        let ids: Vec<Uuid> = models.iter().map(|m| m.id).collect();
        let mut stats: HashMap<Uuid, (i32, Option<Decimal>)> = HashMap::new();
        if !ids.is_empty() {
            for s in product_rating_stats::Entity::find()
                .filter(product_rating_stats::Column::ProductId.is_in(ids))
                .all(&self.db)
                .await?
            {
                stats.insert(s.product_id, (s.review_count, s.avg_overall));
            }
        }

        let items = models
            .into_iter()
            .map(|m| {
                let (review_count, avg_overall) =
                    stats.get(&m.id).cloned().unwrap_or((0, None));
                ProductListItem {
                    product: to_domain(m),
                    review_count,
                    avg_overall,
                }
            })
            .collect();
        Ok((items, total))
    }

    async fn update(&self, id: Uuid, patch: UpdateProduct) -> Result<Product, AppError> {
        let model = products::Entity::find_by_id(id)
            .one(&self.db)
            .await?
            .ok_or(AppError::NotFound)?;
        let mut active: products::ActiveModel = model.into();

        if let Some(v) = patch.name {
            active.name = Set(v);
        }
        if let Some(v) = patch.status {
            active.status = Set(status_to_entity(v));
        }
        if let Some(v) = patch.brand_id {
            active.brand_id = Set(v);
        }
        if let Some(v) = patch.category_id {
            active.category_id = Set(v);
        }
        if let Some(v) = patch.style {
            active.style = Set(v);
        }
        if let Some(v) = patch.price_min {
            active.price_min = Set(v);
        }
        if let Some(v) = patch.price_max {
            active.price_max = Set(v);
        }
        if let Some(v) = patch.currency {
            active.currency = Set(v);
        }
        if let Some(v) = patch.dimensions {
            active.dimensions = Set(v);
        }
        if let Some(v) = patch.origin {
            active.origin = Set(v);
        }
        if let Some(v) = patch.primary_image_key {
            active.primary_image_key = Set(v);
        }
        if let Some(v) = patch.description_md {
            active.description_md = Set(v);
        }
        active.updated_at = Set(Some(Utc::now().into()));

        Ok(to_domain(active.update(&self.db).await?))
    }

    async fn delete(&self, id: Uuid) -> Result<(), AppError> {
        products::Entity::delete_by_id(id).exec(&self.db).await?;
        Ok(())
    }

    async fn count_dependents(&self, product_id: Uuid) -> Result<ProductDependents, AppError> {
        Ok(ProductDependents {
            // Soft-deleted reviews carry nothing worth protecting, so they never
            // block a hard delete.
            reviews: threads::Entity::find()
                .filter(threads::Column::ProductId.eq(product_id))
                .filter(threads::Column::Status.ne(threads::ThreadStatus::Deleted))
                .count(&self.db)
                .await?,
            media: product_media::Entity::find()
                .filter(product_media::Column::ProductId.eq(product_id))
                .count(&self.db)
                .await?,
            materials: product_materials::Entity::find()
                .filter(product_materials::Column::ProductId.eq(product_id))
                .count(&self.db)
                .await?,
        })
    }

    async fn set_materials(
        &self,
        product_id: Uuid,
        material_ids: &[Uuid],
    ) -> Result<(), AppError> {
        product_materials::Entity::delete_many()
            .filter(product_materials::Column::ProductId.eq(product_id))
            .exec(&self.db)
            .await?;
        if material_ids.is_empty() {
            return Ok(());
        }
        let rows: Vec<product_materials::ActiveModel> = material_ids
            .iter()
            .map(|&material_id| product_materials::ActiveModel {
                product_id: Set(product_id),
                material_id: Set(material_id),
            })
            .collect();
        product_materials::Entity::insert_many(rows)
            .on_conflict(
                sea_query::OnConflict::columns([
                    product_materials::Column::ProductId,
                    product_materials::Column::MaterialId,
                ])
                .do_nothing()
                .to_owned(),
            )
            .exec(&self.db)
            .await?;
        Ok(())
    }

    async fn list_material_ids(&self, product_id: Uuid) -> Result<Vec<Uuid>, AppError> {
        Ok(product_materials::Entity::find()
            .filter(product_materials::Column::ProductId.eq(product_id))
            .all(&self.db)
            .await?
            .into_iter()
            .map(|r| r.material_id)
            .collect())
    }

    async fn primary_image_by_threads(
        &self,
        thread_ids: &[Uuid],
    ) -> Result<HashMap<Uuid, String>, AppError> {
        if thread_ids.is_empty() {
            return Ok(HashMap::new());
        }
        #[derive(FromQueryResult)]
        struct Row {
            thread_id: Uuid,
            key: String,
        }
        let rows = threads::Entity::find()
            .select_only()
            .column_as(threads::Column::Id, "thread_id")
            .column_as(products::Column::PrimaryImageKey, "key")
            .join(JoinType::InnerJoin, threads::Relation::Product.def())
            .filter(threads::Column::Id.is_in(thread_ids.to_vec()))
            .filter(products::Column::Status.eq(products::ProductStatus::Published))
            .filter(products::Column::PrimaryImageKey.is_not_null())
            .into_model::<Row>()
            .all(&self.db)
            .await?;
        Ok(rows.into_iter().map(|r| (r.thread_id, r.key)).collect())
    }

    async fn add_media(&self, media: NewProductMedia) -> Result<ProductMedia, AppError> {
        let model = product_media::ActiveModel {
            id: Set(media.id),
            product_id: Set(media.product_id),
            storage_key: Set(media.storage_key),
            kind: Set(media.kind),
            caption: Set(media.caption),
            position: Set(media.position),
            ..Default::default()
        };
        Ok(media_to_domain(model.insert(&self.db).await?))
    }

    async fn list_media(&self, product_id: Uuid) -> Result<Vec<ProductMedia>, AppError> {
        Ok(product_media::Entity::find()
            .filter(product_media::Column::ProductId.eq(product_id))
            .order_by_asc(product_media::Column::Position)
            .order_by_asc(product_media::Column::CreatedAt)
            .all(&self.db)
            .await?
            .into_iter()
            .map(media_to_domain)
            .collect())
    }

    async fn find_media(&self, media_id: Uuid) -> Result<Option<ProductMedia>, AppError> {
        Ok(product_media::Entity::find_by_id(media_id)
            .one(&self.db)
            .await?
            .map(media_to_domain))
    }

    async fn delete_media(&self, media_id: Uuid) -> Result<(), AppError> {
        product_media::Entity::delete_by_id(media_id)
            .exec(&self.db)
            .await?;
        Ok(())
    }
}
