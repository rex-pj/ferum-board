use sea_orm_migration::prelude::*;

use crate::m20260001_000025_create_products::Products;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000027_create_product_media"
    }
}

#[derive(Iden)]
pub enum ProductMedia {
    Table,
    Id,
    ProductId,
    // CAS key; soft reference into stored_files.key (ref-counting handled at repository
    // layer, consistent with thread_thumbnails / user_avatars — no hard FK).
    StorageKey,
    Kind,
    Caption,
    Position,
    CreatedAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(ProductMedia::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(ProductMedia::Id)
                            .uuid()
                            .not_null()
                            .primary_key()
                            .extra("DEFAULT gen_random_uuid()"),
                    )
                    .col(ColumnDef::new(ProductMedia::ProductId).uuid().not_null())
                    .col(ColumnDef::new(ProductMedia::StorageKey).text().not_null())
                    .col(
                        ColumnDef::new(ProductMedia::Kind)
                            .text()
                            .not_null()
                            .default("photo")
                            .check(
                                Expr::col(ProductMedia::Kind)
                                    .is_in(["photo", "before", "after", "detail"]),
                            ),
                    )
                    .col(ColumnDef::new(ProductMedia::Caption).text().null())
                    .col(
                        ColumnDef::new(ProductMedia::Position)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(ProductMedia::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .extra("DEFAULT now()"),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_product_media_product_id")
                            .from(ProductMedia::Table, ProductMedia::ProductId)
                            .to(Products::Table, Products::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_product_media_product")
                    .table(ProductMedia::Table)
                    .col(ProductMedia::ProductId)
                    .col(ProductMedia::Position)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(ProductMedia::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}
