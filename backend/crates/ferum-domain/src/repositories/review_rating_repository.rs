use async_trait::async_trait;
use rust_decimal::Decimal;
use std::collections::HashMap;
use uuid::Uuid;

use crate::models::product_rating_stats::ProductRatingStats;
use crate::models::review_rating::{NewReviewRating, ReviewRating};
use crate::AppError;

/// The author and linked product of a thread — enough to authorize a rating
/// submission and route the stats recompute to the right product.
#[derive(Clone, Debug)]
pub struct ReviewThreadRef {
    pub author_id: Uuid,
    pub product_id: Option<Uuid>,
}

#[async_trait]
pub trait ReviewRatingRepository: Send + Sync {
    async fn thread_ref(&self, thread_id: Uuid) -> Result<Option<ReviewThreadRef>, AppError>;

    /// Insert or replace the rating for a review thread (PK = thread_id).
    async fn upsert(&self, rating: NewReviewRating) -> Result<ReviewRating, AppError>;
    async fn find_by_thread(&self, thread_id: Uuid) -> Result<Option<ReviewRating>, AppError>;
    /// Batch-load ratings for many review threads, keyed by thread_id.
    async fn find_by_threads(
        &self,
        thread_ids: &[Uuid],
    ) -> Result<HashMap<Uuid, ReviewRating>, AppError>;
    async fn delete(&self, thread_id: Uuid) -> Result<(), AppError>;

    /// Recompute and persist `product_rating_stats` for one product from every
    /// rating attached to that product's review threads. Returns the fresh row.
    async fn recompute_stats(&self, product_id: Uuid) -> Result<ProductRatingStats, AppError>;
    async fn find_stats(&self, product_id: Uuid) -> Result<Option<ProductRatingStats>, AppError>;

    /// Platform-wide aggregate: total number of ratings and the average overall
    /// score across every review. Powers the homepage market-data band.
    async fn global_stats(&self) -> Result<(i64, Option<Decimal>), AppError>;

    /// Count of reviews per overall star value for a product, as `[n1, n2, n3, n4, n5]`
    /// (index 0 = one-star). Powers the rating-distribution histogram.
    async fn rating_distribution(&self, product_id: Uuid) -> Result<[i32; 5], AppError>;
}
