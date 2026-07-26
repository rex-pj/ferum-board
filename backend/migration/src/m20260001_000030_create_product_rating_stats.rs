use sea_orm_migration::prelude::*;

use crate::m20260001_000025_create_products::Products;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000030_create_product_rating_stats"
    }
}

// Aggregate-first, anonymized rollup of review_ratings per product. Holds no
// personal data — the row people/partners are allowed to read for market data.
// Recomputed by the application (InlineJobRunner) on review create/update,
// mirroring the plain-table approach of `daily_stats`.
#[derive(Iden)]
pub enum ProductRatingStats {
    Table,
    ProductId,
    ReviewCount,
    AvgOverall,
    AvgDurability,
    AvgMaterials,
    AvgComfort,
    AvgAesthetics,
    AvgValueForMoney,
    UpdatedAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(ProductRatingStats::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(ProductRatingStats::ProductId)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(ProductRatingStats::ReviewCount)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(ColumnDef::new(ProductRatingStats::AvgOverall).decimal_len(4, 2).null())
                    .col(
                        ColumnDef::new(ProductRatingStats::AvgDurability)
                            .decimal_len(4, 2)
                            .null(),
                    )
                    .col(
                        ColumnDef::new(ProductRatingStats::AvgMaterials)
                            .decimal_len(4, 2)
                            .null(),
                    )
                    .col(ColumnDef::new(ProductRatingStats::AvgComfort).decimal_len(4, 2).null())
                    .col(
                        ColumnDef::new(ProductRatingStats::AvgAesthetics)
                            .decimal_len(4, 2)
                            .null(),
                    )
                    .col(
                        ColumnDef::new(ProductRatingStats::AvgValueForMoney)
                            .decimal_len(4, 2)
                            .null(),
                    )
                    .col(
                        ColumnDef::new(ProductRatingStats::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .extra("DEFAULT now()"),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_product_rating_stats_product_id")
                            .from(ProductRatingStats::Table, ProductRatingStats::ProductId)
                            .to(Products::Table, Products::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(ProductRatingStats::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}
