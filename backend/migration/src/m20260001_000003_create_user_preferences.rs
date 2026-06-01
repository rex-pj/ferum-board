use sea_orm_migration::prelude::*;

use crate::m20260001_000002_create_users::Users;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000003_create_user_preferences"
    }
}

#[derive(Iden)]
pub enum UserPreferences {
    Table,
    UserId,
    Theme,
    FontSize,
    Layout,
    EmailNotifications,
    MutedCategories,
    WatchedCategories,
    UpdatedAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(UserPreferences::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(UserPreferences::UserId)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(UserPreferences::Theme)
                            .text()
                            .not_null()
                            .extra("DEFAULT 'auto'"),
                    )
                    .col(
                        ColumnDef::new(UserPreferences::FontSize)
                            .text()
                            .not_null()
                            .extra("DEFAULT 'medium'"),
                    )
                    .col(
                        ColumnDef::new(UserPreferences::Layout)
                            .text()
                            .not_null()
                            .extra("DEFAULT 'comfortable'"),
                    )
                    .col(
                        ColumnDef::new(UserPreferences::EmailNotifications)
                            .json_binary()
                            .not_null()
                            .extra("DEFAULT '{}'"),
                    )
                    .col(
                        ColumnDef::new(UserPreferences::MutedCategories)
                            .custom(Alias::new("uuid[]"))
                            .not_null()
                            .extra("DEFAULT '{}'"),
                    )
                    .col(
                        ColumnDef::new(UserPreferences::WatchedCategories)
                            .custom(Alias::new("uuid[]"))
                            .not_null()
                            .extra("DEFAULT '{}'"),
                    )
                    .col(
                        ColumnDef::new(UserPreferences::UpdatedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_user_preferences_user_id")
                            .from(UserPreferences::Table, UserPreferences::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(UserPreferences::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}
