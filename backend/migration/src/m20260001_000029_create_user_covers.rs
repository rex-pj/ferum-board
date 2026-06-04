use sea_orm_migration::prelude::*;

use crate::m20260001_000002_create_users::Users;
use crate::m20260001_000017_create_stored_files::StoredFiles;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000029_create_user_covers"
    }
}

#[derive(Iden)]
pub enum UserCovers {
    Table,
    UserId,
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
                        ColumnDef::new(UserCovers::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .extra("DEFAULT now()"),
                    )
                    .col(
                        ColumnDef::new(UserCovers::UpdatedAt)
                            .timestamp_with_time_zone()
                            .null(),
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

        manager
            .get_connection()
            .execute_unprepared(
                r#"
                CREATE TRIGGER trg_user_covers_updated_at
                    BEFORE UPDATE ON user_covers
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
                "DROP TRIGGER IF EXISTS trg_user_covers_updated_at ON user_covers",
            )
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(UserCovers::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}
