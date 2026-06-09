use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260003_000038_add_plugin_id_to_webhooks"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
                ALTER TABLE webhooks
                    ADD COLUMN IF NOT EXISTS plugin_id UUID NULL
                        REFERENCES plugins(id) ON DELETE CASCADE;

                CREATE INDEX IF NOT EXISTS idx_webhooks_plugin_id
                    ON webhooks(plugin_id)
                    WHERE plugin_id IS NOT NULL;
                "#,
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
                DROP INDEX IF EXISTS idx_webhooks_plugin_id;
                ALTER TABLE webhooks DROP COLUMN IF EXISTS plugin_id;
                "#,
            )
            .await?;

        Ok(())
    }
}
