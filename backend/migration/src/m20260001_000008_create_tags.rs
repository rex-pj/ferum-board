use sea_orm_migration::prelude::*;

use crate::m20260001_000002_create_users::Users;
use crate::m20260001_000006_create_threads::Threads;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000008_create_tags"
    }
}

#[derive(Iden)]
pub enum Tags {
    Table,
    Id,
    Name,
    Slug,
    Color,
    CreatedAt,
    CreatedById,
}

#[derive(Iden)]
pub enum ThreadTags {
    Table,
    ThreadId,
    TagId,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Tags::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Tags::Id)
                            .uuid()
                            .not_null()
                            .primary_key()
                            .extra("DEFAULT gen_random_uuid()"),
                    )
                    .col(ColumnDef::new(Tags::Name).text().not_null().unique_key())
                    .col(ColumnDef::new(Tags::Slug).text().not_null().unique_key())
                    .col(ColumnDef::new(Tags::Color).text().null())
                    .col(
                        ColumnDef::new(Tags::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .extra("DEFAULT now()"),
                    )
                    .col(ColumnDef::new(Tags::CreatedById).uuid().null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_tags_created_by_id")
                            .from(Tags::Table, Tags::CreatedById)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(ThreadTags::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(ThreadTags::ThreadId).uuid().not_null())
                    .col(ColumnDef::new(ThreadTags::TagId).uuid().not_null())
                    .primary_key(
                        Index::create()
                            .col(ThreadTags::ThreadId)
                            .col(ThreadTags::TagId),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_thread_tags_thread_id")
                            .from(ThreadTags::Table, ThreadTags::ThreadId)
                            .to(Threads::Table, Threads::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_thread_tags_tag_id")
                            .from(ThreadTags::Table, ThreadTags::TagId)
                            .to(Tags::Table, Tags::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(ThreadTags::Table).if_exists().to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Tags::Table).if_exists().to_owned())
            .await
    }
}
