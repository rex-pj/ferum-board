use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000026_roles_updated_at"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(r#"
                ALTER TABLE roles      ADD COLUMN updated_at TIMESTAMPTZ;
                ALTER TABLE user_roles ADD COLUMN updated_at TIMESTAMPTZ;

                CREATE TRIGGER trg_roles_updated_at
                BEFORE UPDATE ON roles
                FOR EACH ROW EXECUTE FUNCTION set_updated_at();

                CREATE TRIGGER trg_user_roles_updated_at
                BEFORE UPDATE ON user_roles
                FOR EACH ROW EXECUTE FUNCTION set_updated_at();
            "#)
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(r#"
                DROP TRIGGER IF EXISTS trg_user_roles_updated_at ON user_roles;
                DROP TRIGGER IF EXISTS trg_roles_updated_at      ON roles;
                ALTER TABLE user_roles DROP COLUMN IF EXISTS updated_at;
                ALTER TABLE roles      DROP COLUMN IF EXISTS updated_at;
            "#)
            .await?;
        Ok(())
    }
}
