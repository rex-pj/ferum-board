use sea_orm_migration::prelude::*;

use crate::enums::{product_status::ProductStatusEnum, product_type::ProductTypeEnum};
use crate::m20260001_000002_create_users::Users;
use crate::m20260001_000022_create_brands::Brands;
use crate::m20260001_000024_create_product_categories::ProductCategories;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000025_create_products"
    }
}

#[derive(Iden)]
pub enum Products {
    Table,
    Id,
    Slug,
    Name,
    ProductType,
    Status,
    BrandId,
    CategoryId,
    Style,
    PriceMin,
    PriceMax,
    Currency,
    Dimensions,
    Origin,
    PrimaryImageKey,
    DescriptionMd,
    CreatedById,
    CreatedAt,
    UpdatedAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Products::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Products::Id)
                            .uuid()
                            .not_null()
                            .primary_key()
                            .extra("DEFAULT gen_random_uuid()"),
                    )
                    .col(
                        ColumnDef::new(Products::Slug)
                            .text()
                            .not_null()
                            .unique_key(),
                    )
                    .col(ColumnDef::new(Products::Name).text().not_null())
                    .col(
                        ColumnDef::new(Products::ProductType)
                            .custom(ProductTypeEnum::Type)
                            .not_null()
                            .extra("DEFAULT 'furniture'"),
                    )
                    .col(
                        ColumnDef::new(Products::Status)
                            .custom(ProductStatusEnum::Type)
                            .not_null()
                            .extra("DEFAULT 'draft'"),
                    )
                    .col(ColumnDef::new(Products::BrandId).uuid().null())
                    // The *catalogue* taxonomy (Sofa, Ghế, Bàn…), not the
                    // forum's tree of discussion topics. No forum category is a
                    // sensible home for a sofa, so pointing this at `categories`
                    // guaranteed the column stayed NULL and every filter built
                    // on it returned nothing.
                    .col(ColumnDef::new(Products::CategoryId).uuid().null())
                    .col(ColumnDef::new(Products::Style).text().null())
                    .col(ColumnDef::new(Products::PriceMin).integer().null())
                    .col(ColumnDef::new(Products::PriceMax).integer().null())
                    .col(
                        ColumnDef::new(Products::Currency)
                            .text()
                            .not_null()
                            .default("VND"),
                    )
                    .col(
                        ColumnDef::new(Products::Dimensions)
                            .json_binary()
                            .not_null()
                            .extra("DEFAULT '{}'"),
                    )
                    .col(ColumnDef::new(Products::Origin).text().null())
                    .col(ColumnDef::new(Products::PrimaryImageKey).text().null())
                    .col(ColumnDef::new(Products::DescriptionMd).text().null())
                    .col(ColumnDef::new(Products::CreatedById).uuid().null())
                    .col(
                        ColumnDef::new(Products::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .extra("DEFAULT now()"),
                    )
                    .col(
                        ColumnDef::new(Products::UpdatedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_products_brand_id")
                            .from(Products::Table, Products::BrandId)
                            .to(Brands::Table, Brands::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_products_product_category")
                            .from(Products::Table, Products::CategoryId)
                            .to(ProductCategories::Table, ProductCategories::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_products_created_by_id")
                            .from(Products::Table, Products::CreatedById)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .check(Expr::cust("char_length(slug) BETWEEN 1 AND 200"))
                    .check(Expr::cust("price_min IS NULL OR price_min >= 0"))
                    .check(Expr::cust(
                        "price_min IS NULL OR price_max IS NULL OR price_max >= price_min",
                    ))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_products_brand")
                    .table(Products::Table)
                    .col(Products::BrandId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_products_category")
                    .table(Products::Table)
                    .col(Products::CategoryId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_products_type_status")
                    .table(Products::Table)
                    .col(Products::ProductType)
                    .col(Products::Status)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Products::Table).if_exists().to_owned())
            .await
    }
}
