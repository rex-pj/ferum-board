use sea_orm_migration::prelude::*;

use crate::m20260001_000023_create_materials::Materials;
use crate::m20260001_000024_create_products::Products;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000025_create_product_materials"
    }
}

#[derive(Iden)]
pub enum ProductMaterials {
    Table,
    ProductId,
    MaterialId,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(ProductMaterials::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(ProductMaterials::ProductId)
                            .uuid()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(ProductMaterials::MaterialId)
                            .uuid()
                            .not_null(),
                    )
                    .primary_key(
                        Index::create()
                            .col(ProductMaterials::ProductId)
                            .col(ProductMaterials::MaterialId),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_product_materials_product_id")
                            .from(ProductMaterials::Table, ProductMaterials::ProductId)
                            .to(Products::Table, Products::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_product_materials_material_id")
                            .from(ProductMaterials::Table, ProductMaterials::MaterialId)
                            .to(Materials::Table, Materials::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_product_materials_material")
                    .table(ProductMaterials::Table)
                    .col(ProductMaterials::MaterialId)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(ProductMaterials::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}
