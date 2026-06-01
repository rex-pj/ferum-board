use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000015_create_indexes_and_fts"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
                -- Thread listing: category feed, pinned first, most recent activity first
                CREATE INDEX idx_threads_category
                    ON threads(category_id, is_pinned DESC, last_post_at DESC NULLS LAST)
                    WHERE deleted_at IS NULL;

                -- Full-text search on thread title
                CREATE INDEX idx_threads_fts ON threads USING GIN(search_vector);

                -- Posts per thread (chronological, exclude deleted)
                CREATE INDEX idx_posts_thread
                    ON posts(thread_id, created_at ASC)
                    WHERE is_deleted = false;

                -- FTS trigger: maintain search_vector from title
                CREATE OR REPLACE FUNCTION update_thread_search_vector()
                RETURNS TRIGGER AS $$
                BEGIN
                    NEW.search_vector :=
                        setweight(to_tsvector('simple', coalesce(NEW.title, '')), 'A');
                    RETURN NEW;
                END;
                $$ LANGUAGE plpgsql;

                CREATE TRIGGER trg_thread_search
                BEFORE INSERT OR UPDATE OF title ON threads
                FOR EACH ROW EXECUTE FUNCTION update_thread_search_vector();

                -- updated_at auto-maintenance trigger
                CREATE OR REPLACE FUNCTION set_updated_at()
                RETURNS TRIGGER AS $$
                BEGIN
                    NEW.updated_at = now();
                    RETURN NEW;
                END;
                $$ LANGUAGE plpgsql;

                CREATE TRIGGER trg_users_updated_at
                    BEFORE UPDATE ON users
                    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

                CREATE TRIGGER trg_user_preferences_updated_at
                    BEFORE UPDATE ON user_preferences
                    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

                CREATE TRIGGER trg_categories_updated_at
                    BEFORE UPDATE ON categories
                    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

                CREATE TRIGGER trg_threads_updated_at
                    BEFORE UPDATE ON threads
                    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

                CREATE TRIGGER trg_webhooks_updated_at
                    BEFORE UPDATE ON webhooks
                    FOR EACH ROW EXECUTE FUNCTION set_updated_at()
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
                DROP TRIGGER IF EXISTS trg_webhooks_updated_at ON webhooks;
                DROP TRIGGER IF EXISTS trg_threads_updated_at ON threads;
                DROP TRIGGER IF EXISTS trg_categories_updated_at ON categories;
                DROP TRIGGER IF EXISTS trg_user_preferences_updated_at ON user_preferences;
                DROP TRIGGER IF EXISTS trg_users_updated_at ON users;
                DROP FUNCTION IF EXISTS set_updated_at;
                DROP TRIGGER IF EXISTS trg_thread_search ON threads;
                DROP FUNCTION IF EXISTS update_thread_search_vector;
                DROP INDEX IF EXISTS idx_posts_thread;
                DROP INDEX IF EXISTS idx_threads_fts;
                DROP INDEX IF EXISTS idx_threads_category
                "#,
            )
            .await?;
        Ok(())
    }
}
