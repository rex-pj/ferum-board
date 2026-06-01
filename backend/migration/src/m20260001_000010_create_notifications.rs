use sea_orm_migration::prelude::*;

use crate::enums::notification_kind::NotificationKindEnum;
use crate::m20260001_000002_create_users::Users;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000010_create_notifications"
    }
}

#[derive(Iden)]
pub enum Notifications {
    Table,
    Id,
    UserId,
    Kind,
    Payload,
    IsRead,
    ReadAt,
    CreatedAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Notifications::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Notifications::Id)
                            .uuid()
                            .not_null()
                            .primary_key()
                            .extra("DEFAULT gen_random_uuid()"),
                    )
                    .col(ColumnDef::new(Notifications::UserId).uuid().not_null())
                    .col(
                        ColumnDef::new(Notifications::Kind)
                            .custom(NotificationKindEnum::Type)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Notifications::Payload)
                            .json_binary()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Notifications::IsRead)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(Notifications::ReadAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(Notifications::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .extra("DEFAULT now()"),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_notifications_user_id")
                            .from(Notifications::Table, Notifications::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Partial index not supported by builder — raw SQL required
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE INDEX idx_notifications_inbox \
                 ON notifications(user_id, created_at DESC) \
                 WHERE is_read = false",
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(Notifications::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}
