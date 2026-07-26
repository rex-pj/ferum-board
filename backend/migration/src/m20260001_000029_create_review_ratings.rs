use sea_orm_migration::prelude::*;

use crate::m20260001_000005_create_threads::Threads;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000029_create_review_ratings"
    }
}

#[derive(Iden)]
pub enum ReviewRatings {
    Table,
    ThreadId,
    Overall,
    Durability,
    Materials,
    Comfort,
    Aesthetics,
    ValueForMoney,
    VerifiedPurchase,
    CreatedAt,
    UpdatedAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(ReviewRatings::Table)
                    .if_not_exists()
                    // One structured rating per review thread.
                    .col(
                        ColumnDef::new(ReviewRatings::ThreadId)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(ReviewRatings::Overall)
                            .small_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(ReviewRatings::Durability).small_integer().null())
                    .col(ColumnDef::new(ReviewRatings::Materials).small_integer().null())
                    .col(ColumnDef::new(ReviewRatings::Comfort).small_integer().null())
                    .col(ColumnDef::new(ReviewRatings::Aesthetics).small_integer().null())
                    .col(
                        ColumnDef::new(ReviewRatings::ValueForMoney)
                            .small_integer()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(ReviewRatings::VerifiedPurchase)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(ReviewRatings::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .extra("DEFAULT now()"),
                    )
                    .col(
                        ColumnDef::new(ReviewRatings::UpdatedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_review_ratings_thread_id")
                            .from(ReviewRatings::Table, ReviewRatings::ThreadId)
                            .to(Threads::Table, Threads::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .check(Expr::cust("overall BETWEEN 1 AND 5"))
                    .check(Expr::cust("durability IS NULL OR durability BETWEEN 1 AND 5"))
                    .check(Expr::cust("materials IS NULL OR materials BETWEEN 1 AND 5"))
                    .check(Expr::cust("comfort IS NULL OR comfort BETWEEN 1 AND 5"))
                    .check(Expr::cust("aesthetics IS NULL OR aesthetics BETWEEN 1 AND 5"))
                    .check(Expr::cust(
                        "value_for_money IS NULL OR value_for_money BETWEEN 1 AND 5",
                    ))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(ReviewRatings::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}
