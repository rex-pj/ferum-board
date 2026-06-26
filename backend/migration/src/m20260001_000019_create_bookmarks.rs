use sea_orm_migration::prelude::*;

use crate::m20260001_000002_create_users::Users;
use crate::m20260001_000005_create_threads::Threads;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000019_create_bookmarks"
    }
}

#[derive(Iden)]
pub enum Bookmarks {
    Table,
    Id,
    UserId,
    ThreadId,
    CreatedAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Bookmarks::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Bookmarks::Id)
                            .uuid()
                            .not_null()
                            .primary_key()
                            .extra("DEFAULT gen_random_uuid()"),
                    )
                    .col(ColumnDef::new(Bookmarks::UserId).uuid().not_null())
                    .col(ColumnDef::new(Bookmarks::ThreadId).uuid().not_null())
                    .col(
                        ColumnDef::new(Bookmarks::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .extra("DEFAULT now()"),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_bookmarks_user_id")
                            .from(Bookmarks::Table, Bookmarks::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_bookmarks_thread_id")
                            .from(Bookmarks::Table, Bookmarks::ThreadId)
                            .to(Threads::Table, Threads::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .index(
                        Index::create()
                            .name("uq_bookmarks_user_thread")
                            .col(Bookmarks::UserId)
                            .col(Bookmarks::ThreadId)
                            .unique(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_bookmarks_user")
                    .table(Bookmarks::Table)
                    .col(Bookmarks::UserId)
                    .col(Bookmarks::CreatedAt)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Bookmarks::Table).if_exists().to_owned())
            .await
    }
}
