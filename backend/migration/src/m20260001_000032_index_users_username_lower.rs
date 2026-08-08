use sea_orm_migration::prelude::*;
use sea_orm::ConnectionTrait;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000032_index_users_username_lower"
    }
}

// Enforces and supports case-insensitive usernames.
//
// `validate_username` admits any alphanumeric, so usernames carry case, but
// `extract_mentions` lowercases what it captures before looking the user up.
// While the lookup compared with `=`, `@TrungLe` matched no row and the mention
// notification was never created — silently, since "no such user" and "user
// found" take the same code path. The same lookup backs `register`'s uniqueness
// check, so `Alice` could also be registered alongside an existing `alice`.
//
// The repository now compares `lower(username)`. Without this index that turns
// the UNIQUE b-tree on `username` into dead weight for the query and every
// profile page, mention resolution and registration does a sequential scan of
// `users` — so the index is part of the fix, not an optimisation on top of it.
// The expression is written exactly as the query builds it; a difference of a
// single function call and Postgres silently declines to use it.
//
// ── UNIQUE, and what that costs ──────────────────────────────────────────────
//
// Unique because the application check alone only stops the *next* collision:
// it cannot remove a pair that already exists, and while one exists
// `find_by_username` resolves to whichever row Postgres happens to return
// first — non-deterministically, so the same login form can reach two different
// accounts on two requests. Only the constraint makes the invariant true rather
// than merely intended.
//
// `CREATE UNIQUE INDEX` fails outright on a database that already holds such a
// pair, and that failure blocks startup because migrations run at boot. That is
// the correct trade for this project, which is pre-release and whose data is
// disposable — but it is a real edge for anyone restoring an old dump, so the
// audit and the repair are written out here rather than left to be rediscovered
// from a stack trace:
//
//     -- what collides
//     SELECT lower(username), count(*), array_agg(username)
//     FROM users GROUP BY 1 HAVING count(*) > 1;
//
//     -- one way to repair: keep the oldest, suffix the rest
//     UPDATE users u SET username = u.username || '-' || left(u.id::text, 4)
//     WHERE EXISTS (
//         SELECT 1 FROM users o
//         WHERE lower(o.username) = lower(u.username)
//           AND o.created_at < u.created_at
//     );
//
// Renaming somebody's account is a product decision, so the repair is offered
// and not performed.
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
