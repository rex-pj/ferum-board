use sea_orm_migration::prelude::*;

use crate::m20260001_000002_create_users::Users;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000022_create_brands"
    }
}

#[derive(Iden)]
pub enum Brands {
    Table,
    Id,
    Slug,
    Name,
    Description,
    LogoUrl,
    Website,
    Country,
    IsVerified,
    OwnerUserId,
    Tier,
    CreatedAt,
    UpdatedAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Brands::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Brands::Id)
                            .uuid()
                            .not_null()
                            .primary_key()
                            .extra("DEFAULT gen_random_uuid()"),
                    )
                    .col(ColumnDef::new(Brands::Slug).text().not_null().unique_key())
                    .col(ColumnDef::new(Brands::Name).text().not_null())
                    .col(ColumnDef::new(Brands::Description).text().null())
                    .col(ColumnDef::new(Brands::LogoUrl).text().null())
                    .col(ColumnDef::new(Brands::Website).text().null())
                    .col(ColumnDef::new(Brands::Country).text().null())
                    .col(
                        ColumnDef::new(Brands::IsVerified)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(ColumnDef::new(Brands::OwnerUserId).uuid().null())
                    .col(
                        ColumnDef::new(Brands::Tier)
                            .text()
                            .not_null()
                            .default("free")
                            .check(Expr::col(Brands::Tier).is_in(["free", "sponsored"])),
                    )
                    .col(
                        ColumnDef::new(Brands::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .extra("DEFAULT now()"),
                    )
                    .col(
                        ColumnDef::new(Brands::UpdatedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_brands_owner_user_id")
                            .from(Brands::Table, Brands::OwnerUserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .check(Expr::cust("char_length(slug) BETWEEN 1 AND 160"))
                    .check(Expr::cust("char_length(name) BETWEEN 1 AND 200"))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_brands_owner")
                    .table(Brands::Table)
                    .col(Brands::OwnerUserId)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Brands::Table).if_exists().to_owned())
            .await
    }
}
