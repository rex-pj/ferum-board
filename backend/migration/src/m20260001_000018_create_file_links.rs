use sea_orm_migration::prelude::*;

use crate::m20260001_000002_create_users::Users;
use crate::m20260001_000006_create_threads::Threads;
use crate::m20260001_000017_create_stored_files::StoredFiles;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000018_create_file_links"
    }
}

#[derive(Iden)]
pub enum UserAvatars {
    Table,
    UserId,
    FileKey,
    CreatedAt,
    UpdatedAt,
}

#[derive(Iden)]
pub enum ThreadThumbnails {
    Table,
    ThreadId,
    FileKey,
    CreatedAt,
    UpdatedAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
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
                    .col(ColumnDef::new(UserAvatars::FileKey).string_len(512).not_null())
                    .col(
                        ColumnDef::new(UserAvatars::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .extra("DEFAULT now()"),
                    )
                    .col(ColumnDef::new(UserAvatars::UpdatedAt).timestamp_with_time_zone().null())
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
                    .col(ColumnDef::new(ThreadThumbnails::FileKey).string_len(512).not_null())
                    .col(
                        ColumnDef::new(ThreadThumbnails::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .extra("DEFAULT now()"),
                    )
                    .col(ColumnDef::new(ThreadThumbnails::UpdatedAt).timestamp_with_time_zone().null())
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
            .get_connection()
            .execute_unprepared(
                r#"
                CREATE TRIGGER trg_user_avatars_updated_at
                    BEFORE UPDATE ON user_avatars
                    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

                CREATE TRIGGER trg_thread_thumbnails_updated_at
                    BEFORE UPDATE ON thread_thumbnails
                    FOR EACH ROW EXECUTE FUNCTION set_updated_at()
                "#,
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
                DROP TRIGGER IF EXISTS trg_thread_thumbnails_updated_at ON thread_thumbnails;
                DROP TRIGGER IF EXISTS trg_user_avatars_updated_at ON user_avatars
                "#,
            )
            .await?;
        manager
            .drop_table(Table::drop().table(ThreadThumbnails::Table).if_exists().to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(UserAvatars::Table).if_exists().to_owned())
            .await?;
        Ok(())
    }
}
