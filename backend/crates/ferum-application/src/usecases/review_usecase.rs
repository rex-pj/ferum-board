use std::collections::HashMap;
use std::sync::Arc;

use rust_decimal::prelude::ToPrimitive;
use uuid::Uuid;

use crate::permission::PermissionChecker;
use crate::shared::AppError;
use ferum_domain::models::product_rating_stats::ProductRatingStats;
use ferum_domain::models::review_rating::{NewReviewRating, ReviewRating};
use ferum_domain::repositories::review_rating_repository::ReviewRatingRepository;
use ferum_domain::AuthUser;

/// A review is an ordinary thread linked to a product, plus a structured rating.
/// This use case owns the rating side; thread creation stays in `ThreadUseCase`.
pub struct ReviewUseCase {
    pub reviews: Arc<dyn ReviewRatingRepository>,
}

impl ReviewUseCase {
    pub fn new(reviews: Arc<dyn ReviewRatingRepository>) -> Self {
        Self { reviews }
    }

    /// Attach (or replace) the actor's rating on their own review thread, then
    /// refresh the product's aggregate stats.
    pub async fn submit_rating(
        &self,
        actor: &AuthUser,
        thread_id: Uuid,
        mut rating: NewReviewRating,
    ) -> Result<ReviewRating, AppError> {
        PermissionChecker::require_not_banned(actor)?;

        let reference = self
            .reviews
            .thread_ref(thread_id)
            .await?
            .ok_or(AppError::NotFound)?;

        let product_id = reference
            .product_id
            .ok_or_else(|| AppError::invalid("thread_not_review"))?;

        if reference.author_id != actor.id {
            return Err(AppError::forbidden("not_author"));
        }

        rating.thread_id = thread_id;
        if !rating.scores_in_range() {
            return Err(AppError::invalid_with(
                "rating_out_of_range",
                [
                    ("min", ferum_domain::models::review_rating::MIN_RATING.into()),
                    ("max", ferum_domain::models::review_rating::MAX_RATING.into()),
                ],
            ));
        }

        let saved = self.reviews.upsert(rating).await?;
        self.reviews.recompute_stats(product_id).await?;
        Ok(saved)
    }

    pub async fn get_rating(&self, thread_id: Uuid) -> Result<Option<ReviewRating>, AppError> {
        self.reviews.find_by_thread(thread_id).await
    }

    /// The product this thread reviews (from the thread's own `product_id`), if any.
    pub async fn product_for_thread(&self, thread_id: Uuid) -> Result<Option<Uuid>, AppError> {
        Ok(self
            .reviews
            .thread_ref(thread_id)
            .await?
            .and_then(|r| r.product_id))
    }

    /// Batch-load ratings for a set of review threads, keyed by thread_id.
    pub async fn ratings_for_threads(
        &self,
        thread_ids: &[Uuid],
    ) -> Result<HashMap<Uuid, ReviewRating>, AppError> {
        self.reviews.find_by_threads(thread_ids).await
    }

    pub async fn delete_rating(&self, actor: &AuthUser, thread_id: Uuid) -> Result<(), AppError> {
        PermissionChecker::require_not_banned(actor)?;

        let reference = self
            .reviews
            .thread_ref(thread_id)
            .await?
            .ok_or(AppError::NotFound)?;
        if reference.author_id != actor.id {
            return Err(AppError::forbidden("not_author"));
        }

        self.reviews.delete(thread_id).await?;
        if let Some(product_id) = reference.product_id {
            self.reviews.recompute_stats(product_id).await?;
        }
        Ok(())
    }

    /// Aggregate, anonymized rating stats for a product — the read the market-data
    /// layer builds on.
    pub async fn product_stats(
        &self,
        product_id: Uuid,
    ) -> Result<Option<ProductRatingStats>, AppError> {
        self.reviews.find_stats(product_id).await
    }

    /// Platform-wide (total reviews, average overall) for the homepage band.
    pub async fn global_stats(&self) -> Result<(i64, Option<f64>), AppError> {
        let (count, avg) = self.reviews.global_stats().await?;
        Ok((count, avg.and_then(|d| d.to_f64())))
    }

    /// Per-star review counts `[n1..n5]` for a product's distribution histogram.
    pub async fn rating_distribution(&self, product_id: Uuid) -> Result<[i32; 5], AppError> {
        self.reviews.rating_distribution(product_id).await
    }
}
