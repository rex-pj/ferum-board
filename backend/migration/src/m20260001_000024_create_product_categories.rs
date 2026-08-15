use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000024_create_product_categories"
    }
}

// A catalogue taxonomy, separate from forum categories — `products.category_id`
// once pointed at the forum tree, where no category is a sensible home for a
// sofa, so the column stayed NULL and every filter on it returned nothing.
//
// `match_keywords` drives auto-filing: Vietnamese furniture names lead with the
// type ("Sofa da Milano"), so the name is real evidence. Data-driven, so an
// admin can teach it a word without a code change.
//
// Starter rows are seeded by PgSystemSeedService; the matcher lives once, in
// PgProductCategoryRepository.
#[derive(Iden)]
pub enum ProductCategories {
    Table,
    Id,
    Slug,
    Name,
    ParentId,
    Position,
    Icon,
    MatchKeywords,
    CreatedAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(ProductCategories::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(ProductCategories::Id)
                            .uuid()
                            .not_null()
                            .primary_key()
                            .extra("DEFAULT gen_random_uuid()"),
                    )
                    .col(
                        ColumnDef::new(ProductCategories::Slug)
                            .text()
                            .not_null()
                            .unique_key(),
                    )
                    .col(ColumnDef::new(ProductCategories::Name).text().not_null())
                    // Two levels, like the forum tree, so "Sofa" can later gain
                    // "Sofa góc" / "Sofa băng" without a migration.
                    .col(ColumnDef::new(ProductCategories::ParentId).uuid().null())
                    .col(
                        ColumnDef::new(ProductCategories::Position)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(ColumnDef::new(ProductCategories::Icon).text().null())
                    // Drives the auto-assign matcher. Editable from the admin UI,
                    // so teaching it a new word is a data change, not a deploy.
                    .col(
                        ColumnDef::new(ProductCategories::MatchKeywords)
                            .array(ColumnType::Text)
                            .not_null()
                            .extra("DEFAULT '{}'"),
                    )
                    .col(
                        ColumnDef::new(ProductCategories::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .extra("DEFAULT now()"),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(ProductCategories::Table, ProductCategories::ParentId)
                            .to(ProductCategories::Table, ProductCategories::Id)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .check(Expr::cust("char_length(slug) BETWEEN 1 AND 120"))
                    .check(Expr::cust("char_length(name) BETWEEN 1 AND 120"))
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(ProductCategories::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}
