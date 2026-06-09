use sea_orm_migration::prelude::*;

use crate::m20260002_000030_create_plugins::Plugins;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260002_000032_create_plugin_ui_slots"
    }
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

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
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

        // Slot lookup index: fast frontend hydration query
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

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(PluginUiSlots::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}
