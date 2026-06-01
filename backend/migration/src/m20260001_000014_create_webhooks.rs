use sea_orm_migration::prelude::*;

use crate::m20260001_000002_create_users::Users;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000014_create_webhooks"
    }
}

#[derive(Iden)]
pub enum Webhooks {
    Table,
    Id,
    Url,
    Events,
    Secret,
    IsActive,
    CreatedAt,
    UpdatedAt,
    CreatedById,
    LastTriggeredAt,
    FailureCount,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Webhooks::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Webhooks::Id)
                            .uuid()
                            .not_null()
                            .primary_key()
                            .extra("DEFAULT gen_random_uuid()"),
                    )
                    .col(ColumnDef::new(Webhooks::Url).text().not_null())
                    .col(
                        ColumnDef::new(Webhooks::Events)
                            .custom(Alias::new("text[]"))
                            .not_null(),
                    )
                    .col(ColumnDef::new(Webhooks::Secret).text().null())
                    .col(ColumnDef::new(Webhooks::IsActive).boolean().not_null().default(true))
                    .col(
                        ColumnDef::new(Webhooks::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .extra("DEFAULT now()"),
                    )
                    .col(ColumnDef::new(Webhooks::UpdatedAt).timestamp_with_time_zone().null())
                    .col(ColumnDef::new(Webhooks::CreatedById).uuid().null())
                    .col(ColumnDef::new(Webhooks::LastTriggeredAt).timestamp_with_time_zone().null())
                    .col(
                        ColumnDef::new(Webhooks::FailureCount)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_webhooks_created_by_id")
                            .from(Webhooks::Table, Webhooks::CreatedById)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Webhooks::Table).if_exists().to_owned())
            .await
    }
}
