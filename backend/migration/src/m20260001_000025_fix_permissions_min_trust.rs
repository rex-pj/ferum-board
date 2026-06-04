use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000025_fix_permissions_min_trust"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();
        conn.execute_unprepared(
            "ALTER TABLE permissions ALTER COLUMN min_trust DROP DEFAULT",
        )
        .await?;
        conn.execute_unprepared(
            "ALTER TABLE permissions \
             ALTER COLUMN min_trust TYPE trust_level \
             USING min_trust::trust_level",
        )
        .await?;
        conn.execute_unprepared(
            "ALTER TABLE permissions ALTER COLUMN min_trust SET DEFAULT 'new'::trust_level",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE permissions \
                 ALTER COLUMN min_trust TYPE TEXT \
                 USING min_trust::TEXT",
            )
            .await?;
        Ok(())
    }
}
