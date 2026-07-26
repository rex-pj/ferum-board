use sea_orm::sqlx;
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

    while i < b.len() {
        // Line comment: -- ... EOL
        if b[i] == '-' && i + 1 < b.len() && b[i + 1] == '-' {
            while i < b.len() && b[i] != '\n' {
                i += 1;
            }
            out.push(' ');
            continue;
        }
        // Block comment: /* ... */ — nestable in Postgres, so track depth.
        if b[i] == '/' && i + 1 < b.len() && b[i + 1] == '*' {
            let mut depth = 1;
            i += 2;
            while i < b.len() && depth > 0 {
                if b[i] == '/' && i + 1 < b.len() && b[i + 1] == '*' {
                    depth += 1;
                    i += 2;
                } else if b[i] == '*' && i + 1 < b.len() && b[i + 1] == '/' {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            out.push(' ');
            continue;
        }
        // Single-quoted literal, with '' as the escape for a literal quote.
        if b[i] == '\'' {
            i += 1;
            while i < b.len() {
                if b[i] == '\'' {
                    if i + 1 < b.len() && b[i + 1] == '\'' {
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
        out.push(b[i]);
        i += 1;
    }
    out
}

/// Defense-in-depth check on plugin-authored SQL. This is the **second** layer,
/// not the primary one.
///
/// The real boundary is the privilege drop in `query()`: plugin SQL executes as
/// `ferum_plugin`, a role holding no grant on any application table, so Postgres
/// refuses `FROM public.users` no matter how the statement is spelled. This
/// denylist survives because it is independent of that — it still applies when
/// the role could not be created (a Postgres user without `CREATEROLE`), and it
/// rejects whole classes of statement (DDL, multiple statements) earlier and
/// with a clearer error than a privilege failure would give.
///
/// Do not reason about it as though it were the only defence, and do not treat
/// a gap in it as automatically exploitable — check whether the role blocks it
/// first.
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

/// Opens `schema` to the shared plugin role, and arranges for tables the plugin
/// creates later to be reachable too.
///
/// `ALTER DEFAULT PRIVILEGES` is the load-bearing half: a plugin that adds a
/// table in a later schema migration would otherwise be unable to read its own
/// new table, because `GRANT ... ON ALL TABLES` only covers tables that exist at
/// the moment it runs.
///
/// Best-effort. When the role is absent (migration could not create it on a
/// restricted Postgres user) every statement here fails harmlessly and
/// [`set_plugin_role`] detects the same condition at query time.
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

    for stmt in stmts {
        if let Err(e) = sqlx::query(&stmt).execute(pool).await {
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
    match sqlx::query(&format!("SET LOCAL ROLE {role}")).execute(tx).await {
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

        sqlx::query(&format!("CREATE SCHEMA IF NOT EXISTS \"{schema}\""))
            .execute(pool)
            .await
            .map_err(|e| AppError::internal(format!("Failed to create plugin schema: {e}")))?;

        for stmt in statements {
            let mut tx = pool.begin().await
                .map_err(|e| AppError::internal(format!("Failed to begin schema tx: {e}")))?;
            sqlx::query(&format!("SET LOCAL search_path TO \"{schema}\""))
                .execute(&mut *tx)
                .await
                .map_err(|e| AppError::internal(format!("Failed to scope schema tx: {e}")))?;
            sqlx::query(stmt)
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
        sqlx::query(&format!("DROP SCHEMA IF EXISTS \"{schema}\" CASCADE"))
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
        sqlx::query(&format!("SET LOCAL search_path TO \"{schema}\""))
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::internal(format!("Failed to scope query tx: {e}")))?;

        // Drop privileges for the duration of this transaction. This — not the
        // denylist above — is what actually stops a plugin reaching
        // `"public".users`: the role holds no grant on any application table, so
        // Postgres refuses the read regardless of how the SQL is spelled. The
        // denylist stays as a second, independent layer.
        //
        // Order matters: `search_path` is set first, while still privileged.
        set_plugin_role(&mut tx, slug).await;

        // "with" covers the common `WITH x AS (INSERT ... RETURNING ...) SELECT
        // row_to_json(x) FROM x` pattern — a writable CTE is the only valid way
        // in Postgres to get an INSERT/UPDATE/DELETE's RETURNING into a
        // FROM-subquery, so plugin authors need it to return data too.
        let trimmed_lower = sql.trim_start().to_ascii_lowercase();
        let is_select = trimmed_lower.starts_with("select") || trimmed_lower.starts_with("with");

        let result = if is_select {
            let mut q = sqlx::query_scalar::<_, Option<serde_json::Value>>(sql);
            for p in &params {
                q = bind_scalar_param(q, p);
            }
            let row: Option<Option<serde_json::Value>> = q.fetch_optional(&mut *tx).await
                .map_err(|e| AppError::unprocessable(&format!("Query failed: {e}")))?;
            row.flatten().unwrap_or(serde_json::Value::Null)
        } else {
            let mut q = sqlx::query(sql);
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
