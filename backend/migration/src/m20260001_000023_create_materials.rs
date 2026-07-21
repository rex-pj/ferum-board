use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000023_create_materials"
    }
}

#[derive(Iden)]
pub enum Materials {
    Table,
    Id,
    Slug,
    Name,
    Category,
    Description,
    CreatedAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Materials::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Materials::Id)
                            .uuid()
                            .not_null()
                            .primary_key()
                            .extra("DEFAULT gen_random_uuid()"),
                    )
                    .col(
                        ColumnDef::new(Materials::Slug)
                            .text()
                            .not_null()
                            .unique_key(),
                    )
                    .col(ColumnDef::new(Materials::Name).text().not_null())
                    .col(
                        ColumnDef::new(Materials::Category).text().not_null().check(
                            Expr::col(Materials::Category).is_in([
                                "wood_natural",
                                "wood_engineered",
                                "rattan_bamboo",
                                "metal",
                                "fabric",
                                "leather",
                                "stone",
                                "glass",
                                "plastic",
                                "other",
                            ]),
                        ),
                    )
                    .col(ColumnDef::new(Materials::Description).text().null())
                    .col(
                        ColumnDef::new(Materials::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .extra("DEFAULT now()"),
                    )
                    .check(Expr::cust("char_length(slug) BETWEEN 1 AND 120"))
                    .check(Expr::cust("char_length(name) BETWEEN 1 AND 120"))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_materials_category")
                    .table(Materials::Table)
                    .col(Materials::Category)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Materials::Table).if_exists().to_owned())
            .await
    }
}
