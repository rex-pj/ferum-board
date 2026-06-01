use sea_orm_migration::prelude::*;

use crate::m20260001_000002_create_users::Users;
use crate::m20260001_000004_create_categories::Categories;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000005_create_category_moderators"
    }
}

#[derive(Iden)]
pub enum CategoryModerators {
    Table,
    Id,
    CategoryId,
    UserId,
    AssignedAt,
    AssignedById,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(CategoryModerators::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(CategoryModerators::Id)
                            .uuid()
                            .not_null()
                            .primary_key()
                            .extra("DEFAULT gen_random_uuid()"),
                    )
                    .col(ColumnDef::new(CategoryModerators::CategoryId).uuid().not_null())
                    .col(ColumnDef::new(CategoryModerators::UserId).uuid().not_null())
                    .col(
                        ColumnDef::new(CategoryModerators::AssignedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .extra("DEFAULT now()"),
                    )
                    .col(ColumnDef::new(CategoryModerators::AssignedById).uuid().null())
                    .index(
                        Index::create()
                            .name("uq_cat_mods_category_user")
                            .col(CategoryModerators::CategoryId)
                            .col(CategoryModerators::UserId)
                            .unique(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_cat_mods_category_id")
                            .from(CategoryModerators::Table, CategoryModerators::CategoryId)
                            .to(Categories::Table, Categories::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_cat_mods_user_id")
                            .from(CategoryModerators::Table, CategoryModerators::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_cat_mods_assigned_by_id")
                            .from(CategoryModerators::Table, CategoryModerators::AssignedById)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_cat_mods_user")
                    .table(CategoryModerators::Table)
                    .col(CategoryModerators::UserId)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(CategoryModerators::Table).if_exists().to_owned())
            .await
    }
}
