# Database Entities & Integration Testing

Infrastructure for keeping repository SQL **strongly typed against the schema**, so
that a column or table rename is a compile error rather than a runtime failure.

## Why

Repository queries are built from generated Sea-ORM entity columns
(`users::Column::CreatedAt`, …). For that to stay honest, two things must hold:

1. **Entities track the schema.** When a migration changes a column, the entity must be
   regenerated so the `Column` variant changes — breaking every repository that
   referenced it, at compile time.
2. **Residual raw SQL is exercised.** Some PostgreSQL constructs have no sea-query AST
   node and stay as `Expr::cust` (enum-literal casts like `'deleted'::thread_status`,
   `COUNT(*) FILTER`, `LATERAL`, `tsvector`, `ANY(array)`, `generate_series`). The
   compiler cannot check column names inside those strings, so integration tests run
   each query against a real database.

## The regeneration loop

```
edit migration  →  regen entities  →  repo uses entity::Column  →  rename = compile error
                         │
                         └── scripts/regen-entities.ps1   (Windows, primary)
                             scripts/regen-entities.sh    (bash)
```

One-time tool install:

```powershell
cargo install sea-orm-cli --version '^2.0'
```

The version must track the `sea-orm` major in `backend/Cargo.toml` (currently 2).

Regenerate after changing any migration:

```powershell
# from the repository root
./scripts/regen-entities.ps1
git diff -- backend/crates/ferum-infrastructure/src/entities/   # review
cargo check --manifest-path backend/Cargo.toml                  # fix repo fallout
```

The script resets a throwaway `ferum_board_regen` database, migrates it with
`cargo run -p migration --features cli -- fresh`, then runs:

```
sea-orm-cli generate entity \
    --entity-format dense \
    --with-serde both \
    --date-time-crate chrono
```

`--entity-format dense` is a Sea-ORM 2.0 output mode: relations are folded onto `Model`
as typed `BelongsTo`/`HasMany` fields instead of a separate `Relation` enum with
hand-written `impl Related<..>` blocks.

The entities are pristine generator output, so `git diff` is the review surface — keep
the tree otherwise clean when running it.

## Integration tests

Test code lives in `backend/tests/`, as five workspace members rather than
`#[cfg(test)]` modules inside the crates:

| Member | Needs a database |
| --- | --- |
| `tests/support` | no — shared fixtures and `mockall` mocks |
| `tests/domain` | no |
| `tests/application` | no — use cases run against mock repositories |
| `tests/infrastructure` | **yes** |
| `tests/web` | no |

```powershell
cd backend
cargo test --workspace
```

`--workspace` is mandatory. `default-members = ["crates/ferum-web"]` (set so a bare
`cargo run` builds the server) also narrows what `cargo test` selects: without it,
cargo runs ferum-web's single unit test, prints `ok`, and executes none of the suites
above.

There is no `db-tests` cargo feature. `tests/infrastructure` is a normal member and
runs by default, so a machine with no reachable PostgreSQL cannot complete a
`--workspace` run — and it does not fail fast, it stalls. To work on the rest:

```powershell
cargo test -p ferum-domain-tests -p ferum-application-tests -p ferum-web-tests
```

Those three need no database: `tests/application` drives use cases against `mockall`
mocks from `tests/support`.

### The harness

[`backend/tests/infrastructure/src/common.rs`](../backend/tests/infrastructure/src/common.rs)
provisions an isolated database per test.

Migrations run **once**, against a shared template database (`ferum_test_template`).
Each per-test database is then created with
`CREATE DATABASE … TEMPLATE ferum_test_template`, so PostgreSQL copies pages at the
filesystem level instead of replaying the whole migration chain. Setup cost goes from
O(migrations × N) to O(migrations + N × template-copy).

The base URL comes from `TEST_DATABASE_URL`, falling back to `DATABASE_URL` from
`backend/.env`. The harness **refuses to run against a non-local host** — the URL must
contain `localhost` or `127.0.0.1` — because it creates and drops databases.

### Writing one

```rust
use crate::common::TestDb;

#[tokio::test]
async fn my_repo_query_runs() {
    let db = TestDb::new("unique_label").await;   // → database ferum_test_unique_label
    let repo = PgSomeRepository::new(db.conn.clone());

    // ... seed rows via the repo / fixtures, then assert on a query ...

    db.teardown().await;                          // drop the throwaway database
}
```

`label` must be unique per test — it names the database, so `cargo test` can run them
in parallel. A crashed run leaves a stale `ferum_test_*` database; the next run with
the same label reclaims it via `DROP DATABASE IF EXISTS`.

### Timezone-sensitive queries

```rust
let db = TestDb::new_in_timezone("daily_stats_buckets", "Asia/Kathmandu").await;
```

This forces a hostile session `TimeZone`. A query whose result changes with that
argument is reading a day boundary it should have named explicitly — see the "Time and
Timezones" rules. Note the constraint: the returned `TestDb` holds a
single-connection pool, so such a test must issue its queries sequentially.

Ordinary `TestDb::new` sessions run at `TimeZone=UTC`, not because the harness sets it
but because `sqlx-postgres` puts `("TimeZone", "UTC")` into every startup packet.

CI runs the whole suite twice, under `TZ=UTC` and `TZ=Asia/Kathmandu`. The second is
chosen because UTC+05:45 is not a whole number of hours, which catches code that
truncates instead of converting.

## Migrations

Existing migrations are **not** converted to the schema builder: they are the schema's
source of truth (not a consumer of it), and much of the DDL is PostgreSQL-specific
(`CREATE TYPE` enums, functions, partial indexes, generated columns, `tsvector`/GIN).
Convention for **new** migrations: use the schema builder (`Table::create()` with
`Iden`) for plain DDL, and `execute_unprepared` only for PostgreSQL-specific features.

Migrations carry **DDL only**. Rows the application needs in order to boot — system
roles, permissions and their first-install grants, `site_config` defaults, the built-in
theme row, the catalogue taxonomy — are written by `PgSystemSeedService`, idempotently,
on every startup. A permission is therefore defined once, in
`ferum-domain::models::role::PERMISSIONS`, next to the `perm::` constant the code
checks against; adding one needs no migration. The test harness runs the same seeder
after `Migrator::up`, which is why repository tests can assert on seeded roles.

The server applies migrations itself at startup (`Migrator::up`). The standalone CLI is
for the regeneration loop and for running them out of band:

```powershell
cd backend
cargo run -p migration --features cli -- up
cargo run -p migration --features cli -- fresh
```

`--features cli` is required — the bin target sits behind `required-features` so the
server binary does not link an argument parser.

## Status

The strong-typing effort is partial. Several repositories still build their queries as
raw SQL (`thread_repository`, the read-model queries, some small `UPDATE`s) rather than
from entity columns; those are the ones the integration suite covers by execution
instead of by type. Converting one should land together with a test using the harness
above.
