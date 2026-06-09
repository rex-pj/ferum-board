use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260002_000034_seed_plugin_permission"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        // Insert admin.plugins permission
        conn.execute_unprepared(r#"
            INSERT INTO permissions (id, key, description, group_name, min_trust)
            VALUES (gen_random_uuid(), 'admin.plugins', 'Install and manage plugins', 'admin', 'new')
            ON CONFLICT (key) DO NOTHING;
        "#)
        .await?;

        // Grant to admin role (admin gets all permissions — same pattern as migration 20)
        conn.execute_unprepared(r#"
            INSERT INTO role_permissions (role_id, permission_id)
            SELECT r.id, p.id
            FROM roles r, permissions p
            WHERE r.slug = 'admin'
              AND p.key = 'admin.plugins'
            ON CONFLICT DO NOTHING;
        "#)
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        conn.execute_unprepared(r#"
            DELETE FROM role_permissions
            WHERE permission_id = (SELECT id FROM permissions WHERE key = 'admin.plugins');
            DELETE FROM permissions WHERE key = 'admin.plugins';
        "#)
        .await?;

        Ok(())
    }
}
