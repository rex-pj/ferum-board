//! Integration tests for [`PgPluginDbGateway`], the Tier-2 plugin SQL gateway.
//! The only file driving `sea_orm::sqlx` directly, so the only place the sqlx
//! 0.9 API is exercised against a real server.
//!
//! **Split by layer on purpose**: `validate_plugin_sql` (denylist,
//! defence-in-depth) and the `SET LOCAL ROLE` privilege drop (the real
//! boundary). Conflating them is how a regression hides — a gap in one is not
//! exploitable while the other holds, but losing either should fail the build.

use ferum_domain::repositories::plugin_db_repository::PluginDbGateway;
use ferum_infrastructure::repositories::PgPluginDbGateway;
use serde_json::json;

use crate::common::TestDb;

/// A plugin schema with one table, ready to query.
async fn provisioned(db: &TestDb, slug: &str) -> PgPluginDbGateway {
    let gw = PgPluginDbGateway::new(db.conn.clone());
    gw.provision_schema(
        slug,
        &[
            "CREATE TABLE IF NOT EXISTS notes (id uuid PRIMARY KEY, body text NOT NULL, n int NOT NULL DEFAULT 0)"
                .to_string(),
        ],
    )
    .await
    .expect("provision_schema");
    gw
}

// ─── provisioning ──────────────────────────────────────────────────────────

#[tokio::test]
async fn provision_schema_creates_the_schema_and_its_tables() {
    let db = TestDb::new("pdb_provision").await;
    let gw = provisioned(&db, "notes-plugin").await;

    // The plugin can see its own table through the pinned search_path, without
    // ever naming the schema (which the validator would reject anyway).
    let out = gw
        .query("notes-plugin", "SELECT to_json(count(*)) FROM notes", vec![])
        .await
        .expect("query own table");
    assert_eq!(out, json!(0));

    db.teardown().await;
}

#[tokio::test]
async fn provision_schema_is_idempotent() {
    let db = TestDb::new("pdb_reprovision").await;
    let _ = provisioned(&db, "notes-plugin").await;
    // Re-running install-time DDL must not error — a reinstall or an
    // interrupted first run has to be able to finish.
    let _ = provisioned(&db, "notes-plugin").await;
    db.teardown().await;
}

#[tokio::test]
async fn a_dotted_slug_is_sanitised_into_a_legal_schema_name() {
    // Slugs are reverse-domain strings; `schema_name()` folds every
    // non-alphanumeric to `_`. If that broke, this would fail at CREATE SCHEMA.
    let db = TestDb::new("pdb_dotted").await;
    let gw = provisioned(&db, "com.ferum.simple-chatbox").await;
    let out = gw
        .query("com.ferum.simple-chatbox", "SELECT to_json(count(*)) FROM notes", vec![])
        .await
        .expect("query");
    assert_eq!(out, json!(0));
    db.teardown().await;
}

#[tokio::test]
async fn drop_schema_removes_everything() {
    let db = TestDb::new("pdb_drop").await;
    let gw = provisioned(&db, "notes-plugin").await;
    gw.drop_schema("notes-plugin").await.expect("drop_schema");

    // The table is gone, so the query fails rather than silently returning null.
    let err = gw
        .query("notes-plugin", "SELECT to_json(count(*)) FROM notes", vec![])
        .await;
    assert!(err.is_err(), "querying a dropped schema must fail");

    db.teardown().await;
}

// ─── query execution + parameter binding ───────────────────────────────────
//
// These are what actually cover the sqlx 0.9 rewrite: every statement below
// goes through `AssertSqlSafe`, and every parameter through
// `bind_scalar_param` / `bind_exec_param`.

#[tokio::test]
async fn a_write_returns_rows_affected_and_a_read_returns_json() {
    let db = TestDb::new("pdb_rw").await;
    let gw = provisioned(&db, "notes-plugin").await;
    let id = uuid::Uuid::new_v4();

    let wrote = gw
        .query(
            "notes-plugin",
            "INSERT INTO notes (id, body) VALUES ($1, $2)",
            vec![json!(id.to_string()), json!("hello")],
        )
        .await
        .expect("insert");
    assert_eq!(wrote, json!({ "rows_affected": 1 }));

    let read = gw
        .query(
            "notes-plugin",
            "SELECT to_json(body) FROM notes WHERE id = $1",
            vec![json!(id.to_string())],
        )
        .await
        .expect("select");
    assert_eq!(read, json!("hello"));

    db.teardown().await;
}

