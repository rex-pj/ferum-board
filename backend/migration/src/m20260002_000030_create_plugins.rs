use sea_orm_migration::prelude::extension::postgres::Type;
use sea_orm_migration::prelude::*;

use crate::enums::{plugin_status::PluginStatusEnum, plugin_tier::PluginTierEnum};
use crate::m20260001_000002_create_users::Users;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260002_000030_create_plugins"
    }
}

#[derive(Iden)]
pub enum Plugins {
    Table,
    Id,
    Slug,
    Name,
    Version,
    Tier,
    Status,
    Manifest,
    Config,
    GrantedCapabilities,
    InstallPath,
    DbSchemaName,
    DbSchemaVersion,
    InstalledBy,
    InstalledAt,
    UpdatedAt,
    ActivatedAt,
    ErrorMessage,
    LastSeenAt,
    RestartCount,
    CircuitOpen,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_type(
                Type::create()
                    .as_enum(PluginTierEnum::Type)
                    .values([
                        PluginTierEnum::Manifest,
                        PluginTierEnum::Script,
                        PluginTierEnum::Service,
                    ])
                    .to_owned(),
            )
            .await?;

        manager
            .create_type(
                Type::create()
                    .as_enum(PluginStatusEnum::Type)
                    .values([
                        PluginStatusEnum::Installing,
                        PluginStatusEnum::Active,
                        PluginStatusEnum::Inactive,
                        PluginStatusEnum::Error,
                        PluginStatusEnum::Disabled,
                        PluginStatusEnum::Uninstalling,
                    ])
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Plugins::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Plugins::Id)
                            .uuid()
                            .not_null()
                            .primary_key()
                            .extra("DEFAULT gen_random_uuid()"),
                    )
                    .col(
                        ColumnDef::new(Plugins::Slug)
                            .string_len(256)
                            .not_null()
                            .unique_key(),
                    )
                    .col(ColumnDef::new(Plugins::Name).text().not_null())
                    .col(ColumnDef::new(Plugins::Version).string_len(64).not_null())
                    .col(
                        ColumnDef::new(Plugins::Tier)
                            .custom(Alias::new("plugin_tier"))
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Plugins::Status)
                            .custom(Alias::new("plugin_status"))
                            .not_null()
                            .extra("DEFAULT 'installing'::plugin_status"),
                    )
                    .col(
                        ColumnDef::new(Plugins::Manifest)
                            .custom(Alias::new("jsonb"))
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Plugins::Config)
                            .custom(Alias::new("jsonb"))
                            .not_null()
                            .extra("DEFAULT '{}'::jsonb"),
                    )
                    .col(
                        ColumnDef::new(Plugins::GrantedCapabilities)
                            .custom(Alias::new("jsonb"))
                            .not_null()
                            .extra("DEFAULT '{}'::jsonb"),
                    )
                    .col(ColumnDef::new(Plugins::InstallPath).text().not_null())
                    .col(ColumnDef::new(Plugins::DbSchemaName).text().null())
                    .col(
                        ColumnDef::new(Plugins::DbSchemaVersion)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(ColumnDef::new(Plugins::InstalledBy).uuid().null())
                    .col(
                        ColumnDef::new(Plugins::InstalledAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .extra("DEFAULT now()"),
                    )
                    .col(
                        ColumnDef::new(Plugins::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .extra("DEFAULT now()"),
                    )
                    .col(
                        ColumnDef::new(Plugins::ActivatedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(ColumnDef::new(Plugins::ErrorMessage).text().null())
                    .col(
                        ColumnDef::new(Plugins::LastSeenAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(Plugins::RestartCount)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(Plugins::CircuitOpen)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_plugins_installed_by")
                            .from(Plugins::Table, Plugins::InstalledBy)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .get_connection()
            .execute_unprepared(
                r#"
                CREATE TRIGGER trg_plugins_updated_at
                    BEFORE UPDATE ON plugins
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
                "DROP TRIGGER IF EXISTS trg_plugins_updated_at ON plugins",
            )
            .await?;

        manager
            .drop_table(Table::drop().table(Plugins::Table).if_exists().to_owned())
            .await?;

        manager
            .drop_type(
                Type::drop()
                    .name(PluginStatusEnum::Type)
                    .if_exists()
                    .to_owned(),
            )
            .await?;

        manager
            .drop_type(
                Type::drop()
                    .name(PluginTierEnum::Type)
                    .if_exists()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}
