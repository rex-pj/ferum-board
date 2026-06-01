use sea_orm_migration::prelude::*;

use crate::m20260001_000002_create_users::Users;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000013_create_site_config"
    }
}

#[derive(Iden)]
pub enum SiteConfig {
    Table,
    Key,
    Value,
    UpdatedAt,
    UpdatedById,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(SiteConfig::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(SiteConfig::Key)
                            .text()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(SiteConfig::Value).text().not_null())
                    .col(
                        ColumnDef::new(SiteConfig::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .extra("DEFAULT now()"),
                    )
                    .col(ColumnDef::new(SiteConfig::UpdatedById).uuid().null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_site_config_updated_by_id")
                            .from(SiteConfig::Table, SiteConfig::UpdatedById)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .get_connection()
            .execute_unprepared(
                "INSERT INTO site_config (key, value) VALUES
                    ('site_name', 'Ferum Board'),
                    ('site_tagline', 'A modern self-hosted forum'),
                    ('logo_url', ''),
                    ('favicon_url', ''),
                    ('primary_color', '#0d6efd'),
                    ('registration_open', 'true'),
                    ('keyword_blacklist', ''),
                    ('smtp_host', ''),
                    ('smtp_port', '587'),
                    ('smtp_user', '')",
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(SiteConfig::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}
