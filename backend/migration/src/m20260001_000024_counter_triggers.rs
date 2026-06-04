use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000024_counter_triggers"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        // ── 1. threads.reply_count + threads.last_post_at ────────────────────
        conn.execute_unprepared(r#"
            CREATE OR REPLACE FUNCTION update_thread_reply_count()
            RETURNS TRIGGER AS $$
            BEGIN
                IF TG_OP = 'INSERT' AND NOT NEW.is_deleted THEN
                    UPDATE threads
                    SET reply_count = reply_count + 1,
                        last_post_at = GREATEST(last_post_at, NEW.created_at)
                    WHERE id = NEW.thread_id;

                ELSIF TG_OP = 'UPDATE' THEN
                    IF NOT OLD.is_deleted AND NEW.is_deleted THEN
                        -- post was just soft-deleted
                        UPDATE threads
                        SET reply_count = GREATEST(reply_count - 1, 0)
                        WHERE id = NEW.thread_id;
                    ELSIF OLD.is_deleted AND NOT NEW.is_deleted THEN
                        -- post was just un-deleted
                        UPDATE threads
                        SET reply_count = reply_count + 1
                        WHERE id = NEW.thread_id;
                    END IF;

                ELSIF TG_OP = 'DELETE' AND NOT OLD.is_deleted THEN
                    UPDATE threads
                    SET reply_count = GREATEST(reply_count - 1, 0)
                    WHERE id = OLD.thread_id;
                END IF;

                RETURN NULL;
            END;
            $$ LANGUAGE plpgsql;

            CREATE TRIGGER trg_posts_reply_count
            AFTER INSERT OR UPDATE OF is_deleted OR DELETE ON posts
            FOR EACH ROW EXECUTE FUNCTION update_thread_reply_count();

            -- Back-fill reply_count and last_post_at from existing data
            UPDATE threads t
            SET
                reply_count  = s.cnt,
                last_post_at = s.latest
            FROM (
                SELECT
                    thread_id,
                    COUNT(*)        AS cnt,
                    MAX(created_at) AS latest
                FROM posts
                WHERE is_deleted = false
                GROUP BY thread_id
            ) s
            WHERE t.id = s.thread_id;
        "#).await?;

        // ── 2. stored_files.ref_count via user_avatars + thread_thumbnails ───
        conn.execute_unprepared(r#"
            CREATE OR REPLACE FUNCTION sync_file_ref_count()
            RETURNS TRIGGER AS $$
            BEGIN
                IF TG_OP = 'INSERT' THEN
                    UPDATE stored_files SET ref_count = ref_count + 1
                    WHERE key = NEW.file_key;
                    RETURN NEW;

                ELSIF TG_OP = 'UPDATE' AND NEW.file_key IS DISTINCT FROM OLD.file_key THEN
                    UPDATE stored_files SET ref_count = GREATEST(ref_count - 1, 0)
                    WHERE key = OLD.file_key;
                    UPDATE stored_files SET ref_count = ref_count + 1
                    WHERE key = NEW.file_key;
                    RETURN NEW;

                ELSIF TG_OP = 'DELETE' THEN
                    UPDATE stored_files SET ref_count = GREATEST(ref_count - 1, 0)
                    WHERE key = OLD.file_key;
                    RETURN OLD;
                END IF;

                RETURN NEW;
            END;
            $$ LANGUAGE plpgsql;

            CREATE TRIGGER trg_user_avatars_ref_count
            AFTER INSERT OR UPDATE OR DELETE ON user_avatars
            FOR EACH ROW EXECUTE FUNCTION sync_file_ref_count();

            CREATE TRIGGER trg_thread_thumbnails_ref_count
            AFTER INSERT OR UPDATE OR DELETE ON thread_thumbnails
            FOR EACH ROW EXECUTE FUNCTION sync_file_ref_count();

            -- Back-fill ref_count from existing links
            UPDATE stored_files sf
            SET ref_count = (
                SELECT COUNT(*) FROM user_avatars     WHERE file_key = sf.key
            ) + (
                SELECT COUNT(*) FROM thread_thumbnails WHERE file_key = sf.key
            );
        "#).await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        conn.execute_unprepared(r#"
            DROP TRIGGER  IF EXISTS trg_thread_thumbnails_ref_count ON thread_thumbnails;
            DROP TRIGGER  IF EXISTS trg_user_avatars_ref_count      ON user_avatars;
            DROP FUNCTION IF EXISTS sync_file_ref_count;
            DROP TRIGGER  IF EXISTS trg_posts_reply_count ON posts;
            DROP FUNCTION IF EXISTS update_thread_reply_count;
        "#).await?;

        Ok(())
    }
}