#[tokio::test]
async fn a_uuid_shaped_string_binds_as_a_uuid_not_as_text() {
    // The binder parses UUID-shaped strings and binds them with the uuid type.
    // Postgres sends an explicit type OID for bound parameters and will not
    // implicitly cast text to uuid in a comparison, so binding this as text
    // makes the WHERE clause error out. Every plugin id crossing this boundary
    // arrives as a JSON string, which is why the special case exists.
    let db = TestDb::new("pdb_uuidbind").await;
    let gw = provisioned(&db, "notes-plugin").await;
    let id = uuid::Uuid::new_v4();

    gw.query(
        "notes-plugin",
        "INSERT INTO notes (id, body) VALUES ($1, 'x')",
        vec![json!(id.to_string())],
    )
    .await
    .expect("insert with uuid param");

    // No `::uuid` cast in the SQL — the bind has to carry the type itself.
    let found = gw
        .query(
            "notes-plugin",
            "SELECT to_json(count(*)) FROM notes WHERE id = $1",
            vec![json!(id.to_string())],
        )
        .await
        .expect("select by uuid param");
    assert_eq!(found, json!(1));

    db.teardown().await;
}

#[tokio::test]
async fn integer_bool_null_and_non_uuid_text_params_all_round_trip() {
    let db = TestDb::new("pdb_paramkinds").await;
    let gw = provisioned(&db, "notes-plugin").await;

    // Exercises the i64, bool, null and plain-string arms of the binder.
    let out = gw
        .query(
            "notes-plugin",
            "SELECT to_json(ARRAY[$1::text, $2::text, $3::text, $4::text])",
            vec![json!(42), json!(true), json!(serde_json::Value::Null), json!("not-a-uuid")],
        )
        .await
        .expect("bind mixed params");

    assert_eq!(out, json!(["42", "true", null, "not-a-uuid"]));

    db.teardown().await;
}

