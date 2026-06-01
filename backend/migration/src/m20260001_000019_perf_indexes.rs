use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000019_perf_indexes"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
                -- Reactions: covers counts_by_post, counts_by_posts, find, remove
                CREATE INDEX IF NOT EXISTS idx_reactions_post
                    ON reactions(post_id);

                -- Reactions: covers user_reactions_for_posts
                CREATE INDEX IF NOT EXISTS idx_reactions_user_post
                    ON reactions(user_id, post_id);

                -- Notifications: covers unread_count, list_for_user, mark_all_read
                CREATE INDEX IF NOT EXISTS idx_notifications_user_read
                    ON notifications(user_id, is_read, created_at DESC);

                -- Threads: covers find_by_slug
                CREATE INDEX IF NOT EXISTS idx_threads_slug
                    ON threads(slug);

                -- Categories: covers find_by_slug
                CREATE INDEX IF NOT EXISTS idx_categories_slug
                    ON categories(slug);

                -- Threads: covers list_by_author
                CREATE INDEX IF NOT EXISTS idx_threads_author
                    ON threads(author_id, created_at DESC)
                    WHERE deleted_at IS NULL
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
                DROP INDEX IF EXISTS idx_threads_author;
                DROP INDEX IF EXISTS idx_categories_slug;
                DROP INDEX IF EXISTS idx_threads_slug;
                DROP INDEX IF EXISTS idx_notifications_user_read;
                DROP INDEX IF EXISTS idx_reactions_user_post;
                DROP INDEX IF EXISTS idx_reactions_post
                "#,
            )
            .await?;
        Ok(())
    }
}
