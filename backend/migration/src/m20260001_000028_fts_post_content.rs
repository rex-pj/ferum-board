use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000028_fts_post_content"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(r#"
                -- Update thread FTS trigger: title (A) + post content (B)
                CREATE OR REPLACE FUNCTION update_thread_search_vector()
                RETURNS TRIGGER AS $$
                BEGIN
                    NEW.search_vector :=
                        setweight(to_tsvector('simple', coalesce(NEW.title, '')), 'A') ||
                        setweight(to_tsvector('simple', (
                            SELECT coalesce(string_agg(content_md, ' '), '')
                            FROM posts
                            WHERE thread_id = NEW.id AND is_deleted = false
                        )), 'B');
                    RETURN NEW;
                END;
                $$ LANGUAGE plpgsql;

                -- Posts trigger: refresh parent thread's search_vector on content changes
                CREATE OR REPLACE FUNCTION update_thread_search_from_post()
                RETURNS TRIGGER AS $$
                DECLARE
                    v_thread_id UUID;
                BEGIN
                    v_thread_id := COALESCE(NEW.thread_id, OLD.thread_id);
                    UPDATE threads
                    SET search_vector = (
                        setweight(to_tsvector('simple', coalesce(title, '')), 'A') ||
                        setweight(to_tsvector('simple', (
                            SELECT coalesce(string_agg(content_md, ' '), '')
                            FROM posts
                            WHERE thread_id = v_thread_id AND is_deleted = false
                        )), 'B')
                    )
                    WHERE id = v_thread_id;
                    RETURN COALESCE(NEW, OLD);
                END;
                $$ LANGUAGE plpgsql;

                CREATE TRIGGER trg_post_update_thread_search
                AFTER INSERT OR UPDATE OF content_md, is_deleted OR DELETE ON posts
                FOR EACH ROW EXECUTE FUNCTION update_thread_search_from_post();

                -- Re-build all existing search vectors to include post content
                UPDATE threads t
                SET search_vector = (
                    setweight(to_tsvector('simple', coalesce(t.title, '')), 'A') ||
                    setweight(to_tsvector('simple', (
                        SELECT coalesce(string_agg(p.content_md, ' '), '')
                        FROM posts p
                        WHERE p.thread_id = t.id AND p.is_deleted = false
                    )), 'B')
                );
            "#)
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(r#"
                DROP TRIGGER  IF EXISTS trg_post_update_thread_search ON posts;
                DROP FUNCTION IF EXISTS update_thread_search_from_post;

                -- Restore original title-only trigger function
                CREATE OR REPLACE FUNCTION update_thread_search_vector()
                RETURNS TRIGGER AS $$
                BEGIN
                    NEW.search_vector :=
                        setweight(to_tsvector('simple', coalesce(NEW.title, '')), 'A');
                    RETURN NEW;
                END;
                $$ LANGUAGE plpgsql;

                -- Rebuild vectors with title only
                UPDATE threads
                SET search_vector =
                    setweight(to_tsvector('simple', coalesce(title, '')), 'A');
            "#)
            .await?;
        Ok(())
    }
}
