use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000023_fix_reports_integrity"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        conn.execute_unprepared(r#"
            -- Drop the old CASCADE foreign keys
            ALTER TABLE reports DROP CONSTRAINT IF EXISTS fk_reports_post_id;
            ALTER TABLE reports DROP CONSTRAINT IF EXISTS fk_reports_thread_id;

            -- Re-add as SET NULL so deleting content preserves the report record
            ALTER TABLE reports
                ADD CONSTRAINT fk_reports_post_id
                    FOREIGN KEY (post_id)   REFERENCES posts(id)    ON DELETE SET NULL,
                ADD CONSTRAINT fk_reports_thread_id
                    FOREIGN KEY (thread_id) REFERENCES threads(id)  ON DELETE SET NULL;

            -- Column to record when the reported content was hard-deleted
            ALTER TABLE reports ADD COLUMN target_deleted_at TIMESTAMPTZ;

            -- Drop the original XOR check (PostgreSQL auto-names it "reports_check")
            DO $$
            DECLARE cname text;
            BEGIN
                SELECT constraint_name INTO cname
                FROM information_schema.table_constraints
                WHERE table_name = 'reports'
                  AND constraint_type = 'CHECK'
                  AND constraint_name NOT LIKE 'chk_%';
                IF cname IS NOT NULL THEN
                    EXECUTE 'ALTER TABLE reports DROP CONSTRAINT ' || quote_ident(cname);
                END IF;
            END $$;

            -- New check: normal XOR, OR both-NULL when target was deleted
            ALTER TABLE reports ADD CONSTRAINT chk_reports_target CHECK (
                (post_id IS NOT NULL AND thread_id IS NULL)
             OR (post_id IS NULL    AND thread_id IS NOT NULL)
             OR (post_id IS NULL    AND thread_id IS NULL AND target_deleted_at IS NOT NULL)
            );

            -- Trigger: stamp target_deleted_at when FK is set to NULL by a cascade
            CREATE OR REPLACE FUNCTION reports_mark_target_deleted()
            RETURNS TRIGGER AS $$
            BEGIN
                IF (OLD.post_id   IS NOT NULL AND NEW.post_id   IS NULL)
                OR (OLD.thread_id IS NOT NULL AND NEW.thread_id IS NULL)
                THEN
                    NEW.target_deleted_at = now();
                END IF;
                RETURN NEW;
            END;
            $$ LANGUAGE plpgsql;

            CREATE TRIGGER trg_reports_target_deleted
            BEFORE UPDATE ON reports
            FOR EACH ROW EXECUTE FUNCTION reports_mark_target_deleted();

            -- Missing FK indexes (post_id, thread_id, reporter_id)
            CREATE INDEX idx_reports_reporter_id ON reports(reporter_id);
            CREATE INDEX idx_reports_post_id   ON reports(post_id)   WHERE post_id   IS NOT NULL;
            CREATE INDEX idx_reports_thread_id ON reports(thread_id) WHERE thread_id IS NOT NULL;
        "#).await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        conn.execute_unprepared(r#"
            DROP INDEX IF EXISTS idx_reports_thread_id;
            DROP INDEX IF EXISTS idx_reports_post_id;
            DROP INDEX IF EXISTS idx_reports_reporter_id;
            DROP TRIGGER  IF EXISTS trg_reports_target_deleted ON reports;
            DROP FUNCTION IF EXISTS reports_mark_target_deleted;
            ALTER TABLE reports DROP CONSTRAINT IF EXISTS chk_reports_target;
            ALTER TABLE reports DROP COLUMN  IF EXISTS target_deleted_at;
            ALTER TABLE reports DROP CONSTRAINT IF EXISTS fk_reports_post_id;
            ALTER TABLE reports DROP CONSTRAINT IF EXISTS fk_reports_thread_id;
            ALTER TABLE reports
                ADD CONSTRAINT fk_reports_post_id
                    FOREIGN KEY (post_id)   REFERENCES posts(id)    ON DELETE CASCADE,
                ADD CONSTRAINT fk_reports_thread_id
                    FOREIGN KEY (thread_id) REFERENCES threads(id)  ON DELETE CASCADE;
            ALTER TABLE reports ADD CONSTRAINT reports_check CHECK (
                (post_id IS NOT NULL AND thread_id IS NULL) OR
                (post_id IS NULL AND thread_id IS NOT NULL)
            );
        "#).await?;

        Ok(())
    }
}
