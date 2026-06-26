use sea_orm_migration::prelude::*;

use crate::enums::{post_policy::PostPolicyEnum, view_policy::ViewPolicyEnum};
use crate::m20260001_000002_create_users::Users;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000004_create_categories"
    }
}

#[derive(Iden)]
pub enum Categories {
    Table,
    Id,
    ParentId,
    Slug,
    Name,
    Description,
    Position,
    ViewPolicy,
    PostPolicy,
    Color,
    CreatedAt,
    UpdatedAt,
    CreatedById,
    UpdatedById,
}

#[derive(Iden)]
pub enum UserMutedCategories {
    Table,
    UserId,
    CategoryId,
}

#[derive(Iden)]
pub enum UserWatchedCategories {
    Table,
    UserId,
    CategoryId,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Categories::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Categories::Id)
                            .uuid()
                            .not_null()
                            .primary_key()
                            .extra("DEFAULT gen_random_uuid()"),
                    )
                    .col(ColumnDef::new(Categories::ParentId).uuid().null())
                    .col(
                        ColumnDef::new(Categories::Slug)
                            .text()
                            .not_null()
                            .unique_key(),
                    )
                    .col(ColumnDef::new(Categories::Name).text().not_null())
                    .col(ColumnDef::new(Categories::Description).text().null())
                    .col(
                        ColumnDef::new(Categories::Position)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(Categories::ViewPolicy)
                            .custom(ViewPolicyEnum::Type)
                            .not_null()
                            .extra("DEFAULT 'public'"),
                    )
                    .col(
                        ColumnDef::new(Categories::PostPolicy)
                            .custom(PostPolicyEnum::Type)
                            .not_null()
                            .extra("DEFAULT 'members'"),
                    )
                    .col(ColumnDef::new(Categories::Color).text().null())
                    .col(
                        ColumnDef::new(Categories::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .extra("DEFAULT now()"),
                    )
                    .col(
                        ColumnDef::new(Categories::UpdatedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(ColumnDef::new(Categories::CreatedById).uuid().null())
                    .col(ColumnDef::new(Categories::UpdatedById).uuid().null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_categories_parent_id")
                            .from(Categories::Table, Categories::ParentId)
                            .to(Categories::Table, Categories::Id)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_categories_created_by_id")
                            .from(Categories::Table, Categories::CreatedById)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_categories_updated_by_id")
                            .from(Categories::Table, Categories::UpdatedById)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .check(Expr::cust("char_length(slug) BETWEEN 1 AND 255"))
                    .to_owned(),
            )
            .await?;

        // User category preferences junction tables (depend on both users and categories)
        manager
            .create_table(
                Table::create()
                    .table(UserMutedCategories::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(UserMutedCategories::UserId).uuid().not_null())
                    .col(ColumnDef::new(UserMutedCategories::CategoryId).uuid().not_null())
                    .primary_key(
                        Index::create()
                            .col(UserMutedCategories::UserId)
                            .col(UserMutedCategories::CategoryId),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_user_muted_cat_user_id")
                            .from(UserMutedCategories::Table, UserMutedCategories::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_user_muted_cat_category_id")
                            .from(UserMutedCategories::Table, UserMutedCategories::CategoryId)
                            .to(Categories::Table, Categories::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(UserWatchedCategories::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(UserWatchedCategories::UserId).uuid().not_null())
                    .col(ColumnDef::new(UserWatchedCategories::CategoryId).uuid().not_null())
                    .primary_key(
                        Index::create()
                            .col(UserWatchedCategories::UserId)
                            .col(UserWatchedCategories::CategoryId),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_user_watched_cat_user_id")
                            .from(UserWatchedCategories::Table, UserWatchedCategories::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_user_watched_cat_category_id")
                            .from(UserWatchedCategories::Table, UserWatchedCategories::CategoryId)
                            .to(Categories::Table, Categories::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_user_muted_cat_user")
                    .table(UserMutedCategories::Table)
                    .col(UserMutedCategories::UserId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_user_watched_cat_user")
                    .table(UserWatchedCategories::Table)
                    .col(UserWatchedCategories::UserId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_categories_slug")
                    .table(Categories::Table)
                    .col(Categories::Slug)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(UserWatchedCategories::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(UserMutedCategories::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(Categories::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}
