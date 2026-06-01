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
                    .col(ColumnDef::new(Categories::Slug).text().not_null().unique_key())
                    .col(ColumnDef::new(Categories::Name).text().not_null())
                    .col(ColumnDef::new(Categories::Description).text().null())
                    .col(ColumnDef::new(Categories::Position).integer().not_null().default(0))
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
                    .col(ColumnDef::new(Categories::UpdatedAt).timestamp_with_time_zone().null())
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
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Categories::Table).if_exists().to_owned())
            .await
    }
}
