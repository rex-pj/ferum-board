use sea_orm_migration::prelude::extension::postgres::Type;
use sea_orm_migration::prelude::*;

use crate::enums::{plugin_status::PluginStatusEnum, plugin_tier::PluginTierEnum};
use crate::m20260001_000002_create_users::Users;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000015_create_plugins"
    }
}

/// Name of the low-privilege role every plugin query runs as.
///
/// One shared role, not one per plugin: roles are cluster-wide objects while
/// plugins are per-database, and a role-per-plugin would leak objects across
/// databases sharing this cluster. Isolation *between* plugins is still enforced
/// — each plugin's schema is granted separately, and `PgPluginDbGateway` pins
/// `search_path` to the calling plugin's own schema.
pub const PLUGIN_DB_ROLE: &str = "ferum_plugin";

/// Creates the role plugin SQL executes as. Lives with the plugin tables it
/// exists to protect rather than in a migration of its own.
///
/// Plugin queries used to run with the application's own database privileges,
/// held back only by a substring denylist over the SQL text — which
/// `FROM "public".users` walked straight through, since the quoted form does not
/// contain the blocked substring `public.`. A denylist over SQL text cannot be
/// made airtight; letting Postgres own the boundary can.
///
/// The role is created with **no grants at all**, and that is sufficient on its
/// own: `PUBLIC` holds `USAGE` on schema `public`, but table privileges are
/// never granted to `PUBLIC` by default, so this role can read and write nothing.
/// Only the plugin schemas explicitly granted by
/// `PgPluginDbGateway::provision_schema` become reachable. No `REVOKE` on
/// `PUBLIC` is needed, which keeps this from disturbing anything else in the
/// database.
///
/// No backfill of pre-existing `plugin_*` schemas is needed here: this runs
/// while the plugin tables are still being created, so no plugin can yet have
/// provisioned one.
///
/// Both statements tolerate failure. `CREATE ROLE` needs `CREATEROLE`, which a
/// locked-down managed-Postgres user may not have, and a hard failure here would
/// leave the application unable to start — strictly worse than the status quo.
/// When the role is absent the gateway logs loudly and falls back to
/// denylist-only enforcement.
async fn create_plugin_db_role(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let conn = manager.get_connection();

    conn.execute_unprepared(&format!(
        r#"
        DO $$
        BEGIN
            CREATE ROLE {PLUGIN_DB_ROLE} NOLOGIN;
        EXCEPTION
            WHEN duplicate_object THEN
                RAISE NOTICE 'role {PLUGIN_DB_ROLE} already exists, leaving as is';
            WHEN insufficient_privilege THEN
                RAISE WARNING
                    'could not create role {PLUGIN_DB_ROLE} (no CREATEROLE). Plugin SQL will fall back to denylist-only enforcement.';
        END
        $$;
        "#
    ))
    .await?;

    // `SET ROLE` requires membership unless the caller is a superuser. Granting
    // the role to the connected user keeps this working for the ordinary,
    // non-superuser deployment too.
    conn.execute_unprepared(&format!(
        r#"
        DO $$
        BEGIN
            IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = '{PLUGIN_DB_ROLE}') THEN
                EXECUTE format('GRANT {PLUGIN_DB_ROLE} TO %I', current_user);
            END IF;
        EXCEPTION
            WHEN insufficient_privilege THEN
                RAISE WARNING 'could not grant {PLUGIN_DB_ROLE} to current_user';
        END
        $$;
        "#
    ))
    .await?;

    Ok(())
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

#[derive(Iden)]
pub enum PluginHooks {
    Table,
    Id,
    PluginId,
    HookName,
    Priority,
    IsActive,
    AvgMs,
    CreatedAt,
}

#[derive(Iden)]
pub enum PluginUiSlots {
    Table,
    Id,
    PluginId,
    SlotName,
    AssetUrl,
    CustomElementTag,
    Props,
    LoadOrder,
    IsActive,
}

