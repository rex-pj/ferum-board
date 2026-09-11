use async_trait::async_trait;
use chrono::Utc;
use rust_decimal::Decimal;
use sea_orm::sea_query::{Expr, OnConflict};
use sea_orm::*;
use uuid::Uuid;

use crate::entities::{product_rating_stats, review_ratings, threads};
use ferum_application::shared::AppError;
use ferum_domain::models::product_rating_stats::ProductRatingStats;
use ferum_domain::models::review_rating::{NewReviewRating, ReviewRating};
use ferum_domain::repositories::review_rating_repository::{
    ReviewRatingRepository, ReviewThreadRef,
};

pub struct PgReviewRatingRepository {
    db: DatabaseConnection,
}

impl PgReviewRatingRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

fn rating_to_domain(m: review_ratings::Model) -> ReviewRating {
    ReviewRating {
        thread_id: m.thread_id,
        overall: m.overall,
        durability: m.durability,
        materials: m.materials,
        comfort: m.comfort,
        aesthetics: m.aesthetics,
        value_for_money: m.value_for_money,
        verified_purchase: m.verified_purchase,
        created_at: m.created_at.with_timezone(&Utc),
        updated_at: m.updated_at.map(|t| t.with_timezone(&Utc)),
    }
}

fn stats_to_domain(m: product_rating_stats::Model) -> ProductRatingStats {
    ProductRatingStats {
        product_id: m.product_id,
        review_count: m.review_count,
        avg_overall: m.avg_overall,
        avg_durability: m.avg_durability,
        avg_materials: m.avg_materials,
        avg_comfort: m.avg_comfort,
        avg_aesthetics: m.avg_aesthetics,
        avg_value_for_money: m.avg_value_for_money,
        updated_at: m.updated_at.with_timezone(&Utc),
    }
}

/// Running sum + count for one rating dimension, yielding an exact 2-dp average.
#[derive(Default)]
struct Avg {
    sum: i64,
    count: i64,
}

impl Avg {
    fn add(&mut self, v: Option<i16>) {
        if let Some(n) = v {
            self.sum += n as i64;
            self.count += 1;
        }
    }

    fn value(&self) -> Option<Decimal> {
        if self.count == 0 {
            return None;
        }
        Some((Decimal::from(self.sum) / Decimal::from(self.count)).round_dp(2))
    }
}

#[async_trait]
impl ReviewRatingRepository for PgReviewRatingRepository {
    async fn thread_ref(&self, thread_id: Uuid) -> Result<Option<ReviewThreadRef>, AppError> {
        Ok(threads::Entity::find_by_id(thread_id)
            .one(&self.db)
            .await?
            .map(|t| ReviewThreadRef {
                author_id: t.author_id,
                product_id: t.product_id,
            }))
    }

    async fn upsert(&self, rating: NewReviewRating) -> Result<ReviewRating, AppError> {
        let model = review_ratings::ActiveModel {
            thread_id: Set(rating.thread_id),
            overall: Set(rating.overall),
            durability: Set(rating.durability),
            materials: Set(rating.materials),
            comfort: Set(rating.comfort),
            aesthetics: Set(rating.aesthetics),
            value_for_money: Set(rating.value_for_money),
            verified_purchase: Set(rating.verified_purchase),
            ..Default::default()
        };

        review_ratings::Entity::insert(model)
            .on_conflict(
                OnConflict::column(review_ratings::Column::ThreadId)
                    .update_columns([
                        review_ratings::Column::Overall,
                        review_ratings::Column::Durability,
                        review_ratings::Column::Materials,
                        review_ratings::Column::Comfort,
                        review_ratings::Column::Aesthetics,
                        review_ratings::Column::ValueForMoney,
                        review_ratings::Column::VerifiedPurchase,
                    ])
                    .value(review_ratings::Column::UpdatedAt, Expr::current_timestamp())
                    .to_owned(),
            )
            .exec(&self.db)
            .await?;

        self.find_by_thread(rating.thread_id)
            .await?
            .ok_or_else(|| AppError::internal("review rating vanished after upsert"))
    }

    async fn find_by_thread(&self, thread_id: Uuid) -> Result<Option<ReviewRating>, AppError> {
        Ok(review_ratings::Entity::find_by_id(thread_id)
            .one(&self.db)
            .await?
            .map(rating_to_domain))
    }

    async fn find_by_threads(
        &self,
        thread_ids: &[Uuid],
    ) -> Result<std::collections::HashMap<Uuid, ReviewRating>, AppError> {
        if thread_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        Ok(review_ratings::Entity::find()
            .filter(review_ratings::Column::ThreadId.is_in(thread_ids.to_vec()))
            .all(&self.db)
            .await?
            .into_iter()
            .map(|m| (m.thread_id, rating_to_domain(m)))
            .collect())
    }

    async fn delete(&self, thread_id: Uuid) -> Result<(), AppError> {
        review_ratings::Entity::delete_by_id(thread_id)
            .exec(&self.db)
            .await?;
        Ok(())
    }

