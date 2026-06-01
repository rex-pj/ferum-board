use sea_orm_migration::prelude::*;

use crate::enums::report_status::ReportStatusEnum;
use crate::m20260001_000002_create_users::Users;
use crate::m20260001_000006_create_threads::Threads;
use crate::m20260001_000007_create_posts::Posts;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000011_create_reports"
    }
}

#[derive(Iden)]
pub enum Reports {
    Table,
    Id,
    ReporterId,
    PostId,
    ThreadId,
    Reason,
    Status,
    ModeratorNotes,
    ResolvedById,
    ResolvedAt,
    CreatedAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Reports::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Reports::Id)
                            .uuid()
                            .not_null()
                            .primary_key()
                            .extra("DEFAULT gen_random_uuid()"),
                    )
                    .col(ColumnDef::new(Reports::ReporterId).uuid().not_null())
                    .col(ColumnDef::new(Reports::PostId).uuid().null())
                    .col(ColumnDef::new(Reports::ThreadId).uuid().null())
                    .col(ColumnDef::new(Reports::Reason).text().not_null())
                    .col(
                        ColumnDef::new(Reports::Status)
                            .custom(ReportStatusEnum::Type)
                            .not_null()
                            .extra("DEFAULT 'pending'"),
                    )
                    .col(ColumnDef::new(Reports::ModeratorNotes).text().null())
                    .col(ColumnDef::new(Reports::ResolvedById).uuid().null())
                    .col(
                        ColumnDef::new(Reports::ResolvedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(Reports::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .extra("DEFAULT now()"),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_reports_reporter_id")
                            .from(Reports::Table, Reports::ReporterId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_reports_post_id")
                            .from(Reports::Table, Reports::PostId)
                            .to(Posts::Table, Posts::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_reports_thread_id")
                            .from(Reports::Table, Reports::ThreadId)
                            .to(Threads::Table, Threads::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_reports_resolved_by_id")
                            .from(Reports::Table, Reports::ResolvedById)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    // XOR: exactly one of post_id or thread_id must be set
                    .check(Expr::cust(
                        "(post_id IS NOT NULL AND thread_id IS NULL) OR \
                         (post_id IS NULL AND thread_id IS NOT NULL)",
                    ))
                    .to_owned(),
            )
            .await?;

        manager
            .get_connection()
            .execute_unprepared(
                "CREATE INDEX idx_reports_pending \
                 ON reports(created_at ASC) \
                 WHERE status = 'pending'",
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Reports::Table).if_exists().to_owned())
            .await
    }
}
