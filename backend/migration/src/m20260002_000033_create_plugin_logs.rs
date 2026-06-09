use sea_orm_migration::prelude::*;

use crate::m20260002_000030_create_plugins::Plugins;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260002_000033_create_plugin_logs"
    }
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
                    .col(ColumnDef::new(PluginLogs::Level).string_len(8).not_null())
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

        // Index for efficient log tailing per plugin
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

        // Constraint: level must be one of known values
        manager
            .get_connection()
            .execute_unprepared(
                r#"
                ALTER TABLE plugin_logs
                    ADD CONSTRAINT chk_plugin_logs_level
                    CHECK (level IN ('trace', 'info', 'warn', 'error'))
                "#,
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(PluginLogs::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}