    async fn recompute_stats(&self, product_id: Uuid) -> Result<ProductRatingStats, AppError> {
        // Threads that review this product.
        let thread_ids: Vec<Uuid> = threads::Entity::find()
            .filter(threads::Column::ProductId.eq(product_id))
            .filter(threads::Column::DeletedAt.is_null())
            .all(&self.db)
            .await?
            .into_iter()
            .map(|t| t.id)
            .collect();

        let ratings = if thread_ids.is_empty() {
            Vec::new()
        } else {
            review_ratings::Entity::find()
                .filter(review_ratings::Column::ThreadId.is_in(thread_ids))
                .all(&self.db)
                .await?
        };

        let mut overall = Avg::default();
        let mut durability = Avg::default();
        let mut materials = Avg::default();
        let mut comfort = Avg::default();
        let mut aesthetics = Avg::default();
        let mut value = Avg::default();
        for r in &ratings {
            overall.add(Some(r.overall));
            durability.add(r.durability);
            materials.add(r.materials);
            comfort.add(r.comfort);
            aesthetics.add(r.aesthetics);
            value.add(r.value_for_money);
        }

        let now = Utc::now().fixed_offset();
        let row = product_rating_stats::ActiveModel {
            product_id: Set(product_id),
            review_count: Set(ratings.len() as i32),
            avg_overall: Set(overall.value()),
            avg_durability: Set(durability.value()),
            avg_materials: Set(materials.value()),
            avg_comfort: Set(comfort.value()),
            avg_aesthetics: Set(aesthetics.value()),
            avg_value_for_money: Set(value.value()),
            updated_at: Set(now),
        };

        product_rating_stats::Entity::insert(row)
            .on_conflict(
                OnConflict::column(product_rating_stats::Column::ProductId)
                    .update_columns([
                        product_rating_stats::Column::ReviewCount,
                        product_rating_stats::Column::AvgOverall,
                        product_rating_stats::Column::AvgDurability,
                        product_rating_stats::Column::AvgMaterials,
                        product_rating_stats::Column::AvgComfort,
                        product_rating_stats::Column::AvgAesthetics,
                        product_rating_stats::Column::AvgValueForMoney,
                        product_rating_stats::Column::UpdatedAt,
                    ])
                    .to_owned(),
            )
            .exec(&self.db)
            .await?;

        self.find_stats(product_id)
            .await?
            .ok_or_else(|| AppError::internal("rating stats vanished after upsert"))
    }

    async fn find_stats(&self, product_id: Uuid) -> Result<Option<ProductRatingStats>, AppError> {
        Ok(product_rating_stats::Entity::find_by_id(product_id)
            .one(&self.db)
            .await?
            .map(stats_to_domain))
    }

    async fn global_stats(&self) -> Result<(i64, Option<Decimal>), AppError> {
        // One aggregate pass: COUNT(*) and SUM(overall). The exact average is then
        // derived as Decimal (ColumnTrait exposes count/sum but not avg).
        #[derive(FromQueryResult, Default)]
        struct Agg {
            cnt: i64,
            total: Option<i64>,
        }
        let row = review_ratings::Entity::find()
            .select_only()
            .column_as(review_ratings::Column::ThreadId.count(), "cnt")
            .column_as(review_ratings::Column::Overall.sum(), "total")
            .into_model::<Agg>()
            .one(&self.db)
            .await?
            .unwrap_or_default();

        let avg = if row.cnt > 0 {
            row.total
                .map(|t| (Decimal::from(t) / Decimal::from(row.cnt)).round_dp(2))
        } else {
            None
        };
        Ok((row.cnt, avg))
    }

    async fn rating_distribution(&self, product_id: Uuid) -> Result<[i32; 5], AppError> {
        // Threads reviewing this product.
        let thread_ids: Vec<Uuid> = threads::Entity::find()
            .filter(threads::Column::ProductId.eq(product_id))
            .filter(threads::Column::DeletedAt.is_null())
            .all(&self.db)
            .await?
            .into_iter()
            .map(|t| t.id)
            .collect();

        let mut dist = [0i32; 5];
        if thread_ids.is_empty() {
            return Ok(dist);
        }

        // GROUP BY overall — the DB does the counting, no rows shipped.
        let rows: Vec<(i16, i64)> = review_ratings::Entity::find()
            .select_only()
            .column(review_ratings::Column::Overall)
            .column_as(review_ratings::Column::ThreadId.count(), "cnt")
            .filter(review_ratings::Column::ThreadId.is_in(thread_ids))
            .group_by(review_ratings::Column::Overall)
            .into_tuple()
            .all(&self.db)
            .await?;

        for (overall, cnt) in rows {
            // `get_mut` subsumes the 1..=5 range check: `dist` has five slots, so
            // a rating outside the range is skipped rather than panicking on a
            // row the DB constraint failed to reject.
            if let Some(slot) = usize::try_from(overall - 1)
                .ok()
                .and_then(|i| dist.get_mut(i))
            {
                *slot = cnt as i32;
            }
        }
        Ok(dist)
    }
}
