use sea_orm_migration::prelude::*;

use crate::m20260001_000002_create_users::Users;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000012_create_site_config"
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

        manager.exec_stmt(
            Query::insert()
                .into_table(SiteConfig::Table)
                .columns([SiteConfig::Key, SiteConfig::Value])
                .values_panic(["site_name".into(), "Ferum Board".into()])
                .values_panic(["site_slogan".into(), "".into()])
                .values_panic(["site_tagline".into(), "A modern self-hosted forum".into()])
                .values_panic(["logo_url".into(), "".into()])
                .values_panic(["favicon_url".into(), "".into()])
                .values_panic(["primary_color".into(), "#0d6efd".into()])
                .values_panic(["registration_open".into(), "true".into()])
                .values_panic(["keyword_blacklist".into(), "".into()])
                .values_panic(["smtp_host".into(), "".into()])
                .values_panic(["smtp_port".into(), "587".into()])
                .values_panic(["smtp_user".into(), "".into()])
                .values_panic(["post_approval_enabled".into(), "false".into()])
                .values_panic(["post_approval_min_trust".into(), "new".into()])
                .to_owned(),
        ).await?;

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