#[derive(Iden)]
pub enum PluginLogs {
    Table,
    Id,
    PluginId,
    Level,
    HookName,
    DurationMs,
    Message,
    Context,
    CreatedAt,
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
            .create_table(
                Table::create()
                    .table(PluginHooks::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(PluginHooks::Id)
                            .uuid()
                            .not_null()
                            .primary_key()
                            .extra("DEFAULT gen_random_uuid()"),
                    )
                    .col(ColumnDef::new(PluginHooks::PluginId).uuid().not_null())
                    .col(ColumnDef::new(PluginHooks::HookName).text().not_null())
                    .col(
                        ColumnDef::new(PluginHooks::Priority)
                            .integer()
                            .not_null()
                            .default(100),
                    )
                    .col(
                        ColumnDef::new(PluginHooks::IsActive)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .col(ColumnDef::new(PluginHooks::AvgMs).integer().null())
                    .col(
                        ColumnDef::new(PluginHooks::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .extra("DEFAULT now()"),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_plugin_hooks_plugin_id")
                            .from(PluginHooks::Table, PluginHooks::PluginId)
                            .to(Plugins::Table, Plugins::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .table(PluginHooks::Table)
                    .col(PluginHooks::HookName)
                    .col(PluginHooks::IsActive)
                    .col(PluginHooks::Priority)
                    .name("idx_plugin_hooks_dispatch")
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(PluginUiSlots::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(PluginUiSlots::Id)
                            .uuid()
                            .not_null()
                            .primary_key()
                            .extra("DEFAULT gen_random_uuid()"),
                    )
                    .col(ColumnDef::new(PluginUiSlots::PluginId).uuid().not_null())
                    .col(ColumnDef::new(PluginUiSlots::SlotName).text().not_null())
                    .col(ColumnDef::new(PluginUiSlots::AssetUrl).text().not_null())
                    .col(
                        ColumnDef::new(PluginUiSlots::CustomElementTag)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(PluginUiSlots::Props)
                            .custom(Alias::new("text[]"))
                            .not_null()
                            .extra("DEFAULT '{}'::text[]"),
                    )
                    .col(
                        ColumnDef::new(PluginUiSlots::LoadOrder)
                            .integer()
                            .not_null()
                            .default(100),
                    )
                    .col(
                        ColumnDef::new(PluginUiSlots::IsActive)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_plugin_ui_slots_plugin_id")
                            .from(PluginUiSlots::Table, PluginUiSlots::PluginId)
                            .to(Plugins::Table, Plugins::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .table(PluginUiSlots::Table)
                    .col(PluginUiSlots::SlotName)
                    .col(PluginUiSlots::IsActive)
                    .col(PluginUiSlots::LoadOrder)
                    .name("idx_plugin_ui_slots_lookup")
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(PluginLogs::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(PluginLogs::Id)
                            .uuid()
                            .not_null()
                            .primary_key()
                            .extra("DEFAULT gen_random_uuid()"),
                    )
                    .col(ColumnDef::new(PluginLogs::PluginId).uuid().not_null())
                    .col(
                        ColumnDef::new(PluginLogs::Level)
                            .string_len(8)
                            .not_null()
                            .check(
                                Expr::col(PluginLogs::Level)
                                    .is_in(["trace", "info", "warn", "error"]),
                            ),
                    )
                    .col(ColumnDef::new(PluginLogs::HookName).text().null())
                    .col(ColumnDef::new(PluginLogs::DurationMs).integer().null())
                    .col(ColumnDef::new(PluginLogs::Message).text().not_null())
                    .col(
                        ColumnDef::new(PluginLogs::Context)
                            .custom(Alias::new("jsonb"))
                            .null(),
                    )
                    .col(
                        ColumnDef::new(PluginLogs::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .extra("DEFAULT now()"),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_plugin_logs_plugin_id")
                            .from(PluginLogs::Table, PluginLogs::PluginId)
                            .to(Plugins::Table, Plugins::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .table(PluginLogs::Table)
                    .col(PluginLogs::PluginId)
                    .col(PluginLogs::CreatedAt)
                    .name("idx_plugin_logs_plugin_time")
                    .to_owned(),
            )
            .await?;

        create_plugin_db_role(manager).await?;

        Ok(())
    }

    /// `ferum_plugin` is deliberately NOT dropped here. A role is cluster-wide,
    /// so it may still hold grants on plugin schemas in another database, and
    /// `DROP ROLE` errors while any grant remains — turning a routine rollback
    /// into a hard failure. An unused NOLOGIN role costs nothing to leave behind.
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(PluginLogs::Table).if_exists().to_owned())
            .await?;

        manager
            .drop_table(
                Table::drop()
                    .table(PluginUiSlots::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;

        manager
            .drop_table(
                Table::drop()
                    .table(PluginHooks::Table)
                    .if_exists()
                    .to_owned(),
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
