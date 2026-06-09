use sea_orm_migration::prelude::*;

use crate::m20260002_000030_create_plugins::Plugins;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260002_000031_create_plugin_hooks"
    }
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

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
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

        // Dispatch index: fast lookup by hook_name for active hooks, ordered by priority
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

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(PluginHooks::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}
