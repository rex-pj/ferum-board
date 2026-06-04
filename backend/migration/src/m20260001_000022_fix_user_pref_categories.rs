use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000022_fix_user_pref_categories"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        conn.execute_unprepared(r#"
            CREATE TABLE user_muted_categories (
                user_id     UUID NOT NULL REFERENCES users(id)      ON DELETE CASCADE,
                category_id UUID NOT NULL REFERENCES categories(id) ON DELETE CASCADE,
                PRIMARY KEY (user_id, category_id)
            );

            CREATE TABLE user_watched_categories (
                user_id     UUID NOT NULL REFERENCES users(id)      ON DELETE CASCADE,
                category_id UUID NOT NULL REFERENCES categories(id) ON DELETE CASCADE,
                PRIMARY KEY (user_id, category_id)
            );

            CREATE INDEX idx_user_muted_cat_user   ON user_muted_categories(user_id);
            CREATE INDEX idx_user_watched_cat_user  ON user_watched_categories(user_id);

            INSERT INTO user_muted_categories (user_id, category_id)
            SELECT up.user_id, unnest(up.muted_categories)
            FROM user_preferences up
            WHERE array_length(up.muted_categories, 1) > 0
            ON CONFLICT DO NOTHING;

            INSERT INTO user_watched_categories (user_id, category_id)
            SELECT up.user_id, unnest(up.watched_categories)
            FROM user_preferences up
            WHERE array_length(up.watched_categories, 1) > 0
            ON CONFLICT DO NOTHING;

            ALTER TABLE user_preferences DROP COLUMN muted_categories;
            ALTER TABLE user_preferences DROP COLUMN watched_categories;
        "#).await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        conn.execute_unprepared(r#"
            ALTER TABLE user_preferences
                ADD COLUMN muted_categories  uuid[] NOT NULL DEFAULT '{}',
                ADD COLUMN watched_categories uuid[] NOT NULL DEFAULT '{}';

            UPDATE user_preferences up
            SET muted_categories = (
                SELECT array_agg(umc.category_id)
                FROM user_muted_categories umc
                WHERE umc.user_id = up.user_id
            )
            WHERE EXISTS (
                SELECT 1 FROM user_muted_categories umc WHERE umc.user_id = up.user_id
            );

            UPDATE user_preferences up
            SET watched_categories = (
                SELECT array_agg(uwc.category_id)
                FROM user_watched_categories uwc
                WHERE uwc.user_id = up.user_id
            )
            WHERE EXISTS (
                SELECT 1 FROM user_watched_categories uwc WHERE uwc.user_id = up.user_id
            );

            DROP TABLE IF EXISTS user_watched_categories;
            DROP TABLE IF EXISTS user_muted_categories;
        "#).await?;

        Ok(())
    }
}
