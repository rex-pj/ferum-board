use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000027_slug_length_constraints"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(r#"
                ALTER TABLE threads
                    ADD CONSTRAINT chk_threads_slug_len
                    CHECK (char_length(slug) BETWEEN 1 AND 255);

                ALTER TABLE categories
                    ADD CONSTRAINT chk_categories_slug_len
                    CHECK (char_length(slug) BETWEEN 1 AND 255);

                ALTER TABLE tags
                    ADD CONSTRAINT chk_tags_name_len
                    CHECK (char_length(name) BETWEEN 1 AND 100),
                    ADD CONSTRAINT chk_tags_slug_len
                    CHECK (char_length(slug) BETWEEN 1 AND 100);

                ALTER TABLE roles
                    ADD CONSTRAINT chk_roles_slug_len
                    CHECK (char_length(slug) BETWEEN 1 AND 100);

                ALTER TABLE permissions
                    ADD CONSTRAINT chk_permissions_key_len
                    CHECK (char_length(key) BETWEEN 1 AND 100);
            "#)
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(r#"
                ALTER TABLE permissions DROP CONSTRAINT IF EXISTS chk_permissions_key_len;
                ALTER TABLE roles       DROP CONSTRAINT IF EXISTS chk_roles_slug_len;
                ALTER TABLE tags        DROP CONSTRAINT IF EXISTS chk_tags_slug_len;
                ALTER TABLE tags        DROP CONSTRAINT IF EXISTS chk_tags_name_len;
                ALTER TABLE categories  DROP CONSTRAINT IF EXISTS chk_categories_slug_len;
                ALTER TABLE threads     DROP CONSTRAINT IF EXISTS chk_threads_slug_len;
            "#)
            .await?;
        Ok(())
    }
}
