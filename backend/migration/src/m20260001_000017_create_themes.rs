use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000017_create_themes"
    }
}

#[derive(Iden)]
enum Themes {
    Table,
    Id,
    Slug,
    Name,
    Author,
    Version,
    Description,
    ParentSlug,
    IsSystem,
    IsActive,
    PreviewUrl,
    CreatedAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Themes::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Themes::Id)
                            .uuid()
                            .not_null()
                            .primary_key()
                            .extra("DEFAULT gen_random_uuid()"),
                    )
                    .col(ColumnDef::new(Themes::Slug).text().not_null().unique_key())
                    .col(ColumnDef::new(Themes::Name).text().not_null())
                    .col(ColumnDef::new(Themes::Author).text().null())
                    .col(
                        ColumnDef::new(Themes::Version)
                            .text()
                            .not_null()
                            .default("1.0.0"),
                    )
                    .col(ColumnDef::new(Themes::Description).text().null())
                    .col(
                        ColumnDef::new(Themes::ParentSlug)
                            .text()
                            .not_null()
                            .default("default"),
                    )
                    .col(
                        ColumnDef::new(Themes::IsSystem)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(Themes::IsActive)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(ColumnDef::new(Themes::PreviewUrl).text().null())
                    .col(
                        ColumnDef::new(Themes::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .extra("DEFAULT now()"),
                    )
                    .to_owned(),
            )
            .await?;

        // The built-in `default` theme row is seeded by PgSystemSeedService.
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Themes::Table).if_exists().to_owned())
            .await
    }
}
