use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260002_000035_post_approval"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // 1. Create post_status enum
        db.execute_unprepared(
            "CREATE TYPE post_status AS ENUM ('pending', 'published')",
        )
        .await?;

        // 2. Add status column to posts (default published so existing rows are unaffected)
        db.execute_unprepared(
            "ALTER TABLE posts ADD COLUMN status post_status NOT NULL DEFAULT 'published'",
        )
        .await?;

        // 3. Add moderated value to post_policy enum
        db.execute_unprepared(
            "ALTER TYPE post_policy ADD VALUE IF NOT EXISTS 'moderated'",
        )
        .await?;

        // 4. Seed site_config rows for global approval settings
        db.execute_unprepared(
            "INSERT INTO site_config (key, value, updated_at)
             VALUES
               ('post_approval_enabled', 'false', NOW()),
               ('post_approval_min_trust', 'new', NOW())
             ON CONFLICT (key) DO NOTHING",
        )
        .await?;

        // 5. Index to fetch pending queue efficiently
        db.execute_unprepared(
            "CREATE INDEX idx_posts_pending ON posts(thread_id, created_at ASC) WHERE status = 'pending'",
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        db.execute_unprepared(
            "DROP INDEX IF EXISTS idx_posts_pending",
        )
        .await?;

        db.execute_unprepared(
            "DELETE FROM site_config WHERE key IN ('post_approval_enabled', 'post_approval_min_trust')",
        )
        .await?;

        db.execute_unprepared(
            "ALTER TABLE posts DROP COLUMN IF EXISTS status",
        )
        .await?;

        db.execute_unprepared("DROP TYPE IF EXISTS post_status").await?;

        Ok(())
    }
}
