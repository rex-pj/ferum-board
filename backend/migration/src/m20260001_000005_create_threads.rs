use sea_orm_migration::prelude::*;

use crate::enums::thread_status::ThreadStatusEnum;
use crate::m20260001_000002_create_users::Users;
use crate::m20260001_000004_create_categories::Categories;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000005_create_threads"
    }
}

#[derive(Iden)]
pub enum Threads {
    Table,
    Id,
    CategoryId,
    AuthorId,
    Title,
    Slug,
    Status,
    IsPinned,
    IsSolved,
    BestAnswerId,
    ViewCount,
    ReplyCount,
    LastPostAt,
    CreatedAt,
    UpdatedAt,
    DeletedAt,
    DeletedById,
    CustomFields,
    // Added in m20260001_000027 — links a review thread to the product it reviews.
    ProductId,
}

#[derive(Iden)]
pub enum ThreadViewDedup {
    Table,
    ThreadId,
    ViewerKey,
    ViewerType,
    LastViewedDate,
    TotalViews,
    FirstViewedAt,
    LastViewedAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Threads::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Threads::Id)
                            .uuid()
                            .not_null()
                            .primary_key()
                            .extra("DEFAULT gen_random_uuid()"),
                    )
                    .col(ColumnDef::new(Threads::CategoryId).uuid().not_null())
                    .col(ColumnDef::new(Threads::AuthorId).uuid().not_null())
                    .col(ColumnDef::new(Threads::Title).text().not_null())
                    .col(ColumnDef::new(Threads::Slug).text().not_null().unique_key())
                    .col(
                        ColumnDef::new(Threads::Status)
                            .custom(ThreadStatusEnum::Type)
                            .not_null()
                            .extra("DEFAULT 'open'"),
                    )
                    .col(
                        ColumnDef::new(Threads::IsPinned)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(Threads::IsSolved)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    // BestAnswerId FK added in migration 006 after posts table exists
                    .col(ColumnDef::new(Threads::BestAnswerId).uuid().null())
                    .col(
                        ColumnDef::new(Threads::ViewCount)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(Threads::ReplyCount)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(Threads::LastPostAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(Threads::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .extra("DEFAULT now()"),
                    )
                    .col(
                        ColumnDef::new(Threads::UpdatedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(Threads::DeletedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(ColumnDef::new(Threads::DeletedById).uuid().null())
                    .col(
                        ColumnDef::new(Threads::CustomFields)
                            .json_binary()
                            .not_null()
                            .extra("DEFAULT '{}'"),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_threads_category_id")
                            .from(Threads::Table, Threads::CategoryId)
                            .to(Categories::Table, Categories::Id)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_threads_author_id")
                            .from(Threads::Table, Threads::AuthorId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_threads_deleted_by_id")
                            .from(Threads::Table, Threads::DeletedById)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .check(Expr::cust("char_length(slug) BETWEEN 1 AND 255"))
                    .to_owned(),
            )
            .await?;

        let conn = manager.get_connection();

        conn.execute_unprepared(
            "CREATE INDEX idx_threads_category \
             ON threads(category_id, is_pinned DESC, last_post_at DESC NULLS LAST) \
             WHERE deleted_at IS NULL; \
             \
             CREATE INDEX idx_threads_author \
             ON threads(author_id, created_at DESC) \
             WHERE deleted_at IS NULL; \
             \
             CREATE INDEX idx_threads_admin_list \
             ON threads(is_pinned DESC, last_post_at DESC NULLS LAST) \
             WHERE deleted_at IS NULL;",
        )
        .await?;

        conn.execute_unprepared(
            "CREATE INDEX idx_threads_fts ON threads \
             USING GIN(to_tsvector('simple', title))",
        )
        .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_threads_slug")
                    .table(Threads::Table)
                    .col(Threads::Slug)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_threads_stats_created")
                    .table(Threads::Table)
                    .col(Threads::CreatedAt)
                    .col(Threads::Status)
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(ThreadViewDedup::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(ThreadViewDedup::ThreadId).uuid().not_null())
                    .col(ColumnDef::new(ThreadViewDedup::ViewerKey).text().not_null())
                    .col(
                        ColumnDef::new(ThreadViewDedup::ViewerType)
                            .text()
                            .not_null()
                            .check(
                                Expr::col(ThreadViewDedup::ViewerType)
                                    .is_in(["user", "guest"]),
                            ),
                    )
                    .col(
                        ColumnDef::new(ThreadViewDedup::LastViewedDate)
                            .date()
                            .not_null()
                            .extra("DEFAULT CURRENT_DATE"),
                    )
                    .col(
                        ColumnDef::new(ThreadViewDedup::TotalViews)
                            .integer()
                            .not_null()
                            .default(1),
                    )
                    .col(
                        ColumnDef::new(ThreadViewDedup::FirstViewedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .extra("DEFAULT now()"),
                    )
                    .col(
                        ColumnDef::new(ThreadViewDedup::LastViewedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .extra("DEFAULT now()"),
                    )
                    .primary_key(
                        Index::create()
                            .col(ThreadViewDedup::ThreadId)
                            .col(ThreadViewDedup::ViewerKey),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_thread_view_dedup_thread")
                            .from(ThreadViewDedup::Table, ThreadViewDedup::ThreadId)
                            .to(Threads::Table, Threads::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_view_dedup_thread")
                    .table(ThreadViewDedup::Table)
                    .col(ThreadViewDedup::ThreadId)
                    .col((ThreadViewDedup::LastViewedDate, IndexOrder::Desc))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_view_dedup_cleanup")
                    .table(ThreadViewDedup::Table)
                    .col(ThreadViewDedup::LastViewedDate)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(ThreadViewDedup::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;

        manager
            .get_connection()
            .execute_unprepared(
                "DROP INDEX IF EXISTS idx_threads_admin_list; \
                 DROP INDEX IF EXISTS idx_threads_author; \
                 DROP INDEX IF EXISTS idx_threads_category;",
            )
            .await?;

        manager
            .drop_table(Table::drop().table(Threads::Table).if_exists().to_owned())
            .await
    }
}
