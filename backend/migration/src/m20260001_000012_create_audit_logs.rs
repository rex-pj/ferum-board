use sea_orm_migration::prelude::*;

use crate::m20260001_000002_create_users::Users;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000012_create_audit_logs"
    }
}

#[derive(Iden)]
pub enum AuditLogs {
    Table,
    Id,
    ActorId,
    Action,
    TargetType,
    TargetId,
    Metadata,
    CreatedAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(AuditLogs::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(AuditLogs::Id)
                            .uuid()
                            .not_null()
                            .primary_key()
                            .extra("DEFAULT gen_random_uuid()"),
                    )
                    .col(ColumnDef::new(AuditLogs::ActorId).uuid().null())
                    .col(ColumnDef::new(AuditLogs::Action).text().not_null())
                    .col(ColumnDef::new(AuditLogs::TargetType).text().not_null())
                    .col(ColumnDef::new(AuditLogs::TargetId).uuid().not_null())
                    .col(ColumnDef::new(AuditLogs::Metadata).json_binary().null())
                    .col(
                        ColumnDef::new(AuditLogs::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .extra("DEFAULT now()"),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_audit_logs_actor_id")
                            .from(AuditLogs::Table, AuditLogs::ActorId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_audit_logs_actor")
                    .table(AuditLogs::Table)
                    .col(AuditLogs::ActorId)
                    .col(AuditLogs::CreatedAt)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_audit_logs_target")
                    .table(AuditLogs::Table)
                    .col(AuditLogs::TargetType)
                    .col(AuditLogs::TargetId)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(AuditLogs::Table).if_exists().to_owned())
            .await
    }
}