/// A `statement_timeout` set through the `options` parameter of the connection
/// URL actually reaches the server.
///
/// This is the mechanism the application-wide ceiling relies on, and it is worth
/// pinning rather than assuming: `options` is a libpq convention that the driver
/// has to forward deliberately. If a future sqlx were to drop it, every query in
/// the process would silently become unbounded again — the failure is invisible,
/// because nothing errors, things merely stop being capped.
#[tokio::test]
async fn statement_timeout_can_be_set_through_the_connection_url() {
    use sea_orm::{ConnectOptions, Database, FromQueryResult};

    // Resolved the same way `TestDb` does; this test needs only *a* server, not
    // a scratch database, so it connects to the configured one directly.
    let _ = dotenvy::from_filename("../../.env");
    let _ = dotenvy::dotenv();
    let base = std::env::var("TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .expect("TEST_DATABASE_URL or DATABASE_URL must be set");
    let sep = if base.contains('?') { '&' } else { '?' };
    let url = format!("{base}{sep}options=-c%20statement_timeout%3D7000");

    let conn = Database::connect(ConnectOptions::new(url))
        .await
        .expect("connect with an options= parameter must succeed");

    let rows: Vec<sea_orm::JsonValue> = sea_orm::JsonValue::find_by_statement(
        sea_orm::Statement::from_string(sea_orm::DbBackend::Postgres, "SHOW statement_timeout"),
    )
    .all(&conn)
    .await
    .expect("SHOW statement_timeout");

    assert_eq!(
        rows[0]["statement_timeout"].as_str(),
        Some("7s"),
        "the driver did not forward `options` to the server — the app-wide \
         statement_timeout would be silently absent"
    );
}

/// A runaway plugin query is cancelled by Postgres, not left holding a pooled
/// connection — the only thing that actually bounds plugin SQL, since the
/// registry's tokio timeout cannot reach a thread inside `block_on`.
///
/// Against a real server because the gateway fails CLOSED: a syntax error in
/// that statement breaks every `Ferum.db.query` at once.
///
/// Burns rows rather than `pg_sleep`, which the validator's denylist blocks.
#[tokio::test]
async fn a_runaway_query_is_cancelled_instead_of_holding_the_connection() {
    let db = TestDb::new("pdb_stmt_timeout").await;
    let gw = provisioned(&db, "notes-plugin").await;

    let started = std::time::Instant::now();
    let result = gw
        .query(
            "notes-plugin",
            "SELECT to_json(count(*)) FROM generate_series(1, 20000000000)",
            vec![],
        )
        .await;
    let elapsed = started.elapsed();

    assert!(
        result.is_err(),
        "a query far longer than the ceiling must be cancelled, got {result:?}"
    );
    // Generous upper bound — the point is that it returns at all rather than
    // running for the minutes that query would otherwise take.
    assert!(
        elapsed < std::time::Duration::from_secs(15),
        "cancellation took {elapsed:?}; the statement timeout does not appear to be in force"
    );

    db.teardown().await;
}

/// The timeout is transaction-scoped, so it must not follow the connection back
/// into the pool and cut short an unrelated caller's legitimate query.
#[tokio::test]
async fn the_statement_timeout_does_not_leak_into_the_next_query() {
    let db = TestDb::new("pdb_timeout_noleak").await;
    let gw = provisioned(&db, "notes-plugin").await;

    // Burn a query that trips the timeout, returning its connection to the pool.
    let _ = gw
        .query(
            "notes-plugin",
            "SELECT to_json(count(*)) FROM generate_series(1, 20000000000)",
            vec![],
        )
        .await;

    // The application's own connection must still have no ceiling imposed.
    use sea_orm::FromQueryResult;
    let shown: Vec<sea_orm::JsonValue> = sea_orm::JsonValue::find_by_statement(
        sea_orm::Statement::from_string(sea_orm::DbBackend::Postgres, "SHOW statement_timeout"),
    )
    .all(&db.conn)
    .await
    .expect("SHOW statement_timeout");

    let value = shown[0]["statement_timeout"].as_str().unwrap_or_default();
    assert_eq!(
        value, "0",
        "SET LOCAL must unwind at COMMIT — a pooled connection came back still capped at {value}"
    );

    db.teardown().await;
}

#[tokio::test]
async fn a_read_matching_no_row_returns_json_null_rather_than_an_error() {
    let db = TestDb::new("pdb_nullrow").await;
    let gw = provisioned(&db, "notes-plugin").await;

    let out = gw
        .query(
            "notes-plugin",
            "SELECT to_json(body) FROM notes WHERE id = $1",
            vec![json!(uuid::Uuid::new_v4().to_string())],
        )
        .await
        .expect("select with no match");
    assert_eq!(out, serde_json::Value::Null);

    db.teardown().await;
}

#[tokio::test]
async fn a_writable_cte_is_treated_as_a_read_so_returning_data_comes_back() {
    // `WITH ... AS (INSERT ... RETURNING ...) SELECT ...` is the only way in
    // Postgres to read an INSERT's RETURNING from a subquery, so the gateway
    // routes statements starting with `with` down the value-returning branch.
    let db = TestDb::new("pdb_cte").await;
    let gw = provisioned(&db, "notes-plugin").await;

    let out = gw
        .query(
            "notes-plugin",
            "WITH ins AS (INSERT INTO notes (id, body) VALUES ($1, $2) RETURNING body) \
             SELECT to_json(body) FROM ins",
            vec![json!(uuid::Uuid::new_v4().to_string()), json!("from-cte")],
        )
        .await
        .expect("writable CTE");

    assert_eq!(out, json!("from-cte"));

    db.teardown().await;
}

// ─── layer 1: statement validation (defence-in-depth) ──────────────────────

#[tokio::test]
async fn the_validator_rejects_dangerous_statement_shapes() {
    let db = TestDb::new("pdb_validate").await;
    let gw = provisioned(&db, "notes-plugin").await;

    // Each of these must be refused before any SQL is sent to the server.
    let rejected = [
        ("multiple statements", "SELECT 1; DROP TABLE notes"),
        ("runtime DDL", "CREATE TABLE evil (id int)"),
        ("schema-qualified core table", "SELECT to_json(id) FROM public.users"),
        ("quoted identifier", "SELECT to_json(id) FROM \"public\".users"),
        ("catalog access", "SELECT to_json(count(*)) FROM pg_catalog.pg_tables"),
        ("another plugin's schema", "SELECT to_json(count(*)) FROM plugin_other.secrets"),
        ("search_path tampering", "SET search_path TO public"),
        ("dollar quoting", "SELECT to_json($$public.users$$)"),
    ];

    for (label, sql) in rejected {
        let result = gw.query("notes-plugin", sql, vec![]).await;
        assert!(result.is_err(), "{label} must be rejected, got: {result:?}");
    }

    db.teardown().await;
}

#[tokio::test]
async fn the_validator_looks_past_comments_and_literals() {
    let db = TestDb::new("pdb_validate_strip").await;
    let gw = provisioned(&db, "notes-plugin").await;

    // A blocked keyword appearing only inside a comment or a string literal is
    // inert — it must not trip the denylist, or ordinary plugin SQL breaks.
    let allowed = gw
        .query(
            "notes-plugin",
            "SELECT to_json(count(*)) FROM notes -- public.users lives elsewhere",
            vec![],
        )
        .await;
    assert!(allowed.is_ok(), "a keyword inside a comment is inert: {allowed:?}");

    // Conversely, splitting a keyword with a block comment must not sneak past.
    let blocked = gw
        .query("notes-plugin", "SELECT to_json(id) FROM public./**/users", vec![])
        .await;
    assert!(blocked.is_err(), "a comment must not be usable to break up `public.`");

    db.teardown().await;
}

// ─── layer 2: the privilege drop (the real boundary) ───────────────────────

#[tokio::test]
async fn plugin_sql_executes_as_the_low_privilege_role() {
    // This is the load-bearing assertion in the file. The comment in
    // `plugin_db_repository.rs` claims plugin SQL runs as `ferum_plugin` — a
    // NOLOGIN role holding no grant on any application table — and that this,
    // not the denylist, is what actually stops a plugin reaching `users`.
    //
    // If `SET LOCAL ROLE` ever silently stops taking effect (a refactor, a
    // pooling change, a migration that could not create the role), every other
    // test here would still pass while the only real boundary was gone.
    let db = TestDb::new("pdb_role").await;
    let gw = provisioned(&db, "notes-plugin").await;

    let who = gw
        .query("notes-plugin", "SELECT to_json(current_user)", vec![])
        .await
        .expect("current_user");

    assert_eq!(
        who,
        json!("ferum_plugin"),
        "plugin SQL must run with the privileges dropped; got {who:?}"
    );

    db.teardown().await;
}

#[tokio::test]
async fn the_reduced_role_does_not_leak_into_the_next_query() {
    // `SET LOCAL` unwinds at COMMIT, so a pooled connection must never be handed
    // back still wearing `ferum_plugin`. Verified by using the same connection
    // for an ordinary application query afterwards.
    let db = TestDb::new("pdb_role_unwind").await;
    let gw = provisioned(&db, "notes-plugin").await;

    gw.query("notes-plugin", "SELECT to_json(current_user)", vec![])
        .await
        .expect("plugin query");

    // A core-table read on the same pool would fail if the role had persisted.
    use sea_orm::{ConnectionTrait, DbBackend, Statement};
    let rows = db
        .conn
        .query_all_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT count(*) FROM users".to_owned(),
        ))
        .await;
    assert!(rows.is_ok(), "application access must be restored after the plugin query: {rows:?}");

    db.teardown().await;
}

#[tokio::test]
async fn one_plugin_cannot_reach_another_plugins_table_unqualified() {
    // Two plugins, each with its own `notes` table. `search_path` is pinned per
    // query, so an unqualified `notes` must resolve to the caller's own schema —
    // and the qualified form is refused by the validator (covered above).
    let db = TestDb::new("pdb_isolation").await;
    let a = provisioned(&db, "plugin-a").await;
    let b = provisioned(&db, "plugin-b").await;

    a.query(
        "plugin-a",
        "INSERT INTO notes (id, body) VALUES ($1, 'owned-by-a')",
        vec![json!(uuid::Uuid::new_v4().to_string())],
    )
    .await
    .expect("insert into a");

    // B's table is still empty: it did not see A's row.
    let seen_by_b = b
        .query("plugin-b", "SELECT to_json(count(*)) FROM notes", vec![])
        .await
        .expect("count in b");
    assert_eq!(seen_by_b, json!(0), "plugin-b must not see plugin-a's rows");

    let seen_by_a = a
        .query("plugin-a", "SELECT to_json(count(*)) FROM notes", vec![])
        .await
        .expect("count in a");
    assert_eq!(seen_by_a, json!(1));

    db.teardown().await;
}
