use sea_orm_migration::prelude::*;

use crate::m20260001_000002_create_users::Users;
use crate::m20260001_000005_create_threads::Threads;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000006_create_posts"
    }
}

#[derive(Iden)]
pub enum Posts {
    Table,
    Id,
    ThreadId,
    AuthorId,
    ParentId,
    ContentMd,
    ContentHtml,
    Status,
    IsDeleted,
    DeletedAt,
    DeletedById,
    EditedAt,
    EditedById,
    EditCount,
    CreatedAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Posts::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Posts::Id)
                            .uuid()
                            .not_null()
                            .primary_key()
                            .extra("DEFAULT gen_random_uuid()"),
                    )
                    .col(ColumnDef::new(Posts::ThreadId).uuid().not_null())
                    .col(ColumnDef::new(Posts::AuthorId).uuid().not_null())
                    .col(ColumnDef::new(Posts::ParentId).uuid().null())
                    .col(ColumnDef::new(Posts::ContentMd).text().not_null())
                    .col(ColumnDef::new(Posts::ContentHtml).text().not_null())
                    .col(
                        ColumnDef::new(Posts::Status)
                            .custom(Alias::new("post_status"))
                            .not_null()
                            .extra("DEFAULT 'published'"),
                    )
                    .col(
                        ColumnDef::new(Posts::IsDeleted)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(Posts::DeletedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(ColumnDef::new(Posts::DeletedById).uuid().null())
                    .col(
                        ColumnDef::new(Posts::EditedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(ColumnDef::new(Posts::EditedById).uuid().null())
                    .col(
                        ColumnDef::new(Posts::EditCount)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(Posts::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .extra("DEFAULT now()"),
                    )
                    .col(
                        // GENERATED ALWAYS AS is not supported by sea-query builder;
                        // embedding the full type+expression as a custom type avoids ALTER TABLE.
                        ColumnDef::new(Alias::new("first_image_url")).custom(Alias::new(
                            r#"TEXT GENERATED ALWAYS AS ((regexp_match(content_html, '<img[^>]+src="([^"]+)"'))[1]) STORED"#,
                        )),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_posts_thread_id")
                            .from(Posts::Table, Posts::ThreadId)
                            .to(Threads::Table, Threads::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_posts_author_id")
                            .from(Posts::Table, Posts::AuthorId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_posts_parent_id")
                            .from(Posts::Table, Posts::ParentId)
                            .to(Posts::Table, Posts::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_posts_deleted_by_id")
                            .from(Posts::Table, Posts::DeletedById)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_posts_edited_by_id")
                            .from(Posts::Table, Posts::EditedById)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await?;

        // Deferred FK: threads.best_answer_id → posts.id (breaks threads↔posts cycle)
        manager
            .create_foreign_key(
                ForeignKey::create()
                    .name("fk_threads_best_answer")
                    .from(Threads::Table, Threads::BestAnswerId)
                    .to(Posts::Table, Posts::Id)
                    .on_delete(ForeignKeyAction::SetNull)
                    .to_owned(),
            )
            .await?;

        let conn = manager.get_connection();

        conn.execute_unprepared(
            "CREATE INDEX idx_posts_thread \
             ON posts(thread_id, created_at ASC) \
             WHERE is_deleted = false; \
             \
             CREATE INDEX idx_posts_pending \
             ON posts(thread_id, created_at ASC) \
             WHERE status = 'pending' AND is_deleted = false;",
        )
        .await?;

        conn.execute_unprepared(
            "CREATE INDEX idx_posts_fts ON posts \
             USING GIN(to_tsvector('simple', content_md))",
        )
        .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_posts_stats_created")
                    .table(Posts::Table)
                    .col(Posts::CreatedAt)
                    .col(Posts::IsDeleted)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "DROP INDEX IF EXISTS idx_posts_fts; \
                 DROP INDEX IF EXISTS idx_posts_pending; \
                 DROP INDEX IF EXISTS idx_posts_thread;",
            )
            .await?;

        manager
            .drop_foreign_key(
                ForeignKey::drop()
                    .name("fk_threads_best_answer")
                    .table(Threads::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_table(Table::drop().table(Posts::Table).if_exists().to_owned())
            .await
    }
}
