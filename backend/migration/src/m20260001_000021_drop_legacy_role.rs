use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000021_drop_legacy_role"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(r#"
                ALTER TABLE users DROP COLUMN IF EXISTS role;
                ALTER TABLE users DROP COLUMN IF EXISTS is_global_mod;
                DROP TABLE IF EXISTS category_moderators;
                DROP TYPE IF EXISTS user_role;
            "#)
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Restore user_role enum and columns. Data cannot be recovered without migration 20.
        manager
            .get_connection()
            .execute_unprepared(r#"
                CREATE TYPE user_role AS ENUM ('member', 'moderator', 'admin');
                ALTER TABLE users ADD COLUMN role user_role NOT NULL DEFAULT 'member';
                ALTER TABLE users ADD COLUMN is_global_mod BOOLEAN NOT NULL DEFAULT false;

                CREATE TABLE category_moderators (
                    id            UUID        NOT NULL PRIMARY KEY DEFAULT gen_random_uuid(),
                    category_id   UUID        NOT NULL REFERENCES categories(id) ON DELETE CASCADE,
                    user_id       UUID        NOT NULL REFERENCES users(id)      ON DELETE CASCADE,
                    assigned_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
                    assigned_by_id UUID               REFERENCES users(id)      ON DELETE SET NULL,
                    CONSTRAINT uq_cat_mods_category_user UNIQUE (category_id, user_id)
                );
                CREATE INDEX idx_cat_mods_user ON category_moderators(user_id);
            "#)
            .await?;
        Ok(())
    }
}
