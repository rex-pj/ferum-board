use sea_orm::Statement;
use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000034_seed_languages_permission"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();
        let backend = manager.get_database_backend();

        // Its own key rather than folding into `admin.config`: managing languages
        // is the one admin task a community translator plausibly needs, and it
        // should not require also granting them SMTP credentials and the
        // registration switch.
        conn.execute(Statement::from_string(
            backend,
            "INSERT INTO permissions (key, description, group_name, min_trust) \
             VALUES ('admin.languages', 'Manage site languages and translations', 'admin', 'new'::trust_level) \
             ON CONFLICT (key) DO NOTHING"
                .to_owned(),
        ))
        .await?;

        // Admin only by default. A site that wants a dedicated translator role
        // can grant it from the permission matrix without a code change.
        conn.execute(Statement::from_string(
            backend,
            "INSERT INTO role_permissions (role_id, permission_id) \
             SELECT r.id, p.id FROM roles r, permissions p \
             WHERE r.slug = 'admin' AND p.key = 'admin.languages' \
             ON CONFLICT (role_id, permission_id) DO NOTHING"
                .to_owned(),
        ))
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();
        conn.execute(Statement::from_string(
            manager.get_database_backend(),
            "DELETE FROM permissions WHERE key = 'admin.languages'".to_owned(),
        ))
        .await?;
        Ok(())
    }
}
