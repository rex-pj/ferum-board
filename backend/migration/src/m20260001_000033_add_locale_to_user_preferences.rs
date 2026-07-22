use sea_orm_migration::prelude::*;

use crate::m20260001_000003_create_user_preferences::UserPreferences;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000033_add_locale_to_user_preferences"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Nullable rather than NOT NULL DEFAULT 'en': NULL means "this user has
        // never chosen", which is genuinely different from "this user chose
        // English". Only the former should follow the site default when an admin
        // changes it, or fall through to Accept-Language negotiation.
        manager
            .alter_table(
                Table::alter()
                    .table(UserPreferences::Table)
                    .add_column(ColumnDef::new(UserPreferences::Locale).string().null())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(UserPreferences::Table)
                    .drop_column(UserPreferences::Locale)
                    .to_owned(),
            )
            .await
    }
}
