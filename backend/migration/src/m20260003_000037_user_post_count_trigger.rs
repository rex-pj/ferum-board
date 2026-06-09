use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260003_000037_user_post_count_trigger"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        conn.execute_unprepared(r#"
            CREATE OR REPLACE FUNCTION update_user_post_count()
            RETURNS TRIGGER AS $$
            BEGIN
                IF TG_OP = 'INSERT' THEN
                    IF NOT NEW.is_deleted AND NEW.status = 'published' THEN
                        UPDATE users SET post_count = post_count + 1 WHERE id = NEW.author_id;
                    END IF;

                ELSIF TG_OP = 'UPDATE' THEN
                    -- Transition: became published & not deleted  →  +1
                    IF (NEW.status = 'published' AND NOT NEW.is_deleted)
                       AND NOT (OLD.status = 'published' AND NOT OLD.is_deleted)
                    THEN
                        UPDATE users SET post_count = post_count + 1 WHERE id = NEW.author_id;

                    -- Transition: was published & not deleted  →  now deleted/pending  →  -1
                    ELSIF (OLD.status = 'published' AND NOT OLD.is_deleted)
                          AND NOT (NEW.status = 'published' AND NOT NEW.is_deleted)
                    THEN
                        UPDATE users SET post_count = GREATEST(0, post_count - 1) WHERE id = NEW.author_id;
                    END IF;

                ELSIF TG_OP = 'DELETE' THEN
                    IF NOT OLD.is_deleted AND OLD.status = 'published' THEN
                        UPDATE users SET post_count = GREATEST(0, post_count - 1) WHERE id = OLD.author_id;
                    END IF;
                END IF;

                RETURN NULL;
            END;
            $$ LANGUAGE plpgsql;

            CREATE TRIGGER trg_user_post_count
            AFTER INSERT OR UPDATE OF is_deleted, status OR DELETE ON posts
            FOR EACH ROW EXECUTE FUNCTION update_user_post_count();

            -- Back-fill post_count for all existing users
            UPDATE users u
            SET post_count = COALESCE((
                SELECT COUNT(*)
                FROM posts p
                WHERE p.author_id = u.id
                  AND p.is_deleted = false
                  AND p.status = 'published'
            ), 0);
        "#).await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();
        conn.execute_unprepared(r#"
            DROP TRIGGER  IF EXISTS trg_user_post_count ON posts;
            DROP FUNCTION IF EXISTS update_user_post_count;
        "#).await?;
        Ok(())
    }
}
