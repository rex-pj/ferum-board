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

/// Defense-in-depth check on plugin-authored SQL. This runs in addition to —
/// never instead of — the forced `search_path` scoping in `query()`. See the
/// trait doc comment for the honest limits of this approach.
fn validate_plugin_sql(sql: &str) -> Result<(), AppError> {
    let trimmed_end = sql.trim_end().trim_end_matches(';');
    if trimmed_end.contains(';') {
        return Err(AppError::invalid("multiple_sql_statements"));
    }

    let lower = sql.to_ascii_lowercase();
    const BLOCKED: &[&str] = &[
        "create ", "drop ", "alter ", "grant ", "revoke ", "truncate ",
        "public.", "pg_catalog", "information_schema", "pg_", "search_path",
    ];
    for kw in BLOCKED {
        if lower.contains(kw) {
            return Err(AppError::forbidden("sql_not_allowed"));
        }
    }
    Ok(())
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
