use sea_orm_migration::prelude::*;

use crate::enums::thread_status::ThreadStatusEnum;
use crate::m20260001_000002_create_users::Users;
use crate::m20260001_000004_create_categories::Categories;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000006_create_threads"
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
    SearchVector,
    CustomFields,
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
                    // BestAnswerId FK added in migration 007 after posts table exists
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
                        ColumnDef::new(Threads::SearchVector)
                            .custom(Alias::new("tsvector"))
                            .null(),
                    )
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
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Threads::Table).if_exists().to_owned())
            .await
    }
}
