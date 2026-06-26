use sea_orm_migration::prelude::*;

use crate::m20260001_000004_create_categories::Categories;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000018_create_daily_stats"
    }
}

#[derive(Iden)]
enum DailyStats {
    Table,
    Id,
    Date,
    Metric,
    Value,
    CategoryId,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(DailyStats::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(DailyStats::Id)
                            .big_integer()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(DailyStats::Date).date().not_null())
                    .col(ColumnDef::new(DailyStats::Metric).text().not_null())
                    .col(
                        ColumnDef::new(DailyStats::Value)
                            .big_integer()
                            .not_null()
                            .default(0i64),
                    )
                    .col(ColumnDef::new(DailyStats::CategoryId).uuid().null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_daily_stats_category_id")
                            .from(DailyStats::Table, DailyStats::CategoryId)
                            .to(Categories::Table, Categories::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .get_connection()
            .execute_unprepared(
                "CREATE UNIQUE INDEX IF NOT EXISTS idx_daily_stats_global \
                     ON daily_stats (date, metric) \
                     WHERE category_id IS NULL; \
                 CREATE UNIQUE INDEX IF NOT EXISTS idx_daily_stats_category \
                     ON daily_stats (date, metric, category_id) \
                     WHERE category_id IS NOT NULL; \
                 CREATE INDEX IF NOT EXISTS idx_daily_stats_lookup \
                     ON daily_stats (metric, date DESC) \
                     WHERE category_id IS NULL;",
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(DailyStats::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}
