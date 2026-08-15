use sea_orm_migration::prelude::*;
use sea_orm::ConnectionTrait;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000032_index_users_username_lower"
    }
}

// Enforces and supports case-insensitive usernames — the repository compares
// `lower(username)`, which the UNIQUE b-tree on `username` cannot serve.
//
// The expression must match the query character-for-character, or Postgres
// silently declines the index and every profile page and mention lookup
// sequentially scans `users`.
//
// UNIQUE fails outright on a database already holding a collision, and that
// blocks startup since migrations run at boot. To audit before upgrading:
//     SELECT lower(username), count(*), array_agg(username)
//     FROM users GROUP BY 1 HAVING count(*) > 1;
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();
        // Drop the non-unique form first. An earlier revision of *this* file
        // created it, so a database migrated from that revision already has the
        // name taken by a plain index — and `CREATE UNIQUE INDEX IF NOT EXISTS`
        // would then find the name present and quietly do nothing, leaving the
        // constraint absent while `seaql_migrations` records it as applied.
        conn.execute_unprepared("DROP INDEX IF EXISTS idx_users_username_lower")
            .await?;
        conn.execute_unprepared(
            "CREATE UNIQUE INDEX idx_users_username_lower ON users (lower(username))",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP INDEX IF EXISTS idx_users_username_lower")
            .await?;
        Ok(())
    }
}
