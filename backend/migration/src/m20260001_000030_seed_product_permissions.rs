use sea_orm::Statement;
use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000030_seed_product_permissions"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();
        let backend = manager.get_database_backend();

        // Seed the catalog-curation permission (idempotent).
        conn.execute(Statement::from_string(
            backend,
            "INSERT INTO permissions (key, description, group_name, min_trust) \
             VALUES ('product.manage', 'Manage the product / material catalog', 'catalog', 'new'::trust_level) \
             ON CONFLICT (key) DO NOTHING"
                .to_owned(),
        ))
        .await?;

        // Grant it to the admin system role.
        conn.execute(Statement::from_string(
            backend,
            "INSERT INTO role_permissions (role_id, permission_id) \
             SELECT r.id, p.id FROM roles r, permissions p \
             WHERE r.slug = 'admin' AND p.key = 'product.manage' \
             ON CONFLICT (role_id, permission_id) DO NOTHING"
                .to_owned(),
        ))
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();
        // Deleting the permission cascades to role_permissions.
        conn.execute(Statement::from_string(
            manager.get_database_backend(),
            "DELETE FROM permissions WHERE key = 'product.manage'".to_owned(),
        ))
        .await?;
        Ok(())
    }
}
