use sea_orm_migration::prelude::*;

use crate::m20260001_000002_create_users::Users;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000017_create_stored_files"
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
                    .col(ColumnDef::new(StoredFiles::ContentType).string_len(128).not_null())
                    .col(ColumnDef::new(StoredFiles::Data).binary().not_null())
                    .col(ColumnDef::new(StoredFiles::Size).big_integer().not_null().default(0))
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
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(StoredFiles::Table).if_exists().to_owned())
            .await
    }
}
