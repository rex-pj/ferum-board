use std::collections::HashMap;

use async_trait::async_trait;
use chrono::Utc;
use sea_orm::{DbBackend, FromQueryResult, Statement};
use sea_orm::*;
use uuid::Uuid;

use crate::entities::product_categories;
use ferum_application::shared::AppError;
use ferum_domain::models::product_category::{
    NewProductCategory, ProductCategory, UpdateProductCategory,
};
use ferum_domain::repositories::product_repository::{
    AutoAssignReport, ProductCategoryRepository,
};

pub struct PgProductCategoryRepository {
    db: DatabaseConnection,
}

impl PgProductCategoryRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

fn to_domain(m: product_categories::Model) -> ProductCategory {
    ProductCategory {
        id: m.id,
        slug: m.slug,
        name: m.name,
        parent_id: m.parent_id,
        position: m.position,
        icon: m.icon,
        match_keywords: m.match_keywords,
        created_at: m.created_at.with_timezone(&Utc),
    }
}

/// The matcher, as one SQL predicate.
///
/// Whole-word (`\m…\M`) against the unaccented, lowercased product name. Word
/// boundaries are the whole point: a substring match files "Bàn phím"
/// (keyboard) under "Bàn" (table), and "den" would hit any name that merely
/// contains those three letters.
const MATCH_PREDICATE: &str = "EXISTS ( \
     SELECT 1 FROM unnest(pc.match_keywords) AS kw \
     WHERE lower(f_unaccent(pr.name)) ~ ('\\m' || kw || '\\M') \
 )";

/// Best match per product: longest keyword first, so a specific keyword beats a
/// generic one ("ghe sofa" → Sofa, not Ghế); category position breaks ties so
/// the result is deterministic.
fn best_match_subquery() -> String {
    format!(
        "SELECT DISTINCT ON (pr.id) pr.id AS product_id, pc.id AS category_id \
         FROM products pr \
         JOIN product_categories pc ON {MATCH_PREDICATE} \
         WHERE pr.category_id IS NULL \
         ORDER BY pr.id, ( \
             SELECT max(length(kw)) FROM unnest(pc.match_keywords) AS kw \
             WHERE lower(f_unaccent(pr.name)) ~ ('\\m' || kw || '\\M') \
         ) DESC, pc.position ASC"
    )
}

#[derive(Debug, FromQueryResult)]
struct CountRow {
    n: i64,
}

#[derive(Debug, FromQueryResult)]
struct GroupCountRow {
    category_id: Option<Uuid>,
    n: i64,
}

#[async_trait]
impl ProductCategoryRepository for PgProductCategoryRepository {
    async fn list(&self) -> Result<Vec<ProductCategory>, AppError> {
        Ok(product_categories::Entity::find()
            .order_by_asc(product_categories::Column::Position)
            .order_by_asc(product_categories::Column::Name)
            .all(&self.db)
            .await?
            .into_iter()
            .map(to_domain)
            .collect())
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<ProductCategory>, AppError> {
        Ok(product_categories::Entity::find_by_id(id)
            .one(&self.db)
            .await?
            .map(to_domain))
    }

    async fn create(&self, category: NewProductCategory) -> Result<ProductCategory, AppError> {
        let model = product_categories::ActiveModel {
            id: Set(category.id),
            slug: Set(category.slug),
            name: Set(category.name),
            parent_id: Set(category.parent_id),
            position: Set(category.position),
            icon: Set(category.icon),
            match_keywords: Set(category.match_keywords),
            ..Default::default()
        };
        Ok(to_domain(model.insert(&self.db).await?))
    }

    async fn update(
        &self,
        id: Uuid,
        patch: UpdateProductCategory,
    ) -> Result<ProductCategory, AppError> {
        let mut model: product_categories::ActiveModel = product_categories::Entity::find_by_id(id)
            .one(&self.db)
            .await?
            .ok_or(AppError::NotFound)?
            .into();

        if let Some(name) = patch.name {
            model.name = Set(name);
        }
        if let Some(parent_id) = patch.parent_id {
            model.parent_id = Set(parent_id);
        }
        if let Some(position) = patch.position {
            model.position = Set(position);
        }
        if let Some(icon) = patch.icon {
            model.icon = Set(icon);
        }
        if let Some(keywords) = patch.match_keywords {
            model.match_keywords = Set(keywords);
        }

        Ok(to_domain(model.update(&self.db).await?))
    }

    async fn delete(&self, id: Uuid) -> Result<(), AppError> {
        product_categories::Entity::delete_by_id(id).exec(&self.db).await?;
        Ok(())
    }

    async fn product_counts(&self) -> Result<HashMap<Option<Uuid>, u64>, AppError> {
        // Grouped in one pass, including the NULL bucket — that bucket is the
        // remaining manual queue and is the number an admin most needs to see.
        let rows = GroupCountRow::find_by_statement(Statement::from_string(
            DbBackend::Postgres,
            "SELECT category_id, COUNT(*)::BIGINT AS n FROM products GROUP BY category_id"
                .to_owned(),
        ))
        .all(&self.db)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| (r.category_id, u64::try_from(r.n).unwrap_or(0)))
            .collect())
    }

    async fn auto_assign_categories(&self, dry_run: bool) -> Result<AutoAssignReport, AppError> {
        let matched = CountRow::find_by_statement(Statement::from_string(
            DbBackend::Postgres,
            format!("SELECT COUNT(*)::BIGINT AS n FROM ({}) m", best_match_subquery()),
        ))
        .one(&self.db)
        .await?
        .map(|r| u64::try_from(r.n).unwrap_or(0))
        .unwrap_or(0);

        let unfiled = CountRow::find_by_statement(Statement::from_string(
            DbBackend::Postgres,
            "SELECT COUNT(*)::BIGINT AS n FROM products WHERE category_id IS NULL".to_owned(),
        ))
        .one(&self.db)
        .await?
        .map(|r| u64::try_from(r.n).unwrap_or(0))
        .unwrap_or(0);

        if !dry_run {
            self.db
                .execute_raw(Statement::from_string(
                    DbBackend::Postgres,
                    format!(
                        "UPDATE products p SET category_id = m.category_id \
                         FROM ({}) m WHERE p.id = m.product_id",
                        best_match_subquery()
                    ),
                ))
                .await?;
        }

        Ok(AutoAssignReport {
            assigned: matched,
            unmatched: unfiled.saturating_sub(matched),
        })
    }
}
