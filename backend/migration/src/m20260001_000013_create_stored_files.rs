use sea_orm_migration::prelude::*;

use crate::m20260001_000002_create_users::Users;
use crate::m20260001_000005_create_threads::Threads;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000013_create_stored_files"
    }
}

#[derive(Iden)]
pub enum StoredFiles {
    Table,
    Key,
    ContentType,
    Data,
    Size,
    RefCount,
    UploadedById,
    CreatedAt,
}

#[derive(Iden)]
pub enum UserAvatars {
    Table,
    UserId,
    FileKey,
    UpdatedAt,
}

#[derive(Iden)]
pub enum ThreadThumbnails {
    Table,
    ThreadId,
    FileKey,
    UpdatedAt,
}

#[derive(Iden)]
pub enum UserCovers {
    Table,
    UserId,
    FileKey,
    UpdatedAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(StoredFiles::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(StoredFiles::Key)
                            .string_len(512)
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(StoredFiles::ContentType)
                            .string_len(128)
                            .not_null(),
                    )
                    // Nullable because this table holds two separable things:
                    // the metadata every backend needs (content type, size,
                    // ref_count, uploader) and the bytes, which only the
                    // database backend stores. NULL means the file lives in an
                    // external store — S3 — and this row carries its bookkeeping
                    // alone. `NOT NULL` here would force every blob to be copied
                    // into Postgres even when something else already owns it,
                    // which is what kept `S3StorageService` from ever being a
                    // real toggle.
                    .col(ColumnDef::new(StoredFiles::Data).binary().null())
                    .col(
                        ColumnDef::new(StoredFiles::Size)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(StoredFiles::RefCount)
                            .integer()
                            .not_null()
                            .default(1),
                    )
                    .col(ColumnDef::new(StoredFiles::UploadedById).uuid().null())
                    .col(
                        ColumnDef::new(StoredFiles::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .extra("DEFAULT now()"),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_stored_files_uploaded_by_id")
                            .from(StoredFiles::Table, StoredFiles::UploadedById)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(UserAvatars::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(UserAvatars::UserId)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(UserAvatars::FileKey)
                            .string_len(512)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(UserAvatars::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .extra("DEFAULT now()"),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_user_avatars_user_id")
                            .from(UserAvatars::Table, UserAvatars::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_user_avatars_file_key")
                            .from(UserAvatars::Table, UserAvatars::FileKey)
                            .to(StoredFiles::Table, StoredFiles::Key)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(ThreadThumbnails::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(ThreadThumbnails::ThreadId)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(ThreadThumbnails::FileKey)
                            .string_len(512)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(ThreadThumbnails::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .extra("DEFAULT now()"),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_thread_thumbnails_thread_id")
                            .from(ThreadThumbnails::Table, ThreadThumbnails::ThreadId)
                            .to(Threads::Table, Threads::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_thread_thumbnails_file_key")
                            .from(ThreadThumbnails::Table, ThreadThumbnails::FileKey)
                            .to(StoredFiles::Table, StoredFiles::Key)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(UserCovers::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(UserCovers::UserId)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(UserCovers::FileKey)
                            .string_len(512)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(UserCovers::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .extra("DEFAULT now()"),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_user_covers_user_id")
                            .from(UserCovers::Table, UserCovers::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_user_covers_file_key")
                            .from(UserCovers::Table, UserCovers::FileKey)
                            .to(StoredFiles::Table, StoredFiles::Key)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(UserCovers::Table).if_exists().to_owned())
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(ThreadThumbnails::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(Table::drop().table(UserAvatars::Table).if_exists().to_owned())
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(StoredFiles::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}
