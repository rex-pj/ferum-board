use sea_orm::sqlx;
use sea_orm::sqlx::AssertSqlSafe;
use sea_orm::DatabaseConnection;

use ferum_application::shared::AppError;
use ferum_domain::repositories::plugin_db_repository::PluginDbGateway;

pub struct PgPluginDbGateway {
    db: DatabaseConnection,
}

impl PgPluginDbGateway {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

/// Wall-clock ceiling on a single `Ferum.db.query`, enforced by Postgres.
///
/// Sized for the workload plugins actually have — a keyed lookup or a small
/// aggregate over their own schema — not for reporting. Raising it raises how
/// long one plugin can hold a connection from a pool that also serves page
/// renders, which is the cost this constant exists to cap.
const PLUGIN_STATEMENT_TIMEOUT_MS: u32 = 2_000;

/// Derive the Postgres schema name for a plugin slug. Slugs are reverse-domain
/// strings (e.g. "com.ferum.simple-chatbox") containing characters invalid in
/// an unquoted identifier, so non-alphanumeric characters are folded to `_`.
fn schema_name(slug: &str) -> String {
    let sanitized: String = slug
        .chars()
        .map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
        .collect();
    format!("plugin_{sanitized}")
}

/// Strips comments and string literals so the denylist below inspects only
/// executable SQL structure.
///
/// Without this, `SELECT 1 /* public. */` trips the filter on an inert comment
/// while `SELECT/**/*/**/FROM x` slips past keyword checks that assume a
/// trailing space. Literal *contents* are never dangerous (they are data, and
/// params are bound separately), so they are replaced wholesale rather than
/// scanned.
fn strip_comments_and_literals(sql: &str) -> String {
    let b: Vec<char> = sql.chars().collect();
    let mut out = String::with_capacity(b.len());
    let mut i = 0;

    // Every read goes through `get`, so the bounds are structural rather than a
    // hand-written `i + 1 < b.len()` beside each one. This is the second layer of
    // the plugin SQL defence (the first is the `ferum_plugin` role), and an
    // off-by-one in a manual guard here is the shape of bug that layer exists to
    // prevent. The rewrite was checked against the index-based original over
    // 400k generated inputs before it landed.
    while let Some(&c) = b.get(i) {
        // Line comment: -- ... EOL
        if c == '-' && b.get(i + 1) == Some(&'-') {
            while b.get(i).is_some_and(|&c| c != '\n') {
                i += 1;
            }
            out.push(' ');
            continue;
        }
        // Block comment: /* ... */ — nestable in Postgres, so track depth.
        if c == '/' && b.get(i + 1) == Some(&'*') {
            let mut depth = 1u32;
            i += 2;
            while depth > 0 {
                match (b.get(i), b.get(i + 1)) {
                    (Some('/'), Some('*')) => {
                        depth += 1;
                        i += 2;
                    }
                    (Some('*'), Some('/')) => {
                        depth -= 1;
                        i += 2;
                    }
                    (Some(_), _) => i += 1,
                    (None, _) => break,
                }
            }
            out.push(' ');
            continue;
        }
        // Single-quoted literal, with '' as the escape for a literal quote.
        if c == '\'' {
            i += 1;
            while let Some(&c) = b.get(i) {
                if c == '\'' {
                    if b.get(i + 1) == Some(&'\'') {
                        i += 2;
                    } else {
                        i += 1;
                        break;
                    }
                } else {
                    i += 1;
                }
            }
            out.push_str("''");
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

/// **Second** layer on plugin SQL, not the primary one — the real boundary is
/// the privilege drop in `query()`, where `ferum_plugin` holds no grant on any
/// application table.
///
/// Kept because it is independent: it still applies when the role could not be
/// created, and rejects DDL and multi-statement input with a clearer error.
/// A gap here is not automatically exploitable — check the role first.
fn validate_plugin_sql(sql: &str) -> Result<(), AppError> {
    let trimmed_end = sql.trim_end().trim_end_matches(';');
    if trimmed_end.contains(';') {
        return Err(AppError::invalid("multiple_sql_statements"));
    }

    let normalized = strip_comments_and_literals(sql);
    let lower = normalized.to_ascii_lowercase();

    // Double-quoted identifiers are refused outright. `search_path` is pinned to
    // the plugin's own schema, so a plugin never needs to quote-qualify anything
    // — while quoting is precisely what defeats a substring denylist:
    // `FROM "public".users` does not contain the string `public.`, so the check
    // below would pass it straight through to the users table.
    if lower.contains('"') {
        return Err(AppError::forbidden("sql_quoted_identifiers_not_allowed"));
    }

    // Dollar-quoting is a second literal syntax and a second way to smuggle text
    // past the scanner above.
    if lower.contains("$$") {
        return Err(AppError::forbidden("sql_not_allowed"));
    }

    const BLOCKED: &[&str] = &[
        "create ", "drop ", "alter ", "grant ", "revoke ", "truncate ",
        "public.", "pg_catalog", "information_schema", "pg_", "search_path",
        // A plugin reaches its OWN tables unqualified through search_path, so an
        // explicit `plugin_*.` qualifier can only be aimed at a different
        // plugin's schema — e.g. reading another plugin's stored API tokens.
        "plugin_",
    ];
    for kw in BLOCKED {
        if lower.contains(kw) {
            return Err(AppError::forbidden("sql_not_allowed"));
        }
    }
    Ok(())
}

/// Opens `schema` to the shared plugin role, now and for tables added later.
///
/// `ALTER DEFAULT PRIVILEGES` is the load-bearing half — `GRANT ON ALL TABLES`
/// covers only what exists when it runs, so without it a plugin cannot read a
/// table its own later migration creates.
///
/// Best-effort: with no role every statement fails harmlessly.
async fn grant_schema_to_plugin_role(pool: &sqlx::PgPool, schema: &str) {
    let role = migration::PLUGIN_DB_ROLE;
    let stmts = [
        format!("GRANT USAGE ON SCHEMA \"{schema}\" TO {role}"),
        format!("GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA \"{schema}\" TO {role}"),
        format!("GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA \"{schema}\" TO {role}"),
        format!(
            "ALTER DEFAULT PRIVILEGES IN SCHEMA \"{schema}\" \
             GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO {role}"
        ),
        format!(
            "ALTER DEFAULT PRIVILEGES IN SCHEMA \"{schema}\" \
             GRANT USAGE, SELECT ON SEQUENCES TO {role}"
        ),
    ];

    // AssertSqlSafe: every interpolation above is server-derived — `schema` comes
    // from `schema_name()` (non-alphanumerics folded to `_`) and `role` is the
    // compile-time `PLUGIN_DB_ROLE` constant. No plugin-supplied text reaches here.
    for stmt in stmts {
        if let Err(e) = sqlx::query(AssertSqlSafe(stmt)).execute(pool).await {
            tracing::warn!(
                schema = %schema,
                error = %e,
                "failed to grant plugin schema to {role}; plugin queries will fall back to \
                 denylist-only enforcement for this schema"
            );
            return;
        }
    }
    tracing::info!(schema = %schema, "granted plugin schema to {role}");
}

/// Drops the transaction down to the low-privilege plugin role.
///
/// `SET LOCAL` is scoped to the transaction and unwinds at COMMIT/ROLLBACK, so
/// the pooled connection is never handed back still wearing the reduced role.
///
/// Returns whether the switch took effect. A `false` here means plugin SQL is
/// about to run with the application's own privileges and the denylist in
/// [`validate_plugin_sql`] is the only thing standing between a plugin and the
/// `users` table — worth a WARN every time, not a silent downgrade.
async fn set_plugin_role(tx: &mut sqlx::PgConnection, slug: &str) -> bool {
    let role = migration::PLUGIN_DB_ROLE;
    // AssertSqlSafe: `role` is the compile-time `PLUGIN_DB_ROLE` constant.
    match sqlx::query(AssertSqlSafe(format!("SET LOCAL ROLE {role}"))).execute(tx).await {
        Ok(_) => true,
        Err(e) => {
            tracing::warn!(
                plugin = %slug,
                error = %e,
                "could not SET LOCAL ROLE {role}; plugin SQL is running with application \
                 privileges, protected only by the statement denylist"
            );
            false
        }
    }
}

#[async_trait::async_trait]
impl PluginDbGateway for PgPluginDbGateway {
    async fn provision_schema(&self, slug: &str, statements: &[String]) -> Result<(), AppError> {
        let schema = schema_name(slug);
        let pool = self.db.get_postgres_connection_pool();

        // AssertSqlSafe: `schema` is `schema_name(slug)` output — non-alphanumerics
        // folded to `_` — so it cannot break out of the quoted identifier.
        sqlx::query(AssertSqlSafe(format!("CREATE SCHEMA IF NOT EXISTS \"{schema}\"")))
            .execute(pool)
            .await
            .map_err(|e| AppError::internal(format!("Failed to create plugin schema: {e}")))?;

        for stmt in statements {
            let mut tx = pool.begin().await
                .map_err(|e| AppError::internal(format!("Failed to begin schema tx: {e}")))?;
            sqlx::query(AssertSqlSafe(format!("SET LOCAL search_path TO \"{schema}\"")))
                .execute(&mut *tx)
                .await
                .map_err(|e| AppError::internal(format!("Failed to scope schema tx: {e}")))?;
            // AssertSqlSafe: plugin-authored schema DDL. It is deliberately trusted
            // *here* — this is the install-time `db` capability an admin granted —
            // and is confined by the `SET LOCAL search_path` above.
            sqlx::query(AssertSqlSafe(stmt.as_str()))
                .execute(&mut *tx)
                .await
                .map_err(|e| AppError::unprocessable(&format!("Schema DDL failed: {e}")))?;
            tx.commit().await
                .map_err(|e| AppError::internal(format!("Failed to commit schema DDL: {e}")))?;
        }

        grant_schema_to_plugin_role(pool, &schema).await;
        Ok(())
    }

    async fn drop_schema(&self, slug: &str) -> Result<(), AppError> {
        let schema = schema_name(slug);
        let pool = self.db.get_postgres_connection_pool();
        // AssertSqlSafe: `schema` is sanitized `schema_name(slug)` output.
        sqlx::query(AssertSqlSafe(format!("DROP SCHEMA IF EXISTS \"{schema}\" CASCADE")))
            .execute(pool)
            .await
            .map_err(|e| AppError::internal(format!("Failed to drop plugin schema: {e}")))?;
        Ok(())
    }

    async fn query(
        &self,
        slug: &str,
        sql: &str,
        params: Vec<serde_json::Value>,
    ) -> Result<serde_json::Value, AppError> {
        validate_plugin_sql(sql)?;

        let schema = schema_name(slug);
        let pool = self.db.get_postgres_connection_pool();
        let mut tx = pool.begin().await
            .map_err(|e| AppError::internal(format!("Failed to begin query tx: {e}")))?;
        // AssertSqlSafe: `schema` is sanitized `schema_name(slug)` output.
        sqlx::query(AssertSqlSafe(format!("SET LOCAL search_path TO \"{schema}\"")))
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::internal(format!("Failed to scope query tx: {e}")))?;

        // THE ONLY thing bounding how long plugin SQL holds this connection.
        // `PluginRegistry`'s tokio timeout cannot help: the plugin runs in
        // `block_on` on a thread outside the runtime, so cancelling the future
        // leaves that thread holding a pooled connection. Postgres abandoning
        // the statement is what closes it.
        //
        // `SET LOCAL` unwinds at COMMIT, so it never leaks to another caller.
        if let Err(e) = sqlx::query(AssertSqlSafe(format!(
            "SET LOCAL statement_timeout = '{PLUGIN_STATEMENT_TIMEOUT_MS}ms'"
        )))
        .execute(&mut *tx)
        .await
        {
            // Failing open here would silently restore the unbounded behaviour
            // this exists to remove, so refuse the query instead.
            return Err(AppError::internal(format!(
                "Failed to bound plugin query time: {e}"
            )));
        }

        // Drop privileges for the duration of this transaction. This — not the
        // denylist above — is what actually stops a plugin reaching
        // `"public".users`: the role holds no grant on any application table, so
        // Postgres refuses the read regardless of how the SQL is spelled. The
        // denylist stays as a second, independent layer.
        //
        // Order matters: `search_path` and the timeout are set first, while
        // still privileged.
        set_plugin_role(&mut tx, slug).await;

        // "with" covers the common `WITH x AS (INSERT ... RETURNING ...) SELECT
        // row_to_json(x) FROM x` pattern — a writable CTE is the only valid way
        // in Postgres to get an INSERT/UPDATE/DELETE's RETURNING into a
        // FROM-subquery, so plugin authors need it to return data too.
        let trimmed_lower = sql.trim_start().to_ascii_lowercase();
        let is_select = trimmed_lower.starts_with("select") || trimmed_lower.starts_with("with");

        let result = if is_select {
            // AssertSqlSafe: plugin-supplied SQL, reaching here only after
            // `validate_plugin_sql` and with `SET LOCAL ROLE ferum_plugin` in
            // force. Values are bound as parameters below, never interpolated.
            let mut q = sqlx::query_scalar::<_, Option<serde_json::Value>>(AssertSqlSafe(sql));
            for p in &params {
                q = bind_scalar_param(q, p);
            }
            let row: Option<Option<serde_json::Value>> = q.fetch_optional(&mut *tx).await
                .map_err(|e| AppError::unprocessable(&format!("Query failed: {e}")))?;
            row.flatten().unwrap_or(serde_json::Value::Null)
        } else {
            // AssertSqlSafe: same contract as the SELECT branch above.
            let mut q = sqlx::query(AssertSqlSafe(sql));
            for p in &params {
                q = bind_exec_param(q, p);
            }
            let res = q.execute(&mut *tx).await
                .map_err(|e| AppError::unprocessable(&format!("Query failed: {e}")))?;
            serde_json::json!({ "rows_affected": res.rows_affected() })
        };

        tx.commit().await
            .map_err(|e| AppError::internal(format!("Failed to commit query tx: {e}")))?;
        Ok(result)
    }
}

type ScalarQuery<'q> = sqlx::query::QueryScalar<'q, sqlx::Postgres, Option<serde_json::Value>, sqlx::postgres::PgArguments>;

fn bind_scalar_param<'q>(q: ScalarQuery<'q>, p: &'q serde_json::Value) -> ScalarQuery<'q> {
    match p {
        // Bind UUID-shaped strings with the actual uuid type, not text. Postgres's
        // extended query protocol sends an explicit type OID for bound params —
        // TEXT for a plain string bind — and does not always implicitly cast that
        // to a `uuid`-typed column/comparison the way a literal in raw SQL would.
        // Every plugin ID this gateway ever hands to JS (actor_id, primary keys
        // from row_to_json results) is a UUID string, so this covers the common
        // case without requiring plugin authors to write `::uuid` casts.
        serde_json::Value::String(s) => match uuid::Uuid::parse_str(s) {
            Ok(id) => q.bind(id),
            Err(_) => q.bind(s),
        },
        serde_json::Value::Number(n) if n.is_i64() => q.bind(n.as_i64()),
        serde_json::Value::Number(n) => q.bind(n.as_f64()),
        serde_json::Value::Bool(b) => q.bind(*b),
        serde_json::Value::Null => q.bind(Option::<String>::None),
        other => q.bind(other.to_string()),
    }
}

type ExecQuery<'q> = sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>;

fn bind_exec_param<'q>(q: ExecQuery<'q>, p: &'q serde_json::Value) -> ExecQuery<'q> {
    match p {
        serde_json::Value::String(s) => match uuid::Uuid::parse_str(s) {
            Ok(id) => q.bind(id),
            Err(_) => q.bind(s),
        },
        serde_json::Value::Number(n) if n.is_i64() => q.bind(n.as_i64()),
        serde_json::Value::Number(n) => q.bind(n.as_f64()),
        serde_json::Value::Bool(b) => q.bind(*b),
        serde_json::Value::Null => q.bind(Option::<String>::None),
        other => q.bind(other.to_string()),
    }
}
