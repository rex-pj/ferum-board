use sea_orm::ConnectionTrait;
use sea_orm_migration::prelude::*;

use crate::m20260001_000005_create_threads::Threads;
use crate::m20260001_000025_create_products::Products;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000028_add_product_to_threads"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Threads::Table)
                    .add_column(ColumnDef::new(Threads::ProductId).uuid().null())
                    .to_owned(),
            )
            .await?;

        manager
            .create_foreign_key(
                ForeignKey::create()
                    .name("fk_threads_product_id")
                    .from(Threads::Table, Threads::ProductId)
                    .to(Products::Table, Products::Id)
                    .on_delete(ForeignKeyAction::SetNull)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_threads_product")
                    .table(Threads::Table)
                    .col(Threads::ProductId)
                    .to_owned(),
            )
            .await?;

        // One review per (product, author).
        //
        // `review_ratings` is keyed by thread_id, so a thread can only ever
        // carry one rating — but nothing stops one account opening ten threads
        // about the same product. `recompute_stats` counts every rating on
        // every non-deleted thread with that product_id and does not group by
        // author, so ten 5★ threads from one account would render as
        // "5.0 (10 reviews)". That is the whole rating system.
        //
        // Partial on `deleted_at IS NULL` so soft-deleting a review frees the
        // slot: a user who removes their review can write a new one. Partial on
        // `product_id IS NOT NULL` too, since ordinary (non-review) threads must
        // stay unconstrained — an author writes as many of those as they like.
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE UNIQUE INDEX uq_threads_product_author \
                 ON threads(product_id, author_id) \
                 WHERE product_id IS NOT NULL AND deleted_at IS NULL",
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP INDEX IF EXISTS uq_threads_product_author")
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .name("idx_threads_product")
                    .table(Threads::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_foreign_key(
                ForeignKey::drop()
                    .name("fk_threads_product_id")
                    .table(Threads::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Threads::Table)
                    .drop_column(Threads::ProductId)
                    .to_owned(),
            )
            .await
    }
}
