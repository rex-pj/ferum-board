use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000032_unique_review_per_author"
    }
}

// One review per (product, author).
//
// `review_ratings` is keyed by thread_id, so a thread can only ever carry one
// rating — but nothing stopped one account opening ten threads on the same
// product. `recompute_stats` counts every rating on every non-deleted thread
// with that product_id and does not group by author, so ten 5★ threads from one
// account rendered as "5.0 (10 reviews)". That is the whole rating system.
//
// The index is partial on `deleted_at IS NULL` so soft-deleting a review frees
// the slot: a user who removes their review can write a new one. It is also
// partial on `product_id IS NOT NULL`, since ordinary (non-review) threads must
// stay unconstrained — an author writes as many of those as they like.
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        // Existing duplicates would make the unique index fail to build, so they
        // are reconciled first. Nothing is deleted: the extra threads are simply
        // unlinked from the product (product_id = NULL), which drops them out of
        // the aggregate — recompute_stats only reads threads that still carry a
        // product_id — while preserving the thread, its posts and its rating row.
        // The earliest thread per (product, author) is kept; (created_at, id)
        // makes the choice deterministic when timestamps collide.
        conn.execute_unprepared(
            "UPDATE threads t \
             SET product_id = NULL \
             WHERE t.product_id IS NOT NULL \
               AND t.deleted_at IS NULL \
               AND EXISTS ( \
                   SELECT 1 FROM threads e \
                   WHERE e.product_id = t.product_id \
                     AND e.author_id  = t.author_id \
                     AND e.deleted_at IS NULL \
                     AND (e.created_at, e.id) < (t.created_at, t.id) \
               )",
        )
        .await?;

        conn.execute_unprepared(
            "CREATE UNIQUE INDEX uq_threads_product_author \
             ON threads(product_id, author_id) \
             WHERE product_id IS NOT NULL AND deleted_at IS NULL",
        )
        .await?;

        // Rebuild every aggregate row from scratch. Any product that lost a
        // duplicate above now has a stale review_count/average cached, and the
        // page reads this table directly rather than recomputing on render.
        // The projection mirrors ReviewRatingRepository::recompute_stats exactly.
        conn.execute_unprepared(
            "DELETE FROM product_rating_stats; \
             INSERT INTO product_rating_stats ( \
                 product_id, review_count, avg_overall, avg_durability, \
                 avg_materials, avg_comfort, avg_aesthetics, \
                 avg_value_for_money, updated_at \
             ) \
             SELECT t.product_id, \
                    count(*)::int, \
                    avg(r.overall), \
                    avg(r.durability), \
                    avg(r.materials), \
                    avg(r.comfort), \
                    avg(r.aesthetics), \
                    avg(r.value_for_money), \
                    now() \
             FROM threads t \
             JOIN review_ratings r ON r.thread_id = t.id \
             WHERE t.product_id IS NOT NULL AND t.deleted_at IS NULL \
             GROUP BY t.product_id",
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Only the index is reversible. Threads unlinked in `up` are not
        // re-linked: the original product_id is not recorded anywhere, and
        // guessing it would silently resurrect the duplicate ratings this
        // migration exists to remove.
        manager
            .get_connection()
            .execute_unprepared("DROP INDEX IF EXISTS uq_threads_product_author")
            .await?;

        Ok(())
    }
}
