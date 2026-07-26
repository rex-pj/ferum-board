# Database Entities & Integration Testing

Infrastructure for keeping repository SQL **strongly typed against the schema**,
so that a column/table rename is a compile error rather than a runtime failure.

## Why

Repository queries are built from generated Sea-ORM entity columns
(`users::Column::CreatedAt`, …). For that to stay honest, two things must hold:

1. **Entities track the schema.** When a migration changes a column, the entity
   must be regenerated so the `Column` variant changes — breaking every repo that
   referenced it, at compile time.
2. **Residual raw SQL is exercised.** Some PostgreSQL constructs have no
   sea-query AST node and stay as `Expr::cust` (enum-literal casts like
   `'deleted'::thread_status`, `COUNT(*) FILTER`, `LATERAL`, `tsvector`,
   `ANY(array)`, `generate_series`). The compiler cannot check the column names
   inside those strings, so integration tests run each query against a real DB.

## The loop

```
edit migration  →  regen entities  →  repo uses entity::Column  →  rename = compile error
                         │
                         └── scripts/regen-entities.ps1   (Windows, primary)
                             scripts/regen-entities.sh    (bash)
```

One-time tool install:

```powershell
cargo install sea-orm-cli --version '^1.1'
```

Regenerate after changing any migration:

```powershell
# from repo root
./scripts/regen-entities.ps1
git diff -- backend/crates/ferum-infrastructure/src/entities/   # review
cargo check --manifest-path backend/Cargo.toml                  # fix repo fallout
```

The script resets a throwaway `ferum_board_regen` database, migrates it, and runs
`sea-orm-cli generate entity --with-serde both --date-time-crate chrono` in place.
The entities are pristine generator output, so `git diff` is the review surface —
keep the tree otherwise clean when running it.

## Integration tests

The harness ([`crates/ferum-infrastructure/tests/common/mod.rs`](../backend/crates/ferum-infrastructure/tests/common/mod.rs))
provisions an isolated database per test, migrates it, and tears it down.

```powershell
# requires a reachable PostgreSQL (TEST_DATABASE_URL, else DATABASE_URL from backend/.env)
cargo test -p ferum-infrastructure --features db-tests
```

The tests are behind the `db-tests` feature, so the default `cargo test` stays
green on machines without a database.

### Writing one

```rust
#![cfg(feature = "db-tests")]
mod common;
use common::TestDb;

#[tokio::test]
async fn my_repo_query_runs() {
    let db = TestDb::new("unique_label").await;   // ferum_test_unique_label
    let repo = PgSomeRepository::new(db.conn.clone());

    // ... seed rows via the repo / entities, then assert on a query ...

    db.teardown().await;                           // drop the throwaway DB
}
```

`label` must be unique per test (it names the database) so `cargo test` can run
them in parallel. A crashed run leaves a stale `ferum_test_*` DB; the next run
with the same label reclaims it via `DROP DATABASE IF EXISTS`.

## Migrations

Existing migrations are **not** converted to the schema builder: they are the
schema's source of truth (not a consumer of it), and much of the DDL is
PostgreSQL-specific (`CREATE TYPE` enums, triggers/functions, partial indexes,
generated columns, `tsvector`/GIN). Convention for **new** migrations: use the
schema builder (`Table::create()` with `Iden`) for plain DDL, raw
`execute_unprepared` only for PostgreSQL-specific features.

Migrations carry **DDL only**. Rows the application needs in order to boot —
system roles, permissions and their first-install grants, `site_config`
defaults, the built-in theme, the catalogue taxonomy — are written by
`PgSystemSeedService`, idempotently, on every startup. A permission is therefore
defined once, in `ferum-domain::models::role::PERMISSIONS`, next to the `perm::`
constant the code checks against; adding one needs no migration. The test
harness runs the same seeder after `Migrator::up`, which is why repository tests
can assert on seeded roles.

## Status

This is the foundation step of the repository strong-typing effort. Remaining
work — converting the raw-SQL repositories (`thread_repository`, read-model
queries, small UPDATEs) to entity-typed sea-query — is tracked separately; each
converted repo should land with an integration test using this harness.
