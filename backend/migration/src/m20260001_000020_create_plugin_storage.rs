use sea_orm_migration::prelude::*;

use crate::m20260001_000015_create_plugins::Plugins;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000020_create_plugin_storage"
    }
}

#[derive(Iden)]
pub enum PluginStorage {
    Table,
    PluginId,
    Key,
    Value,
    UpdatedAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(PluginStorage::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(PluginStorage::PluginId).uuid().not_null())
                    .col(ColumnDef::new(PluginStorage::Key).text().not_null())
                    .col(
                        ColumnDef::new(PluginStorage::Value)
                            .custom(Alias::new("jsonb"))
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(PluginStorage::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .extra("DEFAULT now()"),
                    )
                    .primary_key(
                        Index::create()
                            .col(PluginStorage::PluginId)
                            .col(PluginStorage::Key),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_plugin_storage_plugin_id")
                            .from(PluginStorage::Table, PluginStorage::PluginId)
                            .to(Plugins::Table, Plugins::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // The composite primary key (plugin_id, key) already gives a btree index
        // usable for Ferum.storage.list(prefix) prefix scans — no extra index needed.

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(PluginStorage::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}
