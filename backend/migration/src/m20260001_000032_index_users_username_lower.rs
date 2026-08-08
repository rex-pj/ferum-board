use sea_orm_migration::prelude::*;
use sea_orm::ConnectionTrait;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000032_index_users_username_lower"
    }
}

// Supports the case-insensitive `find_by_username`.
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
// ── Deliberately NOT UNIQUE ──────────────────────────────────────────────────
//
// A unique index here is the right end state — it is what would actually stop
// `Alice` and `alice` from coexisting, rather than merely stopping the *next*
// one from being created. It is not done here because `CREATE UNIQUE INDEX`
// fails outright on any database that already contains such a pair, which would
// turn a routine deploy into a failed one with the application refusing to
// start. Adding it needs a collision audit first:
//
//     SELECT lower(username), count(*), array_agg(username)
//     FROM users GROUP BY 1 HAVING count(*) > 1;
//
// and a decision about what to do with whatever it returns — renaming an
// account is not something a migration should choose on an operator's behalf.
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE INDEX IF NOT EXISTS idx_users_username_lower \
                 ON users (lower(username))",
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
