use sea_orm::Statement;
use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000031_seed_product_submit_permission"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();
        let backend = manager.get_database_backend();

        // Crowd-sourced contribution permission: any member may PROPOSE a product
        // (it lands as `draft`); only `product.manage` (admin) can publish it.
        // min_trust 'basic' = email verified — keeps throwaway bots out.
        conn.execute(Statement::from_string(
            backend,
            "INSERT INTO permissions (key, description, group_name, min_trust) \
             VALUES ('product.submit', 'Submit a product to the catalog for review', 'catalog', 'basic'::trust_level) \
             ON CONFLICT (key) DO NOTHING"
                .to_owned(),
        ))
        .await?;

        // Grant to the member, moderator and admin system roles.
        conn.execute(Statement::from_string(
            backend,
            "INSERT INTO role_permissions (role_id, permission_id) \
             SELECT r.id, p.id FROM roles r, permissions p \
             WHERE r.slug IN ('member', 'moderator', 'admin') AND p.key = 'product.submit' \
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
            "DELETE FROM permissions WHERE key = 'product.submit'".to_owned(),
        ))
        .await?;
        Ok(())
    }
}
