use sea_orm_migration::prelude::extension::postgres::Type;
use sea_orm_migration::prelude::*;

use crate::enums::{product_status::ProductStatusEnum, product_type::ProductTypeEnum};

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000021_create_furniture_enums"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_type(
                Type::create()
                    .as_enum(ProductTypeEnum::Type)
                    .values([
                        ProductTypeEnum::Furniture,
                        ProductTypeEnum::Material,
                        ProductTypeEnum::Room,
                    ])
                    .to_owned(),
            )
            .await?;

        manager
            .create_type(
                Type::create()
                    .as_enum(ProductStatusEnum::Type)
                    .values([
                        ProductStatusEnum::Draft,
                        ProductStatusEnum::Published,
                        ProductStatusEnum::Archived,
                    ])
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_type(
                Type::drop()
                    .name(ProductStatusEnum::Type)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_type(
                Type::drop()
                    .name(ProductTypeEnum::Type)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}
